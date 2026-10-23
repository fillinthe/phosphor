//! Arduino boards over USB (Firmata, the "StandardFirmata" sketch from the Arduino IDE's examples): analog inputs
//! (A0…A5: knobs, light and distance sensors) and digital pins (buttons) come in as controller input
//! (`ControlSource::Arduino`, 0..1); any control can drive a PWM pin (`SendTo::Arduino`: LEDs, servos).

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use phosphor_engine::ControlSource;

/// StandardFirmata talks at this speed.
pub const BAUD: u32 = 57_600;
const ANALOG_PINS: u8 = 6;
const DIGITAL_PORTS: u8 = 3;

/// Firmata's start-up asks: report every analog input and digital port.
pub fn report_all() -> Vec<u8> {
    let mut v = Vec::new();
    for pin in 0..ANALOG_PINS {
        v.extend([0xC0 | pin, 1]);
    }
    for port in 0..DIGITAL_PORTS {
        v.extend([0xD0 | port, 1]);
    }
    v
}

/// A PWM pin set to `v` (0..1): its mode (3 = PWM) the first time, then the value (0..255, 7 bits at a time).
pub fn pwm(pin: u8, v: f32, set_mode: bool) -> Vec<u8> {
    let mut out = Vec::new();
    if set_mode {
        out.extend([0xF4, pin, 3]);
    }
    let x = (v.clamp(0.0, 1.0) * 255.0).round() as u16;
    if pin < 16 {
        out.extend([0xE0 | pin, (x & 0x7F) as u8, (x >> 7) as u8]);
    } else {
        // Extended analog (pins 16 and up), a sysex message.
        out.extend([0xF0, 0x6F, pin, (x & 0x7F) as u8, (x >> 7) as u8, 0xF7]);
    }
    out
}

/// Reads Firmata messages from a byte stream: analog values (0..1023) and digital ports (8 pins each).
#[derive(Default)]
pub struct Parser {
    msg: Vec<u8>,
    in_sysex: bool,
    /// Digital pins last seen (a change is input).
    digital: [u8; 16],
    seen_ports: u16,
}

impl Parser {
    /// The inputs these bytes complete: (source, 0..1). Digital pins report only when they change.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<(ControlSource, f32)> {
        let mut out = Vec::new();
        for &b in bytes {
            if self.in_sysex {
                // Sysex replies (firmware name, capabilities) aren't needed.
                self.in_sysex = b != 0xF7;
                continue;
            }
            if b == 0xF0 {
                self.in_sysex = true;
                self.msg.clear();
                continue;
            }
            if b & 0x80 != 0 {
                self.msg.clear();
            } else if self.msg.is_empty() {
                continue;
            }
            self.msg.push(b);
            if self.msg.len() < 3 {
                continue;
            }
            let (cmd, lo, hi) = (self.msg[0], self.msg[1], self.msg[2]);
            self.msg.clear();
            let value = lo as u16 | (hi as u16) << 7;
            match cmd & 0xF0 {
                0xE0 => out.push((ControlSource::Arduino { pin: cmd & 0x0F, analog: true }, value.min(1023) as f32 / 1023.0)),
                0x90 => {
                    let port = (cmd & 0x0F) as usize;
                    if port >= self.digital.len() {
                        continue;
                    }
                    let bits = value as u8;
                    let first = self.seen_ports & (1 << port) == 0;
                    self.seen_ports |= 1 << port;
                    for i in 0..8u8 {
                        let (now, was) = (bits >> i & 1, self.digital[port] >> i & 1);
                        // The first report only says where things are.
                        if !first && now != was {
                            out.push((ControlSource::Arduino { pin: port as u8 * 8 + i, analog: false }, now as f32));
                        }
                    }
                    self.digital[port] = bits;
                }
                _ => {}
            }
        }
        out
    }
}

/// A board on a USB port: a reader thread sends its inputs as controller input; `send` writes PWM values.
pub struct Arduino {
    pub path: String,
    writer: Arc<Mutex<std::fs::File>>,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<String>>,
    /// PWM pins already switched to PWM mode.
    pwm_pins: u64,
}

impl Arduino {
    pub fn open(path: &str, tx: std::sync::mpsc::Sender<crate::control::Incoming>) -> Result<Self, String> {
        let mut file = crate::serial::open(path, BAUD)?;
        let mut reader = file.try_clone().map_err(|e| e.to_string())?;
        // Boards reset when the port opens: StandardFirmata is ready about two seconds later. Ask now and again then.
        let _ = file.write_all(&report_all());
        let writer = Arc::new(Mutex::new(file));
        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(format!("{path}: waiting for the board…")));
        let (w, st, flag, name) = (writer.clone(), status.clone(), stop.clone(), path.to_string());
        std::thread::Builder::new()
            .name("arduino".into())
            .spawn(move || {
                let mut parser = Parser::default();
                let mut buf = [0u8; 256];
                let started = std::time::Instant::now();
                let (mut asked_again, mut heard) = (false, 0u64);
                while !flag.load(Ordering::Relaxed) {
                    if !asked_again && started.elapsed().as_secs_f32() > 2.5 {
                        asked_again = true;
                        if let Ok(mut f) = w.lock() {
                            let _ = f.write_all(&report_all());
                        }
                    }
                    match reader.read(&mut buf) {
                        Ok(0) => std::thread::sleep(std::time::Duration::from_millis(5)),
                        Ok(n) => {
                            for (source, value) in parser.feed(&buf[..n]) {
                                heard += 1;
                                if tx.send(crate::control::Incoming { source, value }).is_err() {
                                    return;
                                }
                            }
                            if heard > 0 && let Ok(mut s) = st.lock() {
                                *s = format!("{name}: connected");
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted || e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(e) => {
                            if let Ok(mut s) = st.lock() {
                                *s = format!("{name}: {e}");
                            }
                            return;
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { path: path.to_string(), writer, stop, status, pwm_pins: 0 })
    }

    /// A PWM pin to `v` (0..1).
    pub fn send(&mut self, pin: u8, v: f32) {
        let first = pin < 64 && self.pwm_pins & (1 << pin) == 0;
        if pin < 64 {
            self.pwm_pins |= 1 << pin;
        }
        if let Ok(mut f) = self.writer.lock() {
            let _ = f.write_all(&pwm(pin, v, first));
        }
    }

    pub fn status(&self) -> String {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

impl Drop for Arduino {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firmata_messages_become_inputs() {
        let mut p = Parser::default();
        // A0 at 1023, A2 at 512, a sysex reply in between, a message split across reads.
        let got = p.feed(&[0xE0, 0x7F, 0x07, 0xF0, 0x79, 2, 5, 0xF7, 0xE2, 0x00]);
        assert_eq!(got, vec![(ControlSource::Arduino { pin: 0, analog: true }, 1.0)]);
        let got = p.feed(&[0x04]);
        assert_eq!(got, vec![(ControlSource::Arduino { pin: 2, analog: true }, 512.0 / 1023.0)]);
        // Digital port 0: the first report says where things are, then pin 2 going up is a press.
        assert!(p.feed(&[0x90, 0b0000_0001, 0]).is_empty());
        assert_eq!(p.feed(&[0x90, 0b0000_0101, 0]), vec![(ControlSource::Arduino { pin: 2, analog: false }, 1.0)]);
    }

    #[test]
    fn pwm_values_go_out_as_firmata() {
        assert_eq!(pwm(9, 1.0, true), vec![0xF4, 9, 3, 0xE9, 0x7F, 0x01]);
        assert_eq!(pwm(9, 0.0, false), vec![0xE9, 0, 0]);
        assert_eq!(pwm(20, 0.5, false), vec![0xF0, 0x6F, 20, 0x00, 0x01, 0xF7]);
    }

    /// A pretend board on a pseudo-terminal: the start-up asks arrive, its analog report comes back as input, and a
    /// PWM value reaches it.
    #[cfg(unix)]
    #[test]
    fn a_board_on_a_serial_port() {
        use std::os::fd::FromRawFd;
        let (mut master, mut slave) = (0, 0);
        let mut name = [0 as libc::c_char; 128];
        // SAFETY: openpty fills two descriptors and the slave's name.
        if unsafe { libc::openpty(&mut master, &mut slave, name.as_mut_ptr(), std::ptr::null_mut(), std::ptr::null_mut()) } != 0 {
            return;
        }
        // SAFETY: openpty wrote a NUL-terminated name.
        let path = unsafe { std::ffi::CStr::from_ptr(name.as_ptr()) }.to_string_lossy().to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut board = Arduino::open(&path, tx).unwrap();
        // SAFETY: we own the master descriptor.
        let mut m = unsafe { std::fs::File::from_raw_fd(master) };
        let mut asks = vec![0u8; report_all().len()];
        m.read_exact(&mut asks).unwrap();
        assert_eq!(asks, report_all());
        m.write_all(&[0xE3, 0x00, 0x04]).unwrap();
        let got = rx.recv_timeout(std::time::Duration::from_secs(3)).expect("an input");
        assert_eq!((got.source, got.value), (ControlSource::Arduino { pin: 3, analog: true }, 512.0 / 1023.0));
        board.send(5, 1.0);
        let mut out = vec![0u8; 6];
        m.read_exact(&mut out).unwrap();
        assert_eq!(out, pwm(5, 1.0, true));
        drop(board);
        // SAFETY: the slave descriptor from openpty, closed once.
        unsafe { libc::close(slave) };
    }
}
