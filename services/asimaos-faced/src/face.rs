//! Face detection (YuNet) and recognition (SFace), both open models from OpenCV Zoo, run with
//! tract (pure Rust). Frames are grey (infrared cameras give grey pictures) or RGB (tests).

use anyhow::{Context, Result};
use std::path::Path;
use tract_onnx::prelude::*;

pub const DETECT_SIZE: usize = 640; // YuNet 2023mar's declared input
const SCORE_MIN: f32 = 0.8;
const NMS_IOU: f32 = 0.3;

/// SFace's reference landmarks in its 112×112 input: eyes, nose tip, mouth corners.
const REFERENCE: [(f32, f32); 5] =
    [(38.2946, 51.6963), (73.5318, 51.5014), (56.0252, 71.7366), (41.5493, 92.3655), (70.7299, 92.2041)];

/// An image: `channels` is 1 (grey) or 3 (RGB), row-major.
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub channels: usize,
    pub data: Vec<u8>,
}

impl Frame {
    fn rgb(&self, x: usize, y: usize) -> [f32; 3] {
        let i = (y * self.width + x) * self.channels;
        if self.channels == 1 {
            let v = self.data[i] as f32;
            [v, v, v]
        } else {
            [self.data[i] as f32, self.data[i + 1] as f32, self.data[i + 2] as f32]
        }
    }

    /// Bilinear sample; outside the picture is black.
    fn sample(&self, x: f32, y: f32) -> [f32; 3] {
        if x < 0.0 || y < 0.0 || x > (self.width - 1) as f32 || y > (self.height - 1) as f32 {
            return [0.0; 3];
        }
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let (a, b, c, d) = (self.rgb(x0, y0), self.rgb(x1, y0), self.rgb(x0, y1), self.rgb(x1, y1));
        let mut out = [0.0; 3];
        for k in 0..3 {
            out[k] = (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy;
        }
        out
    }
}

#[derive(Debug, Clone)]
pub struct Face {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub score: f32,
    pub landmarks: [(f32, f32); 5], // right eye, left eye, nose, right mouth, left mouth (image coordinates)
}

type Plan = SimplePlan<TypedFact, Box<dyn TypedOp>, Graph<TypedFact, Box<dyn TypedOp>>>;

pub struct Models {
    detector: Plan,
    recognizer: Plan,
}

impl Models {
    pub fn load(dir: &Path) -> Result<Models> {
        let detector = tract_onnx::onnx()
            .model_for_path(dir.join("face_detection_yunet_2023mar.onnx"))
            .context("the face detection model is missing")?
            .with_input_fact(0, f32::fact([1, 3, DETECT_SIZE, DETECT_SIZE]).into())?
            .into_optimized()?
            .into_runnable()?;
        let recognizer = tract_onnx::onnx()
            .model_for_path(dir.join("face_recognition_sface_2021dec.onnx"))
            .context("the face recognition model is missing")?
            .with_input_fact(0, f32::fact([1, 3, 112, 112]).into())?
            .into_optimized()?
            .into_runnable()?;
        Ok(Models { detector, recognizer })
    }

    /// Faces in `frame`, best first.
    pub fn detect(&self, frame: &Frame) -> Result<Vec<Face>> {
        // Letterbox into DETECT_SIZE × DETECT_SIZE (top-left), BGR, 0–255 as YuNet expects.
        let scale = DETECT_SIZE as f32 / frame.width.max(frame.height) as f32;
        let n = DETECT_SIZE;
        let mut input = tract_ndarray::Array4::<f32>::zeros((1, 3, n, n));
        let (sw, sh) = (((frame.width as f32) * scale) as usize, ((frame.height as f32) * scale) as usize);
        for y in 0..sh.min(n) {
            for x in 0..sw.min(n) {
                let p = frame.sample(x as f32 / scale, y as f32 / scale);
                input[[0, 0, y, x]] = p[2];
                input[[0, 1, y, x]] = p[1];
                input[[0, 2, y, x]] = p[0];
            }
        }
        let out = self.detector.run(tvec!(Tensor::from(input).into()))?;
        let view = |i: usize| -> Result<Vec<f32>> { Ok(out[i].to_array_view::<f32>()?.iter().copied().collect()) };

        let mut faces = Vec::new();
        for (k, stride) in [8usize, 16, 32].iter().enumerate() {
            let (cls, obj, bbox, kps) = (view(k)?, view(3 + k)?, view(6 + k)?, view(9 + k)?);
            let cols = n / stride;
            for idx in 0..cls.len() {
                let score = (cls[idx].clamp(0.0, 1.0) * obj[idx].clamp(0.0, 1.0)).sqrt();
                if score < SCORE_MIN {
                    continue;
                }
                let (r, c) = ((idx / cols) as f32, (idx % cols) as f32);
                let s = *stride as f32;
                let cx = (c + bbox[idx * 4]) * s;
                let cy = (r + bbox[idx * 4 + 1]) * s;
                let w = bbox[idx * 4 + 2].exp() * s;
                let h = bbox[idx * 4 + 3].exp() * s;
                let mut landmarks = [(0.0, 0.0); 5];
                for (j, l) in landmarks.iter_mut().enumerate() {
                    *l = (((kps[idx * 10 + 2 * j] + c) * s) / scale, ((kps[idx * 10 + 2 * j + 1] + r) * s) / scale);
                }
                faces.push(Face {
                    x: (cx - w / 2.0) / scale,
                    y: (cy - h / 2.0) / scale,
                    w: w / scale,
                    h: h / scale,
                    score,
                    landmarks,
                });
            }
        }
        // Non-maximum suppression: keep the best of overlapping boxes.
        faces.sort_by(|a, b| b.score.total_cmp(&a.score));
        let mut kept: Vec<Face> = Vec::new();
        for f in faces {
            if kept.iter().all(|k| iou(k, &f) < NMS_IOU) {
                kept.push(f);
            }
        }
        Ok(kept)
    }

    /// The 128-number faceprint of `face`, unit length.
    pub fn embed(&self, frame: &Frame, face: &Face) -> Result<Vec<f32>> {
        // Similarity transform (scale, rotation, shift) from the landmarks to SFace's reference.
        let src = face.landmarks;
        let (msx, msy) = mean(&src);
        let (mdx, mdy) = mean(&REFERENCE);
        let (mut num_a, mut num_b, mut den) = (0.0f32, 0.0f32, 0.0f32);
        for i in 0..5 {
            let (xs, ys) = (src[i].0 - msx, src[i].1 - msy);
            let (xd, yd) = (REFERENCE[i].0 - mdx, REFERENCE[i].1 - mdy);
            num_a += xs * xd + ys * yd;
            num_b += xs * yd - ys * xd;
            den += xs * xs + ys * ys;
        }
        let (a, b) = (num_a / den.max(1e-6), num_b / den.max(1e-6));
        let (tx, ty) = (mdx - (a * msx - b * msy), mdy - (b * msx + a * msy));
        let det = a * a + b * b;
        let mut input = tract_ndarray::Array4::<f32>::zeros((1, 3, 112, 112));
        for v in 0..112 {
            for u in 0..112 {
                // Inverse transform: where in the camera frame this output pixel comes from.
                let (du, dv) = (u as f32 - tx, v as f32 - ty);
                let x = (a * du + b * dv) / det;
                let y = (-b * du + a * dv) / det;
                let p = frame.sample(x, y);
                for ch in 0..3 {
                    input[[0, ch, v, u]] = p[ch]; // RGB, 0–255
                }
            }
        }
        let out = self.recognizer.run(tvec!(Tensor::from(input).into()))?;
        let mut e: Vec<f32> = out[0].to_array_view::<f32>()?.iter().copied().collect();
        let norm = e.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-6);
        e.iter_mut().for_each(|v| *v /= norm);
        Ok(e)
    }
}

fn mean(p: &[(f32, f32); 5]) -> (f32, f32) {
    (p.iter().map(|q| q.0).sum::<f32>() / 5.0, p.iter().map(|q| q.1).sum::<f32>() / 5.0)
}

fn iou(a: &Face, b: &Face) -> f32 {
    let (x1, y1) = (a.x.max(b.x), a.y.max(b.y));
    let (x2, y2) = ((a.x + a.w).min(b.x + b.w), (a.y + a.h).min(b.y + b.h));
    let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    inter / (a.w * a.h + b.w * b.h - inter).max(1e-6)
}

/// Cosine similarity of two unit-length faceprints.
pub fn similarity(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// How far the head is turned sideways: the nose tip's offset from between the eyes, in eye
/// distances. About 0 looking straight at the camera, ±0.3 and more turned.
pub fn yaw(f: &Face) -> f32 {
    let [right, left, nose, _, _] = f.landmarks;
    let between = (right.0 + left.0) / 2.0;
    (nose.0 - between) / (left.0 - right.0).abs().max(1.0)
}

/// How far the head is tilted up or down: the nose tip's height between the eyes and the mouth
/// (about 0.5 to 0.6 looking straight; lower looking up, higher looking down).
pub fn pitch(f: &Face) -> f32 {
    let [right, left, nose, mouth_r, mouth_l] = f.landmarks;
    let eyes = (right.1 + left.1) / 2.0;
    let mouth = (mouth_r.1 + mouth_l.1) / 2.0;
    (nose.1 - eyes) / (mouth - eyes).max(1.0)
}

/// How open the eyes look: the vertical contrast of a small patch around each eye, along the eye
/// line, in units of the patch's own brightness. An open eye shows the dark iris and the lid lines;
/// a closed one is mostly smooth skin. A heuristic (YuNet gives the eyes' centres, not the lids):
/// to be tuned on real cameras.
pub fn eye_openness(frame: &Frame, f: &Face) -> f32 {
    const N: usize = 12;
    let [right, left, ..] = f.landmarks;
    let (dx, dy) = (left.0 - right.0, left.1 - right.1);
    let dist = (dx * dx + dy * dy).sqrt().max(1.0);
    let (c, s) = (dx / dist, dy / dist);
    let half = dist * 0.18;
    let mut total = 0.0;
    for eye in [right, left] {
        let mut v = [[0f32; N]; N];
        for (j, row) in v.iter_mut().enumerate() {
            for (i, px) in row.iter_mut().enumerate() {
                let u = (i as f32 / (N - 1) as f32 * 2.0 - 1.0) * half;
                let w = (j as f32 / (N - 1) as f32 * 2.0 - 1.0) * half * 0.7; // eyes are wider than tall
                let p = frame.sample(eye.0 + u * c - w * s, eye.1 + u * s + w * c);
                *px = (p[0] + p[1] + p[2]) / 3.0;
            }
        }
        let mean = v.iter().flatten().sum::<f32>() / (N * N) as f32;
        let edges: f32 =
            (0..N - 1).flat_map(|j| (0..N).map(move |i| (j, i))).map(|(j, i)| (v[j + 1][i] - v[j][i]).abs()).sum();
        total += edges / ((N - 1) * N) as f32 / (mean + 8.0);
    }
    total / 2.0
}

/// Counts blinks in a stream of eye-openness values: the eyes look clearly less open than usual
/// for one to ten frames, then open again.
#[derive(Default)]
pub struct Blinks {
    usual: f32,
    seen: u32,
    closed: u32,
    pub count: u32,
}

const CLOSED: f32 = 0.6; // of the usual openness
const OPEN: f32 = 0.85;

impl Blinks {
    pub fn see(&mut self, openness: f32) -> u32 {
        if self.seen < 3 {
            self.usual = self.usual.max(openness);
            self.seen += 1;
            return self.count;
        }
        if openness < self.usual * CLOSED {
            self.closed += 1;
        } else if openness >= self.usual * OPEN {
            if (1..=10).contains(&self.closed) {
                self.count += 1;
            }
            self.closed = 0;
            self.usual = self.usual * 0.9 + openness * 0.1; // follows slow changes of light
        }
        self.count
    }
}

/// When a stream of frames counts as the person: with an infrared camera, two matching frames in a
/// row. With a normal webcam (D47) two matching frames at the stricter threshold, and two blinks
/// while the same person looks at it: a photo held up can't blink.
pub struct Liveness {
    webcam: bool,
    streak: u32,
    straight: bool,
    blinks: Blinks,
}

pub const STRAIGHT: f32 = 0.15;
pub const TURNED: f32 = 0.22;
/// Below this, the face in front of the camera is someone else: the blinks start again.
const SOMEONE_ELSE: f32 = 0.25;

impl Liveness {
    pub fn new(webcam: bool) -> Self {
        Liveness { webcam, streak: 0, straight: false, blinks: Blinks::default() }
    }

    /// One frame's best similarity and eye openness; true once the person is proven.
    pub fn see(&mut self, similarity: f32, strict: f32, openness: f32) -> bool {
        if !self.webcam {
            self.streak = if similarity >= strict { self.streak + 1 } else { 0 };
            return self.streak >= 2;
        }
        if similarity >= strict {
            self.streak += 1;
            if self.streak >= 2 {
                self.straight = true;
            }
        } else {
            self.streak = 0;
        }
        if similarity < SOMEONE_ELSE {
            self.blinks = Blinks::default();
            self.straight = false;
        } else {
            self.blinks.see(openness);
        }
        self.straight && self.blinks.count >= 2
    }
}

/// Setting up a face (Asima Manager): look straight, turn the head to one side and the other, nod
/// up and down, then blink three times. Faceprints from every pose are kept, so the face is known
/// from more angles, and the turns and blinks show it's a person.
pub struct EnrollGuide {
    step: usize,
    held: u32,
    usual_pitch: f32,
    side: f32,
    nod: f32,
    blinks: Blinks,
}

const STEPS: [&str; 6] = ["straight", "turn", "turn-other", "nod", "nod-other", "blink"];
const NODDED: f32 = 0.12;

impl Default for EnrollGuide {
    fn default() -> Self {
        EnrollGuide { step: 0, held: 0, usual_pitch: 0.0, side: 0.0, nod: 0.0, blinks: Blinks::default() }
    }
}

impl EnrollGuide {
    pub fn done(&self) -> bool {
        self.step >= STEPS.len()
    }

    pub fn progress(&self) -> f64 {
        self.step as f64 / STEPS.len() as f64
    }

    /// What the person is asked to do now.
    pub fn hint(&self) -> &'static str {
        STEPS.get(self.step).copied().unwrap_or("hold")
    }

    /// One frame with a face; true when its faceprint should be kept.
    pub fn see(&mut self, yaw: f32, pitch: f32, openness: f32) -> bool {
        let keep = match self.hint() {
            "straight" if yaw.abs() < STRAIGHT => {
                self.usual_pitch = (self.usual_pitch * self.held as f32 + pitch) / (self.held + 1) as f32;
                self.held += 1;
                self.held >= 4
            }
            "turn" if yaw.abs() >= TURNED => {
                self.side = yaw.signum();
                true
            }
            "turn-other" => yaw * self.side <= -TURNED,
            "nod" if (pitch - self.usual_pitch).abs() >= NODDED => {
                self.nod = (pitch - self.usual_pitch).signum();
                true
            }
            "nod-other" => (pitch - self.usual_pitch) * self.nod <= -NODDED,
            "blink" => {
                if yaw.abs() < STRAIGHT {
                    self.blinks.see(openness);
                }
                self.blinks.count >= 3
            }
            _ => false,
        };
        if keep {
            self.step += 1;
            self.held = 0;
        }
        // Keep looks that show the face well: straight ones and the turned and nodded ones.
        keep || (self.hint() == "straight" && yaw.abs() < STRAIGHT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webcam_needs_a_match_and_two_blinks() {
        // Infrared: two good frames in a row.
        let mut ir = Liveness::new(false);
        assert!(!ir.see(0.6, 0.45, 1.0));
        assert!(ir.see(0.6, 0.45, 1.0));
        // A photo in front of a webcam: a perfect match that never blinks.
        let mut photo = Liveness::new(true);
        assert!((0..50).all(|_| !photo.see(0.9, 0.55, 1.0)));
        // A person: looks, blinks twice (the eyes close for a few frames each time).
        let mut person = Liveness::new(true);
        let eyes = [1.0, 1.0, 1.0, 1.0, 0.3, 0.3, 1.0, 1.0, 0.35, 1.0];
        let proven: Vec<bool> = eyes.iter().map(|&o| person.see(0.62, 0.55, o)).collect();
        assert_eq!(proven.last(), Some(&true));
        assert!(!proven[..proven.len() - 1].iter().any(|&p| p));
        // Eyes shut for a long time aren't a blink.
        let mut shut = Liveness::new(true);
        let long: Vec<f32> = [1.0; 4].into_iter().chain([0.2; 20]).chain([1.0; 3]).collect();
        assert!(!long.iter().any(|&o| shut.see(0.62, 0.55, o)));
        // Someone else in between starts the blinks again.
        let mut swap = Liveness::new(true);
        for o in [1.0, 1.0, 1.0, 1.0, 0.3, 1.0] {
            swap.see(0.62, 0.55, o);
        }
        swap.see(0.1, 0.55, 1.0);
        assert!(![1.0, 1.0, 1.0, 0.3, 1.0].iter().any(|&o| swap.see(0.62, 0.55, o)));
    }

    #[test]
    fn guided_enrolment() {
        let mut g = EnrollGuide::default();
        assert_eq!(g.hint(), "straight");
        for _ in 0..4 {
            g.see(0.0, 0.55, 1.0);
        }
        assert_eq!(g.hint(), "turn");
        assert!(!g.see(0.1, 0.55, 1.0));
        g.see(-0.3, 0.55, 1.0);
        assert_eq!(g.hint(), "turn-other");
        assert!(!g.see(-0.3, 0.55, 1.0)); // the same side again isn't the other one
        g.see(0.3, 0.55, 1.0);
        assert_eq!(g.hint(), "nod");
        g.see(0.0, 0.70, 1.0); // down
        assert_eq!(g.hint(), "nod-other");
        g.see(0.0, 0.40, 1.0); // up
        assert_eq!(g.hint(), "blink");
        for o in [1.0, 1.0, 1.0, 1.0, 0.3, 1.0, 0.3, 1.0, 0.3, 1.0] {
            g.see(0.0, 0.55, o);
        }
        assert!(g.done() && g.progress() == 1.0);
    }

    #[test]
    fn open_eyes_have_more_contrast() {
        // A grey face picture with dark irises (open) or without (closed).
        let face = Face {
            x: 40.0,
            y: 40.0,
            w: 120.0,
            h: 140.0,
            score: 0.9,
            landmarks: [(70.0, 90.0), (130.0, 90.0), (100.0, 120.0), (75.0, 150.0), (125.0, 150.0)],
        };
        let draw = |open: bool| {
            let mut data = vec![170u8; 200 * 200];
            if open {
                for (cx, cy) in [(70i32, 90i32), (130, 90)] {
                    for y in -4..=4i32 {
                        for x in -6..=6i32 {
                            if x * x * 4 + y * y * 9 <= 144 {
                                data[((cy + y) * 200 + cx + x) as usize] = 40;
                            }
                        }
                    }
                }
            }
            Frame { width: 200, height: 200, channels: 1, data }
        };
        let (open, closed) = (eye_openness(&draw(true), &face), eye_openness(&draw(false), &face));
        assert!(open > closed * 3.0, "open {open} closed {closed}");
        assert!(pitch(&face) > 0.4 && pitch(&face) < 0.6);
    }

    fn models() -> Option<Models> {
        let dir = std::env::var("ASIMAOS_FACE_MODELS").ok()?;
        Some(Models::load(Path::new(&dir)).expect("models load"))
    }

    fn photo(name: &str) -> Frame {
        let dir = std::env::var("ASIMAOS_FACE_MODELS").unwrap();
        let img = image::open(Path::new(&dir).join(name)).expect("test photo").to_rgb8();
        Frame { width: img.width() as usize, height: img.height() as usize, channels: 3, data: img.into_raw() }
    }

    fn grey(f: &Frame) -> Frame {
        let data =
            f.data.chunks(3).map(|p| ((p[0] as u32 * 30 + p[1] as u32 * 59 + p[2] as u32 * 11) / 100) as u8).collect();
        Frame { width: f.width, height: f.height, channels: 1, data }
    }

    fn shifted(f: &Frame, dx: usize) -> Frame {
        let mut data = vec![0u8; f.data.len()];
        for y in 0..f.height {
            for x in dx..f.width {
                for c in 0..f.channels {
                    data[(y * f.width + x) * f.channels + c] = f.data[(y * f.width + x - dx) * f.channels + c];
                }
            }
        }
        Frame { width: f.width, height: f.height, channels: f.channels, data }
    }

    #[test]
    fn finds_and_tells_faces_apart() {
        let Some(m) = models() else {
            eprintln!("ASIMAOS_FACE_MODELS not set: skipped");
            return;
        };
        let lena = grey(&photo("lena.jpg"));
        let messi = grey(&photo("messi5.jpg"));
        let fl = m.detect(&lena).unwrap();
        let fm = m.detect(&messi).unwrap();
        assert!(!fl.is_empty() && !fm.is_empty(), "a face in each photo");
        let a = m.embed(&lena, &fl[0]).unwrap();
        let moved = shifted(&lena, 23);
        let a2 = m.embed(&moved, &m.detect(&moved).unwrap()[0]).unwrap();
        let b = m.embed(&messi, &fm[0]).unwrap();
        let same = similarity(&a, &a2);
        let other = similarity(&a, &b);
        eprintln!("same person {same:.3}, different person {other:.3}");
        assert!(same > 0.8, "the same face, moved, matches");
        assert!(other < 0.36, "a different face doesn't match");
    }
}
