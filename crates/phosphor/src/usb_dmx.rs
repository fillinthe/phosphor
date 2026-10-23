//! DMX out through a USB interface: the Enttec DMX USB Pro and compatibles (DMXking ultraDMX, many clones speak
//! the same "Pro" protocol). The widget does the DMX timing; we write one packet a frame to its USB serial port,
//! from a worker thread (the newest frame wins, the render loop never waits).

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

/// USB serial ports that might be a DMX interface (asks the system: call when a menu opens).
pub fn ports() -> Vec<String> {
    crate::serial::ports()
}

/// An open interface: frames go to its worker thread.
pub struct ProOut {
    pub path: String,
    tx: SyncSender<Vec<u8>>,
    /// The last write error (the widget unplugged…).
    pub error: Arc<Mutex<Option<String>>>,
}

impl ProOut {
    pub fn open(path: &str) -> Result<Self, String> {
        let mut file = crate::serial::open(path, 57_600)?;
        let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(1);
        let error: Arc<Mutex<Option<String>>> = Arc::default();
        let err = error.clone();
        std::thread::Builder::new()
            .name("usb dmx".into())
            .spawn(move || {
                while let Ok(packet) = rx.recv() {
                    let result = file.write_all(&packet).and_then(|_| file.flush());
                    if let Ok(mut e) = err.lock() {
                        *e = result.err().map(|e| format!("writing to the interface failed ({e})"));
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { path: path.to_string(), tx, error })
    }

    /// Sends one universe; a frame still being written is replaced by the next.
    pub fn send(&self, data: &[u8; 512]) -> bool {
        !matches!(self.tx.try_send(pro_packet(data)), Err(TrySendError::Disconnected(_)))
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
        let out = ProOut::open(&path).unwrap();
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
        let out = ProOut::open(path.to_str().unwrap()).unwrap();
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
}
