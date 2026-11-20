//! Flashes the Cavway X1 over its USB cable, and checks what is on it. The protocol is
//! `cave_instrument::cavway_flash` (the same code CaveCAD can use over Bluetooth); this is the serial port around it.
//!
//!   x1flash ports                       the X1's USB serial ports
//!   x1flash info  [--port P]            app version, bootloader, serial; or "boot mode"
//!   x1flash inspect FILE.bin            what an update file is
//!   x1flash flash FILE.bin --yes [--port P]   flash it (the X1 must be in boot mode)
//!   x1flash flash FILE.bin --simulate   the whole flash against a pretend X1, nothing sent
//!   x1flash off   [--port P]            power the X1 off (also leaves boot mode)
//!   x1flash scan start|stop|status      CaveCAD firmware (-cc) scan control
//!   x1flash clock [--port P]            set the X1's clock to this Mac's local time
//!
//! `--json` prints one JSON object per line instead of text (the Mac app reads these).

use cave_instrument::cavway::{self, parse_reply, query_frame};
use cave_instrument::cavway_flash::{self as fl, Flasher, Image, SimBootloader, Step};
use std::time::{Duration, Instant};

// ---------- serial port (raw termios, 115200 8N1) ----------

struct Serial {
    fd: i32,
}

impl Serial {
    fn open(path: &str) -> Result<Serial, String> {
        let c = std::ffi::CString::new(path).map_err(|e| e.to_string())?;
        // SAFETY: plain POSIX calls on a descriptor we own.
        unsafe {
            let fd = libc::open(c.as_ptr(), libc::O_RDWR | libc::O_NOCTTY | libc::O_NONBLOCK);
            if fd < 0 {
                let e = std::io::Error::last_os_error();
                if e.raw_os_error() == Some(libc::EBUSY) {
                    return Err(format!(
                        "{path} is busy: another program (X1 Flasher?) has the X1 open"
                    ));
                }
                return Err(format!("cannot open {path}: {e}"));
            }
            // exclusive: while this runs, nothing else (the Mac app's 3 s check, a second x1flash) can open the
            // port and flush or interleave with the bootloader's answers
            if libc::ioctl(fd, libc::TIOCEXCL as _) != 0 {
                libc::close(fd);
                return Err(format!("{path}: could not get exclusive use"));
            }
            let mut t: libc::termios = std::mem::zeroed();
            libc::tcgetattr(fd, &mut t);
            libc::cfmakeraw(&mut t);
            libc::cfsetspeed(&mut t, libc::B115200);
            t.c_cflag |= libc::CLOCAL | libc::CREAD;
            t.c_cflag &= !libc::CRTSCTS;
            libc::tcsetattr(fd, libc::TCSANOW, &t);
            // the USB bridge needs a moment after open before the first bytes get through
            std::thread::sleep(Duration::from_millis(300));
            libc::tcflush(fd, libc::TCIOFLUSH);
            Ok(Serial { fd })
        }
    }
}

impl Drop for Serial {
    fn drop(&mut self) {
        // SAFETY: closing our own descriptor once.
        unsafe { libc::close(self.fd) };
    }
}

/// Moves bytes to and from an X1 (the cable, or the pretend one).
trait Wire {
    fn send(&mut self, bytes: &[u8]) -> Result<(), String>;
    /// Whatever arrives within `ms` (returns as soon as something has).
    fn recv(&mut self, ms: u32) -> Vec<u8>;
    fn discard(&mut self);
}

impl Wire for Serial {
    fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        let mut off = 0;
        while off < bytes.len() {
            // SAFETY: writing from a live slice.
            let n =
                unsafe { libc::write(self.fd, bytes[off..].as_ptr().cast(), bytes.len() - off) };
            if n < 0 {
                let e = std::io::Error::last_os_error();
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                }
                return Err(format!("write failed: {e}"));
            }
            off += n as usize;
        }
        // SAFETY: waits for the bytes to leave.
        unsafe { libc::tcdrain(self.fd) };
        Ok(())
    }

    fn recv(&mut self, ms: u32) -> Vec<u8> {
        let mut p = libc::pollfd {
            fd: self.fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one pollfd we own.
        let r = unsafe { libc::poll(&mut p, 1, ms as i32) };
        if r <= 0 {
            return Vec::new();
        }
        let mut buf = [0u8; 512];
        // SAFETY: reading into our own buffer.
        let n = unsafe { libc::read(self.fd, buf.as_mut_ptr().cast(), buf.len()) };
        if n <= 0 {
            Vec::new()
        } else {
            buf[..n as usize].to_vec()
        }
    }

    fn discard(&mut self) {
        // SAFETY: flushes our own descriptor's input.
        unsafe { libc::tcflush(self.fd, libc::TCIFLUSH) };
    }
}

/// A pretend X1 in boot mode, for `--simulate`.
struct Sim {
    x1: SimBootloader,
    pending: Vec<u8>,
}

impl Wire for Sim {
    fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        let a = self.x1.receive(bytes);
        self.pending.extend(a);
        Ok(())
    }
    fn recv(&mut self, _ms: u32) -> Vec<u8> {
        std::mem::take(&mut self.pending)
    }
    fn discard(&mut self) {
        self.pending.clear();
    }
}

// ---------- output ----------

struct Out {
    json: bool,
}

fn esc(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect(),
            '\n' => "\\n".chars().collect(),
            c if (c as u32) < 0x20 => format!("\\u{:04x}", c as u32).chars().collect(),
            c => vec![c],
        })
        .collect()
}

impl Out {
    /// `fields` are already-JSON values keyed by name; `text` is the human line.
    fn emit(&self, event: &str, fields: &[(&str, String)], text: &str) {
        if self.json {
            let body: Vec<String> = std::iter::once(format!("\"event\":\"{event}\""))
                .chain(fields.iter().map(|(k, v)| format!("\"{k}\":{v}")))
                .collect();
            println!("{{{}}}", body.join(","));
        } else if !text.is_empty() {
            println!("{text}");
        }
    }
    fn error(&self, msg: &str) -> ! {
        self.emit(
            "error",
            &[("message", format!("\"{}\"", esc(msg)))],
            &format!("error: {msg}"),
        );
        std::process::exit(1);
    }
}

fn s(v: &str) -> String {
    format!("\"{}\"", esc(v))
}

// ---------- device ----------

fn ports() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir("/dev")
        .map(|d| {
            d.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with("cu.usbmodem") || n.starts_with("cu.wchusbserial"))
                .map(|n| format!("/dev/{n}"))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// Sends `frame` and collects answer bytes until `done` says the answer is complete, or `ms` passes.
fn ask(
    w: &mut dyn Wire,
    frame: &[u8],
    ms: u32,
    done: impl Fn(&[u8]) -> bool,
) -> Result<Vec<u8>, String> {
    w.discard();
    w.send(frame)?;
    let end = Instant::now() + Duration::from_millis(u64::from(ms));
    let mut buf = Vec::new();
    while Instant::now() < end {
        let left = end.saturating_duration_since(Instant::now()).as_millis() as u32;
        buf.extend(w.recv(left.clamp(1, 100)));
        if done(&buf) {
            break;
        }
    }
    Ok(buf)
}

/// An app-mode memory read (`3d`), or `None` if nothing sensible came back.
fn read_mem(w: &mut dyn Wire, addr: u16, n: usize) -> Result<Option<Vec<u8>>, String> {
    let q = query_frame(addr, n).ok_or("bad read size")?;
    let head = [0x3d, (addr & 0xff) as u8, (addr >> 8) as u8, n as u8];
    let find = |b: &[u8]| b.windows(4).position(|x| x == head);
    let buf = ask(w, &q, 800, |b| {
        find(b).is_some_and(|i| b.len() >= i + 4 + n)
    })?;
    Ok(find(&buf)
        .and_then(|i| parse_reply(&buf[i..]))
        .map(|r| r.data))
}

enum Mode {
    App {
        version: String,
        bootloader: u16,
        serial: u32,
    },
    Boot {
        bootloader: u16,
    },
    Silent,
}

fn probe(w: &mut dyn Wire) -> Result<Mode, String> {
    if let Some(v) = read_mem(w, 0xe000, 8)? {
        let bl = read_mem(w, 0xe004, 4)?.map_or(0, |b| u16::from_le_bytes([b[0], b[1]]));
        let serial =
            read_mem(w, 0x8008, 4)?.map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        return Ok(Mode::App {
            version: fl::build_name(&v),
            bootloader: bl,
            serial,
        });
    }
    let buf = ask(w, &fl::hello_frame(), 800, |b| {
        fl::parse_boot_reply(b).is_some()
    })?;
    if let Some((fl::BootReply::Hello { version }, _)) = fl::parse_boot_reply(&buf) {
        return Ok(Mode::Boot {
            bootloader: version,
        });
    }
    Ok(Mode::Silent)
}

fn report_mode(out: &Out, port: &str, m: &Mode) {
    match m {
        Mode::App {
            version,
            bootloader,
            serial,
        } => out.emit(
            "device",
            &[
                ("port", s(port)),
                ("mode", s("app")),
                ("version", s(version)),
                ("bootloader", bootloader.to_string()),
                ("serial", serial.to_string()),
            ],
            &format!(
                "{port}: X1 running firmware {version}, bootloader 0x{bootloader:02X}, serial {serial}"
            ),
        ),
        Mode::Boot { bootloader } => out.emit(
            "device",
            &[
                ("port", s(port)),
                ("mode", s("boot")),
                ("bootloader", bootloader.to_string()),
            ],
            &format!("{port}: X1 in BOOT MODE, bootloader 0x{bootloader:02X}: ready to flash"),
        ),
        Mode::Silent => out.emit(
            "device",
            &[("port", s(port)), ("mode", s("none"))],
            &format!("{port}: no answer (is the X1 on and the cable a data cable?)"),
        ),
    }
}

fn open_port(out: &Out, chosen: Option<&str>) -> (String, Serial) {
    let path = match chosen {
        Some(p) => p.to_string(),
        None => match ports().as_slice() {
            [] => {
                out.error("no X1 USB port found: plug the X1 in with a data cable and switch it on")
            }
            [one] => one.clone(),
            many => out.error(&format!(
                "several USB serial ports ({}); pick one with --port",
                many.join(", ")
            )),
        },
    };
    match Serial::open(&path) {
        Ok(p) => (path, p),
        Err(e) => out.error(&e),
    }
}

fn image_fields(img: &Image, path: &str) -> Vec<(&'static str, String)> {
    vec![
        ("path", s(path)),
        ("version", s(&img.version_text())),
        ("built", img.built.to_string()),
        ("bytes", img.payload.len().to_string()),
        ("packets", img.packets().to_string()),
        (
            "warnings",
            format!(
                "[{}]",
                img.warnings
                    .iter()
                    .map(|w| s(w))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        ),
    ]
}

fn load_image(out: &Out, path: &str) -> Image {
    let bytes =
        std::fs::read(path).unwrap_or_else(|e| out.error(&format!("cannot read {path}: {e}")));
    Image::parse(&bytes).unwrap_or_else(|e| out.error(&format!("{path}: {e}")))
}

fn run_flash(out: &Out, w: &mut dyn Wire, img: Image) -> bool {
    let debug = std::env::var_os("X1FLASH_DEBUG").is_some();
    let t0 = Instant::now();
    let total = img.packets();
    let mut f = Flasher::new(img);
    let mut step = f.begin();
    out.emit(
        "phase",
        &[("phase", s("erasing"))],
        "Preparing the X1 (erasing staging)…",
    );
    let mut last_pct = usize::MAX;
    loop {
        match step {
            Step::Send { bytes, wait_ms } => {
                if let Err(e) = w.send(&bytes) {
                    out.error(&e);
                }
                let wait_ms = match std::env::var("X1FLASH_PACKET_WAIT_MS") {
                    Ok(v) if f.phase() == fl::Phase::Sending => v.parse().unwrap_or(wait_ms),
                    _ => wait_ms,
                };
                let end = Instant::now() + Duration::from_millis(u64::from(wait_ms));
                let mut next = None;
                let sent_at = Instant::now();
                let mut seen: Vec<u8> = Vec::new();
                while next.is_none() && Instant::now() < end {
                    let got = w.recv(50);
                    if !got.is_empty() {
                        seen.extend_from_slice(&got);
                        next = f.on_bytes(&got);
                    }
                }
                if debug {
                    let (i, _) = f.progress();
                    match &next {
                        None => eprintln!(
                            "[debug] t={:.2}s packet {i}: no answer in {} ms; bytes seen: {:02x?}", t0.elapsed().as_secs_f64(),
                            sent_at.elapsed().as_millis(),
                            seen
                        ),
                        Some(_) if seen.len() > 6 => {
                            eprintln!("[debug] packet {i}: extra bytes: {:02x?}", seen)
                        }
                        _ => {}
                    }
                }
                step = next.unwrap_or_else(|| f.on_timeout());
                let (done, _) = f.progress();
                match f.phase() {
                    fl::Phase::Sending => {
                        let pct = done * 100 / total.max(1);
                        if pct != last_pct {
                            last_pct = pct;
                            out.emit(
                                "progress",
                                &[("done", done.to_string()), ("total", total.to_string())],
                                &format!("  sent {done}/{total} packets ({pct}%)"),
                            );
                        }
                    }
                    fl::Phase::Installing if last_pct != 1000 => {
                        last_pct = 1000;
                        out.emit(
                            "phase",
                            &[("phase", s("installing"))],
                            "All packets confirmed. The X1 is checking and installing (UPGRADING...)…",
                        );
                    }
                    _ => {}
                }
            }
            Step::Done => {
                if f.resends() > 0 {
                    out.emit(
                        "resends",
                        &[("count", f.resends().to_string())],
                        &format!("  ({} packets were resent along the way)", f.resends()),
                    );
                }
                out.emit("done", &[], "UPGRADE SUCCESS: the new firmware is installed. Press CLR to power off, then start with DIST.");
                return true;
            }
            Step::Failed(e) => {
                out.emit(
                    "failed",
                    &[
                        ("message", s(&e.to_string())),
                        ("app_untouched", e.app_untouched().to_string()),
                    ],
                    &format!("FAILED: {e}"),
                );
                return false;
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    let yes = args.iter().any(|a| a == "--yes");
    let simulate = args.iter().any(|a| a == "--simulate");
    let port = args
        .iter()
        .position(|a| a == "--port")
        .and_then(|i| args.get(i + 1))
        .map(String::as_str);
    let plain: Vec<&str> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| !a.starts_with("--") && (*i == 0 || args[i - 1] != "--port"))
        .map(|(_, a)| a.as_str())
        .collect();
    let out = Out { json };

    match plain.as_slice() {
        ["ports"] => {
            let p = ports();
            out.emit(
                "ports",
                &[(
                    "ports",
                    format!("[{}]", p.iter().map(|x| s(x)).collect::<Vec<_>>().join(",")),
                )],
                &if p.is_empty() {
                    "no USB serial ports".to_string()
                } else {
                    p.join("\n")
                },
            );
        }
        ["info"] => {
            let (path, mut w) = open_port(&out, port);
            let m = probe(&mut w).unwrap_or_else(|e| out.error(&e));
            report_mode(&out, &path, &m);
        }
        ["inspect", file] => {
            let img = load_image(&out, file);
            let mut text = format!(
                "{file}\n  firmware {}  ({} bytes, {} packets)  built {} (Unix time)",
                img.version_text(),
                img.payload.len(),
                img.packets(),
                img.built
            );
            for w in &img.warnings {
                text.push_str(&format!("\n  warning: {w}"));
            }
            out.emit("image", &image_fields(&img, file), &text);
        }
        ["flash", file] => {
            let img = load_image(&out, file);
            out.emit(
                "image",
                &image_fields(&img, file),
                &format!(
                    "Firmware {} from {file} ({} packets).",
                    img.version_text(),
                    img.packets()
                ),
            );
            if simulate {
                let mut w = Sim {
                    x1: SimBootloader::new(vec![0xaa; 1024]),
                    pending: Vec::new(),
                };
                out.emit(
                    "phase",
                    &[("phase", s("simulating"))],
                    "SIMULATION: nothing is sent to a device.",
                );
                let ok = run_flash(&out, &mut w, img);
                std::process::exit(if ok { 0 } else { 1 });
            }
            let (path, mut w) = open_port(&out, port);
            let m = probe(&mut w).unwrap_or_else(|e| out.error(&e));
            report_mode(&out, &path, &m);
            match m {
                Mode::Boot { .. } => {}
                Mode::App { .. } => out.error(
                    "the X1 is running its normal firmware, which cannot take an update. Power it off, then hold both \
                     arrow keys and press DIST to enter boot mode",
                ),
                Mode::Silent => out.error("the X1 did not answer: is it on, in boot mode, and on a data cable?"),
            }
            if !yes {
                out.error("this replaces the X1's firmware and erases its stored shots; add --yes to go ahead");
            }
            let ok = run_flash(&out, &mut w, img);
            std::process::exit(if ok { 0 } else { 1 });
        }
        ["scan", what] => {
            let cmd = match *what {
                "start" => cavway::CMD_SCAN_START,
                "stop" => cavway::CMD_SCAN_STOP,
                "status" => cavway::CMD_SCAN_STATUS,
                _ => out.error("scan start, scan stop or scan status"),
            };
            let (_, mut w) = open_port(&out, port);
            let find = |b: &[u8]| {
                b.windows(3)
                    .position(|w| w == [0x3d, cmd, 0xcc])
                    .filter(|&i| b.len() >= i + 16)
            };
            let buf = ask(&mut w, &cavway::command(cmd), 1500, |b| find(b).is_some())
                .unwrap_or_else(|e| out.error(&e));
            let Some(st) = find(&buf).and_then(|i| cavway::parse_scan_status(&buf[i..])) else {
                out.error("no answer: this X1 is not running CaveCAD firmware (-cc), or is off")
            };
            let end = format!("{:?}", st.last_end);
            out.emit(
                "scan",
                &[
                    ("result", st.result.to_string()),
                    ("scanning", st.scanning().to_string()),
                    ("mode", st.mode.to_string()),
                    ("laser", st.laser.to_string()),
                    ("count", st.count.to_string()),
                    ("seconds", format!("{:.2}", st.seconds())),
                    ("options", st.options.to_string()),
                    ("last_end", s(&end)),
                    ("cc", st.cc_version.to_string()),
                ],
                &format!(
                    "{}: mode {}{}, laser {}, {} shots, {:.1} s, options 0x{:02X}, last end {end}, cc{}",
                    match st.result {
                        0 => "ok",
                        1 => "wrong mode (start needs the laser on: send 36 first; stop needs a scan)",
                        2 => "laser not ready yet, try again in a moment",
                        _ => "?",
                    },
                    st.mode,
                    if st.scanning() { " (scanning)" } else { "" },
                    st.laser,
                    st.count,
                    st.seconds(),
                    st.options,
                    st.cc_version
                ),
            );
        }
        ["clock"] => {
            let (path, mut w) = open_port(&out, port);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64);
            // SAFETY: localtime_r fills a struct we own.
            let offset = unsafe {
                let t: libc::time_t = now as libc::time_t;
                let mut tm: libc::tm = std::mem::zeroed();
                libc::localtime_r(&t, &mut tm);
                tm.tm_gmtoff as i32
            };
            let local = cavway::local_seconds(now, offset);
            let head = [0x3d, 0x00, 0x80, 0x04];
            let buf = ask(&mut w, &cavway::clock_frame(local), 1500, |b| {
                b.windows(4).position(|x| x == head).is_some_and(|i| b.len() >= i + 8)
            })
            .unwrap_or_else(|e| out.error(&e));
            let echoed = buf
                .windows(4)
                .position(|x| x == head)
                .map(|i| u32::from_le_bytes([buf[i + 4], buf[i + 5], buf[i + 6], buf[i + 7]]));
            let note = match echoed {
                Some(v) if v.abs_diff(local) <= 2 => "confirmed".to_string(),
                Some(v) => format!("X1 reads back {v}"),
                None => "sent (no read-back: over USB the X1 only answers while charging)".to_string(),
            };
            out.emit(
                "clock",
                &[("port", s(&path)), ("local", local.to_string()), ("confirmed", (note == "confirmed").to_string())],
                &format!("Clock set to this Mac's local time ({local}): {note}"),
            );
        }
        ["off"] => {
            let (path, mut w) = open_port(&out, port);
            w.send(&cavway::command(fl::cmd::POWER_OFF))
                .unwrap_or_else(|e| out.error(&e));
            out.emit("off", &[("port", s(&path))], "Power off sent.");
        }
        _ => {
            eprintln!(
                "x1flash: flash the Cavway X1 over USB\n\n  x1flash ports\n  x1flash info [--port P]\n  x1flash inspect FILE.bin\n  \
                 x1flash flash FILE.bin --yes [--port P]\n  x1flash flash FILE.bin --simulate\n  x1flash off [--port P]\n  x1flash clock [--port P]\n  x1flash scan start|stop|status [--port P]\n\n  --json  one JSON object per line"
            );
            std::process::exit(2);
        }
    }
}
