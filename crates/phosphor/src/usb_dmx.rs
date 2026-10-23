//! DMX out through a USB interface, from a worker thread (the newest frame wins, the render loop never waits):
//! - the Enttec DMX USB Pro and compatibles (DMXking ultraDMX, many clones speak the same "Pro" protocol): the
//!   widget does the DMX timing; we write one packet a frame to its USB serial port;
//! - the Enttec Open DMX and other plain FTDI cables: no timing of their own, so the worker makes the DMX signal
//!   itself (break, then the start code and 512 channels at 250 kbit/s) and repeats the last frame ~40 times a
//!   second, as lights expect.

use std::io::Write;
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::{Arc, Mutex};

/// One "Output Only Send DMX Packet" message: start byte, label 6, length (start code + 512 slots), start code 0, the
/// channels, end byte.
pub fn pro_packet(data: &[u8; 512]) -> Vec<u8> {
    let len = (data.len() + 1) as u16;
    let mut p = Vec::with_capacity(data.len() + 6);
    p.extend_from_slice(&[0x7E, 6, len as u8, (len >> 8) as u8, 0x00]);
    p.extend_from_slice(data);
    p.push(0xE7);
    p
}

/// What the Open DMX's line carries after each break: the start code, then the channels.
pub fn open_frame(data: &[u8; 512]) -> Vec<u8> {
    let mut f = Vec::with_capacity(513);
    f.push(0x00);
    f.extend_from_slice(data);
    f
}

/// The two kinds of interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interface {
    Pro,
    Open,
}

/// Open DMX: a frame at most every 25 ms (40 a second; 513 slots take ~23 ms on the wire anyway), break 110 µs.
const OPEN_PERIOD: std::time::Duration = std::time::Duration::from_millis(25);
const BREAK_MICROS: u64 = 110;

/// USB serial ports that might be a DMX interface (asks the system: call when a menu opens).
pub fn ports() -> Vec<String> {
    crate::serial::ports()
}

/// An open interface: frames go to its worker thread.
pub struct UsbOut {
    pub path: String,
    pub kind: Interface,
    tx: SyncSender<Vec<u8>>,
    /// The last write error (the widget unplugged…).
    pub error: Arc<Mutex<Option<String>>>,
}

impl UsbOut {
    pub fn open(path: &str, kind: Interface) -> Result<Self, String> {
        let file = match kind {
            Interface::Pro => crate::serial::open(path, 57_600)?,
            Interface::Open => crate::serial::open_dmx(path)?,
        };
        let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(1);
        let error: Arc<Mutex<Option<String>>> = Arc::default();
        let err = error.clone();
        std::thread::Builder::new()
            .name("usb dmx".into())
            .spawn(move || match kind {
                Interface::Pro => pro_worker(file, rx, err),
                Interface::Open => open_worker(file, rx, err),
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { path: path.to_string(), kind, tx, error })
    }

    /// Sends one universe; a frame still being written is replaced by the next.
    pub fn send(&self, data: &[u8; 512]) -> bool {
        let bytes = match self.kind {
            Interface::Pro => pro_packet(data),
            Interface::Open => open_frame(data),
        };
        !matches!(self.tx.try_send(bytes), Err(TrySendError::Disconnected(_)))
    }
}

fn report(err: &Mutex<Option<String>>, result: std::io::Result<()>) {
    if let Ok(mut e) = err.lock() {
        *e = result.err().map(|e| format!("writing to the interface failed ({e})"));
    }
}

fn pro_worker(mut file: std::fs::File, rx: std::sync::mpsc::Receiver<Vec<u8>>, err: Arc<Mutex<Option<String>>>) {
    while let Ok(packet) = rx.recv() {
        report(&err, file.write_all(&packet).and_then(|_| file.flush()));
    }
}

/// Makes the DMX signal: break, mark (the gap before the write), the frame, then the next break once it has left;
/// the last frame keeps repeating until a new one comes or the interface is closed.
fn open_worker(mut file: std::fs::File, rx: std::sync::mpsc::Receiver<Vec<u8>>, err: Arc<Mutex<Option<String>>>) {
    use std::sync::mpsc::TryRecvError;
    let Ok(mut frame) = rx.recv() else { return };
    loop {
        let started = std::time::Instant::now();
        crate::serial::send_break(&file, BREAK_MICROS);
        let result = file.write_all(&frame).and_then(|_| file.flush());
        let failed = result.is_err();
        report(&err, result);
        crate::serial::drain(&file);
        let wait = if failed { std::time::Duration::from_millis(500) } else { OPEN_PERIOD.saturating_sub(started.elapsed()) };
        std::thread::sleep(wait);
        match rx.try_recv() {
            Ok(f) => frame = f,
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pro_packet_carries_a_universe() {
        let mut data = [0u8; 512];
        (data[0], data[511]) = (255, 7);
        let p = pro_packet(&data);
        assert_eq!(p.len(), 518);
        assert_eq!(&p[..5], &[0x7E, 6, 0x01, 0x02, 0x00], "label 6, length 513, start code 0");
        assert_eq!((p[5], p[516], p[517]), (255, 7, 0xE7));
    }

    /// Through a real terminal device (a pseudo-terminal): raw mode lets every byte through unchanged (a cooked tty
    /// would turn 0x0A into 0x0D 0x0A and break the packet).
    #[cfg(unix)]
    #[test]
    fn a_serial_port_gets_the_bytes_unchanged() {
        use std::io::Read;
        use std::os::fd::FromRawFd;
        let (mut master, mut slave) = (0, 0);
        let mut name = [0 as libc::c_char; 128];
        // SAFETY: openpty fills two descriptors and the slave's name (128 bytes is plenty).
        let ok = unsafe { libc::openpty(&mut master, &mut slave, name.as_mut_ptr(), std::ptr::null_mut(), std::ptr::null_mut()) } == 0;
        if !ok {
            return;
        }
        // SAFETY: openpty wrote a NUL-terminated name.
        let path = unsafe { std::ffi::CStr::from_ptr(name.as_ptr()) }.to_string_lossy().to_string();
        let out = UsbOut::open(&path, Interface::Pro).unwrap();
        let mut data = [0x0Au8; 512];
        data[0] = 0x0D;
        assert!(out.send(&data));
        // SAFETY: we own the master descriptor from openpty.
        let mut m = unsafe { std::fs::File::from_raw_fd(master) };
        let mut got = vec![0u8; 518];
        m.read_exact(&mut got).unwrap();
        // SAFETY: the slave descriptor from openpty, closed once.
        unsafe { libc::close(slave) };
        assert_eq!(got, pro_packet(&data));
    }

    /// Through the worker to a file standing in for the port: the packet arrives whole.
    #[test]
    fn frames_reach_the_port() {
        let path = std::env::temp_dir().join(format!("phosphor-usb-dmx-{}", std::process::id()));
        std::fs::write(&path, b"").unwrap();
        let out = UsbOut::open(path.to_str().unwrap(), Interface::Pro).unwrap();
        let mut data = [0u8; 512];
        data[2] = 42;
        assert!(out.send(&data));
        let mut got = Vec::new();
        for _ in 0..100 {
            got = std::fs::read(&path).unwrap();
            if got.len() >= 518 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let _ = std::fs::remove_file(&path);
        assert_eq!(got, pro_packet(&data));
    }

    /// Open DMX: the worker keeps repeating the last frame (lights black out without a steady signal), and a new
    /// frame replaces it.
    #[test]
    fn open_dmx_repeats_the_last_frame() {
        let path = std::env::temp_dir().join(format!("phosphor-open-dmx-{}", std::process::id()));
        std::fs::write(&path, b"").unwrap();
        let out = UsbOut::open(path.to_str().unwrap(), Interface::Open).unwrap();
        let mut data = [0u8; 512];
        data[0] = 9;
        assert!(out.send(&data));
        let wait_for = |n: usize| {
            for _ in 0..200 {
                let got = std::fs::read(&path).unwrap();
                if got.len() >= n {
                    return got;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            std::fs::read(&path).unwrap()
        };
        let got = wait_for(513 * 2);
        assert!(got.len() >= 513 * 2, "repeats on its own");
        assert_eq!(&got[..513], open_frame(&data).as_slice());
        assert_eq!(&got[513..1026], open_frame(&data).as_slice());
        data[0] = 77;
        assert!(out.send(&data));
        let start = std::fs::read(&path).unwrap().len();
        let got = wait_for(start + 513 * 3);
        drop(out);
        let _ = std::fs::remove_file(&path);
        let last = got.len() / 513 * 513 - 513;
        assert_eq!(&got[last..last + 513], open_frame(&data).as_slice(), "the new frame took over");
    }

    /// On a terminal device the line is set up for DMX (2 stop bits) and frames come through whole; a port that can't
    /// run at 250 kbit/s (a pseudo-terminal, here) is refused with a plain reason instead of sending garbage.
    #[cfg(target_os = "macos")]
    #[test]
    fn open_dmx_sets_up_the_port() {
        use std::io::Read;
        use std::os::fd::FromRawFd;
        let (mut master, mut slave) = (0, 0);
        let mut name = [0 as libc::c_char; 128];
        // SAFETY: openpty fills two descriptors and the slave's name.
        let ok = unsafe { libc::openpty(&mut master, &mut slave, name.as_mut_ptr(), std::ptr::null_mut(), std::ptr::null_mut()) } == 0;
        if !ok {
            return;
        }
        // SAFETY: openpty wrote a NUL-terminated name.
        let path = unsafe { std::ffi::CStr::from_ptr(name.as_ptr()) }.to_string_lossy().to_string();
        let opened = UsbOut::open(&path, Interface::Open);
        if let Err(e) = &opened {
            assert!(e.contains("DMX speed"), "{e}");
        }
        // SAFETY: we own both descriptors from openpty.
        let mut m = unsafe { std::fs::File::from_raw_fd(master) };
        if let Ok(out) = opened {
            let mut t: libc::termios = unsafe { std::mem::zeroed() };
            // SAFETY: tcgetattr on the slave descriptor we own.
            unsafe { libc::tcgetattr(slave, &mut t) };
            assert_ne!(t.c_cflag & libc::CSTOPB, 0, "2 stop bits");
            let mut data = [0x0Au8; 512];
            data[5] = 200;
            assert!(out.send(&data));
            let mut got = vec![0u8; 513];
            m.read_exact(&mut got).unwrap();
            assert_eq!(got, open_frame(&data));
        }
        // SAFETY: the slave descriptor from openpty, closed once.
        unsafe { libc::close(slave) };
    }
}
