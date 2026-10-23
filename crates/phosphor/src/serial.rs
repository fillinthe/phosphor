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

/// Opens `path` the way DMX's wire wants it: raw, 250 kbit/s, 8 data bits, 2 stop bits (an FTDI chip with no timing
/// of its own, like the Enttec Open DMX; `send_break` starts each frame).
pub fn open_dmx(path: &str) -> Result<std::fs::File, String> {
    let file = open(path, 57_600)?;
    dmx_line(&file)?;
    Ok(file)
}

#[cfg(target_os = "macos")]
fn dmx_line(file: &std::fs::File) -> Result<(), String> {
    use std::os::fd::AsRawFd;
    let fd = file.as_raw_fd();
    // IOSSIOSPEED = _IOW('T', 2, speed_t): any speed the driver can make (termios only knows the standard ones).
    const IOSSIOSPEED: libc::c_ulong = 0x8008_5402;
    // SAFETY: termios calls and the IOSSIOSPEED ioctl (which reads one speed_t) on a descriptor we own.
    unsafe {
        if libc::isatty(fd) != 1 {
            return Ok(());
        }
        let mut t: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut t) == 0 {
            t.c_cflag |= libc::CSTOPB;
            t.c_cflag &= !(libc::PARENB | libc::CRTSCTS);
            libc::tcsetattr(fd, libc::TCSANOW, &t);
        }
        let speed: libc::speed_t = 250_000;
        if libc::ioctl(fd, IOSSIOSPEED, &speed) != 0 {
            return Err("the port can't run at DMX speed (250 kbit/s)".into());
        }
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn dmx_line(_file: &std::fs::File) -> Result<(), String> {
    Err("Open DMX interfaces work on the Mac only for now".into())
}

/// Holds the line low for `micros` (a DMX "break"), then lets it go; the bytes written next follow the "mark".
#[cfg(unix)]
pub fn send_break(file: &std::fs::File, micros: u64) {
    use std::os::fd::AsRawFd;
    let fd = file.as_raw_fd();
    // SAFETY: break on / off ioctls on a descriptor we own (they fail harmlessly on things that aren't ports).
    unsafe {
        libc::ioctl(fd, libc::TIOCSBRK as _);
        std::thread::sleep(std::time::Duration::from_micros(micros));
        libc::ioctl(fd, libc::TIOCCBRK as _);
    }
}

#[cfg(not(unix))]
pub fn send_break(_file: &std::fs::File, _micros: u64) {}

/// Waits until everything written has left the port.
#[cfg(unix)]
pub fn drain(file: &std::fs::File) {
    use std::os::fd::AsRawFd;
    // SAFETY: tcdrain on a descriptor we own.
    unsafe { libc::tcdrain(file.as_raw_fd()) };
}

#[cfg(not(unix))]
pub fn drain(_file: &std::fs::File) {}
