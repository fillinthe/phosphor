//! USB serial ports (DMX interfaces, Arduino boards): finding them and opening them raw (bytes as they are, at a
//! chosen speed).

/// USB serial ports (asks the system: call when a menu opens).
pub fn ports() -> Vec<String> {
    let Ok(dir) = std::fs::read_dir("/dev") else { return Vec::new() };
    let mut v: Vec<String> = dir
        .filter_map(|e| e.ok())
        .map(|e| format!("/dev/{}", e.file_name().to_string_lossy()))
        .filter(|p| ["/dev/cu.usbserial", "/dev/cu.usbmodem", "/dev/ttyUSB", "/dev/ttyACM"].iter().any(|k| p.starts_with(k)))
        .collect();
    v.sort();
    v
}

/// Opens `path` for reading and writing; a terminal device is put in raw mode at `baud` (plain files, for tests,
/// are left alone).
pub fn open(path: &str, baud: u32) -> Result<std::fs::File, String> {
    let file = std::fs::OpenOptions::new().read(true).write(true).open(path).map_err(|e| format!("can't open {path} ({e})"))?;
    raw(&file, baud);
    Ok(file)
}

#[cfg(unix)]
fn raw(file: &std::fs::File, baud: u32) {
    use std::os::fd::AsRawFd;
    let fd = file.as_raw_fd();
    let speed = match baud {
        9600 => libc::B9600,
        19200 => libc::B19200,
        38400 => libc::B38400,
        115200 => libc::B115200,
        _ => libc::B57600,
    };
    // SAFETY: plain termios calls on a file descriptor we own, with a zeroed struct filled by tcgetattr.
    unsafe {
        if libc::isatty(fd) != 1 {
            return;
        }
        let mut t: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut t) == 0 {
            libc::cfmakeraw(&mut t);
            libc::cfsetspeed(&mut t, speed);
            // Reads wait at most 0.2 s, so a reader thread can notice it should stop.
            t.c_cc[libc::VMIN] = 0;
            t.c_cc[libc::VTIME] = 2;
            libc::tcsetattr(fd, libc::TCSANOW, &t);
        }
    }
}

#[cfg(not(unix))]
fn raw(_file: &std::fs::File, _baud: u32) {}
