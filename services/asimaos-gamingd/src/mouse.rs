//! "Use a controller as mouse and keyboard" (Asima Gaming): while the person is signed in, a game
//! controller drives the desktop. Right stick: the pointer; right trigger: left click; left
//! trigger: right click; left stick up or down: scrolling; A: Enter; B: Escape; D-pad: the arrow
//! keys. It pauses while Steam runs (Steam then gives its own desktop controls) and while a game
//! runs (GameMode's hook, asimaos-gamemode-hook, marks it), so games have the controller to
//! themselves. The controller is only read, never taken away; a virtual mouse and keyboard
//! (/dev/uinput, open to the person at the computer) does the clicking.
//!
//!   asimaos-gamingd --controller-mouse     run by the user service asimaos-controller-mouse.service

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

// linux/input-event-codes.h
const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_REL: u16 = 0x02;
const EV_ABS: u16 = 0x03;
const SYN_REPORT: u16 = 0;
const REL_X: u16 = 0x00;
const REL_Y: u16 = 0x01;
const REL_WHEEL: u16 = 0x08;
const ABS_Y: u16 = 0x01;
const ABS_Z: u16 = 0x02;
const ABS_RX: u16 = 0x03;
const ABS_RY: u16 = 0x04;
const ABS_RZ: u16 = 0x05;
const ABS_HAT0X: u16 = 0x10;
const ABS_HAT0Y: u16 = 0x11;
const BTN_LEFT: u16 = 0x110;
const BTN_RIGHT: u16 = 0x111;
const BTN_SOUTH: u16 = 0x130;
const BTN_EAST: u16 = 0x131;
const BTN_TL2: u16 = 0x138;
const BTN_TR2: u16 = 0x139;
const BTN_DPAD_UP: u16 = 0x220;
const BTN_DPAD_DOWN: u16 = 0x221;
const BTN_DPAD_LEFT: u16 = 0x222;
const BTN_DPAD_RIGHT: u16 = 0x223;
const KEY_ESC: u16 = 1;
const KEY_ENTER: u16 = 28;
const KEY_UP: u16 = 103;
const KEY_LEFT: u16 = 105;
const KEY_RIGHT: u16 = 106;
const KEY_DOWN: u16 = 108;

// linux/uinput.h and linux/input.h (x86_64)
const UI_DEV_CREATE: u64 = 0x5501;
const UI_DEV_SETUP: u64 = 0x405c_5503; // _IOW('U', 3, struct uinput_setup), 92 bytes
const UI_SET_EVBIT: u64 = 0x4004_5564;
const UI_SET_KEYBIT: u64 = 0x4004_5565;
const UI_SET_RELBIT: u64 = 0x4004_5566;
const EVIOCGABS_BASE: u64 = 0x8018_4540; // _IOR('E', 0x40 + axis, struct input_absinfo)
const NAME: &str = "Asima controller mouse";

/// Pointer speed at full deflection, in pixels per second; a dead zone keeps a resting stick still.
const SPEED: f64 = 1400.0;
const DEAD: f64 = 0.15;
const TICK: Duration = Duration::from_millis(8);

/// One game controller from /proc/bus/input/devices: its event device and name.
#[derive(Debug, PartialEq)]
pub struct Pad {
    pub event: String,
    pub name: String,
}

/// The game controllers among the input devices: a joystick handler and both right-stick axes
/// (that leaves out motion sensors, touchpads and this program's own virtual device).
pub fn pads(devices: &str) -> Vec<Pad> {
    let mut out = Vec::new();
    for block in devices.split("\n\n") {
        let field = |p: &str| block.lines().find_map(|l| l.strip_prefix(p)).map(str::trim);
        let name = field("N: Name=").unwrap_or("").trim_matches('"').to_string();
        let handlers = field("H: Handlers=").unwrap_or("");
        let abs =
            field("B: ABS=").and_then(|h| u64::from_str_radix(h.split_whitespace().last()?, 16).ok()).unwrap_or(0);
        let event = handlers.split_whitespace().find(|h| h.starts_with("event"));
        let joystick = handlers.split_whitespace().any(|h| h.starts_with("js"));
        let right_stick = abs & (1 << ABS_RX) != 0 && abs & (1 << ABS_RY) != 0;
        if let (Some(event), true, true) = (event, joystick, right_stick) {
            if name != NAME {
                out.push(Pad { event: event.to_string(), name });
            }
        }
    }
    out
}

/// A stick position from -1 to 1 with the dead zone taken out and a gentle curve, so small
/// movements are precise and full deflection is fast.
pub fn shape(raw: i32, min: i32, max: i32) -> f64 {
    if max <= min {
        return 0.0;
    }
    let mid = (f64::from(min) + f64::from(max)) / 2.0;
    let half = (f64::from(max) - f64::from(min)) / 2.0;
    let v = ((f64::from(raw) - mid) / half).clamp(-1.0, 1.0);
    if v.abs() <= DEAD {
        return 0.0;
    }
    let s = (v.abs() - DEAD) / (1.0 - DEAD);
    v.signum() * s * s
}

#[derive(Default)]
struct Axis {
    value: i32,
    min: i32,
    max: i32,
}

struct Controller {
    file: File,
    axes: HashMap<u16, Axis>,
    keys: HashMap<u16, bool>,
}

impl Controller {
    fn open(pad: &Pad) -> Result<Controller> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(format!("/dev/input/{}", pad.event))
            .with_context(|| format!("{} can't be read", pad.name))?;
        let mut axes = HashMap::new();
        for code in [ABS_Y, ABS_Z, ABS_RX, ABS_RY, ABS_RZ, ABS_HAT0X, ABS_HAT0Y] {
            let mut info = [0i32; 6]; // value, minimum, maximum, fuzz, flat, resolution
                                      // SAFETY: EVIOCGABS fills exactly one struct input_absinfo (6 × i32).
            let r =
                unsafe { libc::ioctl(file.as_raw_fd(), (EVIOCGABS_BASE + u64::from(code)) as _, info.as_mut_ptr()) };
            if r == 0 {
                axes.insert(code, Axis { value: info[0], min: info[1], max: info[2] });
            }
        }
        Ok(Controller { file, axes, keys: HashMap::new() })
    }

    /// Reads what happened since the last tick. False when the controller is gone.
    fn drain(&mut self) -> bool {
        let mut buf = [0u8; 24 * 64];
        loop {
            match self.file.read(&mut buf) {
                Ok(0) => return false,
                Ok(n) => {
                    for ev in buf[..n].as_chunks::<24>().0 {
                        let kind = u16::from_ne_bytes([ev[16], ev[17]]);
                        let code = u16::from_ne_bytes([ev[18], ev[19]]);
                        let value = i32::from_ne_bytes([ev[20], ev[21], ev[22], ev[23]]);
                        match kind {
                            EV_ABS => {
                                if let Some(a) = self.axes.get_mut(&code) {
                                    a.value = value;
                                }
                            }
                            EV_KEY => {
                                self.keys.insert(code, value != 0);
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => return true,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(_) => return false,
            }
        }
    }

    fn stick(&self, code: u16) -> f64 {
        self.axes.get(&code).map(|a| shape(a.value, a.min, a.max)).unwrap_or(0.0)
    }

    /// A trigger as a button: an analogue trigger past halfway, or its digital button.
    fn trigger(&self, axis: u16, button: u16) -> bool {
        let analogue = self.axes.get(&axis).is_some_and(|a| a.max > a.min && a.value > (a.min + a.max) / 2);
        analogue || self.key(button)
    }

    fn key(&self, code: u16) -> bool {
        self.keys.get(&code).copied().unwrap_or(false)
    }

    fn hat(&self, axis: u16) -> i32 {
        self.axes.get(&axis).map(|a| a.value.signum()).unwrap_or(0)
    }
}

/// The virtual mouse and keyboard.
struct Output {
    file: File,
    held: HashMap<u16, bool>,
}

impl Output {
    fn create() -> Result<Output> {
        let file = OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open("/dev/uinput")
            .context("/dev/uinput can't be opened (is this the person at the computer?)")?;
        let fd = file.as_raw_fd();
        let set = |req: u64, v: u16| -> Result<()> {
            // SAFETY: the UI_SET_* ioctls take an int by value.
            if unsafe { libc::ioctl(fd, req as _, libc::c_int::from(v)) } < 0 {
                bail!("the virtual mouse can't be set up");
            }
            Ok(())
        };
        set(UI_SET_EVBIT, EV_KEY)?;
        set(UI_SET_EVBIT, EV_REL)?;
        for code in [REL_X, REL_Y, REL_WHEEL] {
            set(UI_SET_RELBIT, code)?;
        }
        for code in [BTN_LEFT, BTN_RIGHT, KEY_ENTER, KEY_ESC, KEY_UP, KEY_DOWN, KEY_LEFT, KEY_RIGHT] {
            set(UI_SET_KEYBIT, code)?;
        }
        // struct uinput_setup: struct input_id (bustype, vendor, product, version), name[80],
        // ff_effects_max.
        let mut setup = [0u8; 92];
        setup[0..2].copy_from_slice(&0x06u16.to_ne_bytes()); // BUS_VIRTUAL
        setup[2..4].copy_from_slice(&0x1d6bu16.to_ne_bytes());
        setup[4..6].copy_from_slice(&0x0104u16.to_ne_bytes());
        setup[6..8].copy_from_slice(&1u16.to_ne_bytes());
        setup[8..8 + NAME.len()].copy_from_slice(NAME.as_bytes());
        // SAFETY: UI_DEV_SETUP reads one struct uinput_setup; UI_DEV_CREATE takes no argument.
        unsafe {
            if libc::ioctl(fd, UI_DEV_SETUP as _, setup.as_ptr()) < 0 || libc::ioctl(fd, UI_DEV_CREATE as _) < 0 {
                bail!("the virtual mouse can't be created");
            }
        }
        Ok(Output { file, held: HashMap::new() })
    }

    fn event(buf: &mut Vec<u8>, kind: u16, code: u16, value: i32) {
        buf.extend_from_slice(&[0u8; 16]); // the kernel sets the time
        buf.extend_from_slice(&kind.to_ne_bytes());
        buf.extend_from_slice(&code.to_ne_bytes());
        buf.extend_from_slice(&value.to_ne_bytes());
    }

    fn key(&mut self, buf: &mut Vec<u8>, code: u16, down: bool) {
        if self.held.get(&code).copied().unwrap_or(false) != down {
            self.held.insert(code, down);
            Output::event(buf, EV_KEY, code, i32::from(down));
        }
    }

    fn flush(&mut self, mut buf: Vec<u8>) {
        if !buf.is_empty() {
            Output::event(&mut buf, EV_SYN, SYN_REPORT, 0);
            let _ = self.file.write_all(&buf);
        }
    }

    fn release_all(&mut self) {
        let mut buf = Vec::new();
        let held: Vec<u16> = self.held.iter().filter(|(_, &d)| d).map(|(&c, _)| c).collect();
        for code in held {
            self.key(&mut buf, code, false);
        }
        self.flush(buf);
    }
}

/// Paused while Steam runs (its own desktop controls take over) or a game runs (GameMode).
fn paused() -> bool {
    let game = std::env::var_os("XDG_RUNTIME_DIR")
        .map(|d| PathBuf::from(d).join("asimaos-game-running"))
        .is_some_and(|f| f.exists());
    game || steam_running()
}

fn steam_running() -> bool {
    std::fs::read_dir("/proc")
        .map(|rd| {
            rd.flatten().any(|e| std::fs::read_to_string(e.path().join("comm")).is_ok_and(|c| c.trim() == "steam"))
        })
        .unwrap_or(false)
}

pub fn run() -> Result<()> {
    let mut out = Output::create()?;
    let mut open: HashMap<String, Controller> = HashMap::new();
    let (mut rest_x, mut rest_y, mut scroll) = (0.0f64, 0.0f64, 0.0f64);
    let mut last_scan = Instant::now() - Duration::from_secs(10);
    let mut is_paused = false;
    loop {
        if last_scan.elapsed() >= Duration::from_secs(2) {
            last_scan = Instant::now();
            let list = pads(&std::fs::read_to_string("/proc/bus/input/devices").unwrap_or_default());
            for p in list {
                if !open.contains_key(&p.event) {
                    if let Ok(c) = Controller::open(&p) {
                        eprintln!("asimaos-controller-mouse: using {}", p.name);
                        open.insert(p.event.clone(), c);
                    }
                }
            }
            let now = paused();
            if now && !is_paused {
                out.release_all();
            }
            is_paused = now;
        }
        open.retain(|_, c| c.drain());
        if !is_paused && !open.is_empty() {
            let dt = TICK.as_secs_f64();
            let mut buf = Vec::new();
            let (mut dx, mut dy, mut wheel) = (0.0, 0.0, 0.0);
            let (mut left, mut right, mut enter, mut esc) = (false, false, false, false);
            let (mut up, mut down, mut l, mut r) = (false, false, false, false);
            for c in open.values() {
                dx += c.stick(ABS_RX) * SPEED * dt;
                dy += c.stick(ABS_RY) * SPEED * dt;
                wheel += -c.stick(ABS_Y) * 12.0 * dt; // notches per second at full deflection
                left |= c.trigger(ABS_RZ, BTN_TR2);
                right |= c.trigger(ABS_Z, BTN_TL2);
                enter |= c.key(BTN_SOUTH);
                esc |= c.key(BTN_EAST);
                up |= c.hat(ABS_HAT0Y) < 0 || c.key(BTN_DPAD_UP);
                down |= c.hat(ABS_HAT0Y) > 0 || c.key(BTN_DPAD_DOWN);
                l |= c.hat(ABS_HAT0X) < 0 || c.key(BTN_DPAD_LEFT);
                r |= c.hat(ABS_HAT0X) > 0 || c.key(BTN_DPAD_RIGHT);
            }
            rest_x += dx;
            rest_y += dy;
            scroll += wheel;
            let (mx, my, mw) = (rest_x.trunc(), rest_y.trunc(), scroll.trunc());
            rest_x -= mx;
            rest_y -= my;
            scroll -= mw;
            if mx != 0.0 {
                Output::event(&mut buf, EV_REL, REL_X, mx as i32);
            }
            if my != 0.0 {
                Output::event(&mut buf, EV_REL, REL_Y, my as i32);
            }
            if mw != 0.0 {
                Output::event(&mut buf, EV_REL, REL_WHEEL, mw as i32);
            }
            for (code, down_now) in [
                (BTN_LEFT, left),
                (BTN_RIGHT, right),
                (KEY_ENTER, enter),
                (KEY_ESC, esc),
                (KEY_UP, up),
                (KEY_DOWN, down),
                (KEY_LEFT, l),
                (KEY_RIGHT, r),
            ] {
                out.key(&mut buf, code, down_now);
            }
            out.flush(buf);
        }
        std::thread::sleep(TICK);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_game_controllers_only() {
        let devices = "I: Bus=0003 Vendor=045e Product=028e Version=0110\nN: Name=\"Microsoft X-Box 360 pad\"\n\
                       H: Handlers=event5 js0 \nB: EV=20000b\nB: ABS=3003f\n\n\
                       I: Bus=0005 Vendor=054c Product=0ce6 Version=8100\nN: Name=\"DualSense Wireless Controller Motion Sensors\"\n\
                       H: Handlers=event7 \nB: ABS=3f\n\n\
                       I: Bus=0011 Vendor=0001 Product=0001 Version=ab41\nN: Name=\"AT Translated Set 2 keyboard\"\n\
                       H: Handlers=sysrq kbd event0 \nB: EV=120013\n\n\
                       I: Bus=0006 Vendor=1d6b Product=0104 Version=0001\nN: Name=\"Asima controller mouse\"\n\
                       H: Handlers=event9 js1 \nB: ABS=3003f\n";
        assert_eq!(pads(devices), vec![Pad { event: "event5".into(), name: "Microsoft X-Box 360 pad".into() }]);
    }

    #[test]
    fn stick_shape() {
        assert_eq!(shape(0, -32768, 32767), 0.0); // centred
        assert_eq!(shape(3000, -32768, 32767), 0.0); // inside the dead zone
        assert!((shape(32767, -32768, 32767) - 1.0).abs() < 0.001);
        assert!((shape(-32768, -32768, 32767) + 1.0).abs() < 0.001);
        let half = shape(16384, -32768, 32767);
        assert!(half > 0.0 && half < 0.5, "{half}"); // gentle at small deflection
        assert_eq!(shape(5, 0, 0), 0.0); // no range: no movement
    }
}
