//! Frames from the camera, through v4l2-ctl (v4l-utils): it negotiates the format and streams raw
//! frames to us, of which the grey (luma) part is used. Infrared cameras first; a normal webcam
//! only when the person switched that on (D37). Tests use a still picture instead (`Still`).

use crate::face::Frame;
use anyhow::{bail, Context, Result};
use std::io::Read;
use std::process::{Child, ChildStdout, Command, Stdio};

pub trait Source: Send {
    fn next(&mut self) -> Result<Frame>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Windows Hello style: a printed photo doesn't look like a face in infrared.
    Infrared,
    /// A normal webcam: a photo can look like a face, so the rules are stricter (D37).
    Webcam,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Infrared => "infrared",
            Kind::Webcam => "webcam",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Camera {
    pub dev: String,
    pub name: String,
    pub kind: Kind,
}

/// Every first capture node (index 0), in order: (device, V4L2 name, infrared?).
fn nodes() -> Vec<(String, String, bool)> {
    let Ok(rd) = std::fs::read_dir("/sys/class/video4linux") else { return Vec::new() };
    let mut nodes: Vec<_> = rd.flatten().collect();
    nodes.sort_by_key(|e| e.file_name());
    nodes
        .into_iter()
        .filter_map(|e| {
            let p = e.path();
            let name = std::fs::read_to_string(p.join("name")).unwrap_or_default().trim().to_string();
            let index = std::fs::read_to_string(p.join("index")).unwrap_or_default().trim().to_string();
            let ir = name.contains("IR") || name.to_lowercase().contains("infrared");
            (index == "0").then(|| (format!("/dev/{}", e.file_name().to_string_lossy()), name, ir))
        })
        .collect()
}

/// The camera face sign-in uses: an infrared one if there is one, else a normal webcam when
/// `webcam` (the person's opt-in) allows it.
pub fn find(webcam: bool) -> Option<Camera> {
    let all = nodes();
    if let Some((dev, name, _)) = all.iter().find(|n| n.2) {
        return Some(Camera { dev: dev.clone(), name: name.clone(), kind: Kind::Infrared });
    }
    if webcam {
        return webcam_name().and_then(|name| all.into_iter().find(|n| n.1 == name)).map(|(dev, name, _)| Camera {
            dev,
            name,
            kind: Kind::Webcam,
        });
    }
    None
}

/// A normal webcam's name, whether or not face sign-in may use it.
pub fn webcam_name() -> Option<String> {
    nodes().into_iter().find(|n| !n.2).map(|n| n.1)
}

pub struct V4l2 {
    child: Child,
    out: ChildStdout,
    width: usize,
    height: usize,
    stride: usize,
    frame_bytes: usize,
    yuyv: bool,
    buf: Vec<u8>,
}

fn fmt_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|l| l.trim().strip_prefix(key)).map(|v| v.trim_start_matches([' ', ':']).trim())
}

impl V4l2 {
    pub fn open(camera: &Camera) -> Result<V4l2> {
        let dev = camera.dev.as_str();
        if camera.kind == Kind::Infrared {
            // Many infrared cameras only light their emitter after a vendor-specific switch.
            if std::path::Path::new("/usr/bin/linux-enable-ir-emitter").exists() {
                let _ = Command::new("/usr/bin/linux-enable-ir-emitter").arg("run").status();
            }
            let _ = Command::new("v4l2-ctl").args(["-d", dev, "--set-fmt-video=pixelformat=GREY"]).status();
        } else {
            // Webcams: uncompressed YUYV at VGA, which nearly every webcam offers; its Y part is grey.
            let _ = Command::new("v4l2-ctl")
                .args(["-d", dev, "--set-fmt-video=width=640,height=480,pixelformat=YUYV"])
                .status();
        }
        let out =
            Command::new("v4l2-ctl").args(["-d", dev, "--get-fmt-video"]).output().context("v4l2-ctl is missing")?;
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let (w, h) =
            fmt_value(&text, "Width/Height").and_then(|v| v.split_once('/')).context("the camera reported no size")?;
        let (width, height): (usize, usize) = (w.trim().parse()?, h.trim().parse()?);
        let pixel = fmt_value(&text, "Pixel Format").unwrap_or_default();
        let yuyv = pixel.contains("YUYV");
        if !yuyv && !pixel.contains("GREY") {
            bail!("The camera sends {pixel}, which Asima OS doesn't read yet");
        }
        let stride: usize =
            fmt_value(&text, "Bytes per Line").and_then(|v| v.parse().ok()).unwrap_or(width * if yuyv { 2 } else { 1 });
        let frame_bytes: usize = fmt_value(&text, "Size Image").and_then(|v| v.parse().ok()).unwrap_or(stride * height);
        let mut child = Command::new("v4l2-ctl")
            .args(["-d", dev, "--stream-mmap=3", "--stream-to=-"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("the camera could not be started")?;
        let out = child.stdout.take().context("no camera stream")?;
        Ok(V4l2 { child, out, width, height, stride, frame_bytes, yuyv, buf: vec![0; frame_bytes] })
    }
}

impl Source for V4l2 {
    fn next(&mut self) -> Result<Frame> {
        self.out.read_exact(&mut self.buf).context("the camera stopped")?;
        let mut data = Vec::with_capacity(self.width * self.height);
        for y in 0..self.height {
            let row = &self.buf[y * self.stride..];
            if self.yuyv {
                data.extend((0..self.width).map(|x| row[x * 2]));
            } else {
                data.extend_from_slice(&row[..self.width]);
            }
        }
        let _ = self.frame_bytes;
        Ok(Frame { width: self.width, height: self.height, channels: 1, data })
    }
}

impl Drop for V4l2 {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Test source: a still grey picture that moves a little each frame, like a person would.
pub struct Still {
    pub frame: Frame,
    pub n: usize,
}

impl Source for Still {
    fn next(&mut self) -> Result<Frame> {
        self.n += 1;
        let dx = (self.n % 7) * 3;
        let f = &self.frame;
        let mut data = vec![0u8; f.width * f.height];
        for y in 0..f.height {
            for x in dx..f.width {
                data[y * f.width + x] = f.data[y * f.width + x - dx];
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
        Ok(Frame { width: f.width, height: f.height, channels: 1, data })
    }
}
