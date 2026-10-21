//! PJLink (class 1): switch projectors on and off and close their shutter over the network (TCP port 4352), like
//! MadMapper's projector control. Each output can name its projector (`Projector`); commands run on a short-lived
//! thread each (never on the UI thread) and their answers come back as a status line per output.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use phosphor_engine::Projector;

pub const PORT: u16 = 4352;
const TIMEOUT: Duration = Duration::from_secs(3);

/// What to ask a projector.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ask {
    PowerOn,
    PowerOff,
    /// Picture and sound muted (the shutter), or back.
    Shutter(bool),
    /// Power state and name.
    Status,
}

impl Ask {
    fn commands(self) -> &'static [&'static str] {
        match self {
            Ask::PowerOn => &["POWR 1"],
            Ask::PowerOff => &["POWR 0"],
            Ask::Shutter(true) => &["AVMT 31"],
            Ask::Shutter(false) => &["AVMT 30"],
            Ask::Status => &["POWR ?", "NAME ?", "AVMT ?"],
        }
    }
}

/// Sends `commands` to `host` (with `:port` or the PJLink port) and returns each answer's value (after `=`).
pub fn exchange(host: &str, password: &str, commands: &[&str]) -> Result<Vec<String>, String> {
    let addr = if host.contains(':') { host.to_string() } else { format!("{host}:{PORT}") };
    let sock = addr.to_socket_addrs().map_err(|e| format!("Can't find {host} ({e})"))?.next().ok_or_else(|| format!("Can't find {host}"))?;
    let stream = TcpStream::connect_timeout(&sock, TIMEOUT).map_err(|e| format!("Can't reach {host} ({e})"))?;
    stream.set_read_timeout(Some(TIMEOUT)).ok();
    stream.set_write_timeout(Some(TIMEOUT)).ok();
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut writer = stream;
    let greeting = read_line(&mut reader)?;
    // "PJLINK 0" = no password; "PJLINK 1 <random>" = prefix the first command with md5(random + password).
    let mut prefix = match greeting.split_whitespace().collect::<Vec<_>>()[..] {
        ["PJLINK", "0"] => String::new(),
        ["PJLINK", "1", random] => hex(&md5(format!("{random}{password}").as_bytes())),
        ["PJLINK", "ERRA"] => return Err("Wrong projector password".into()),
        _ => return Err(format!("Not a PJLink projector ({greeting})")),
    };
    let mut answers = Vec::new();
    for cmd in commands {
        writer.write_all(format!("{}%1{cmd}\r", std::mem::take(&mut prefix)).as_bytes()).map_err(|e| e.to_string())?;
        let line = read_line(&mut reader)?;
        if line == "PJLINK ERRA" {
            return Err("Wrong projector password".into());
        }
        let value = line.split_once('=').map(|(_, v)| v.to_string()).ok_or_else(|| format!("Odd answer: {line}"))?;
        answers.push(value);
    }
    Ok(answers)
}

fn read_line(r: &mut impl BufRead) -> Result<String, String> {
    let mut buf = Vec::new();
    r.read_until(b'\r', &mut buf).map_err(|e| format!("No answer ({e})"))?;
    if buf.is_empty() {
        return Err("The projector hung up".into());
    }
    Ok(String::from_utf8_lossy(&buf).trim().to_string())
}

/// One answer in words.
fn says(ask: Ask, answers: &[String]) -> String {
    let err = |v: &str| match v {
        "ERR1" => Some("it doesn't know that command"),
        "ERR2" => Some("it refused the setting"),
        "ERR3" => Some("busy or warming up; try again in a moment"),
        "ERR4" => Some("projector fault (lamp, cover or fan)"),
        _ => None,
    };
    if let Some(e) = answers.iter().find_map(|a| err(a)) {
        return format!("Projector says: {e}");
    }
    match ask {
        Ask::PowerOn => "Switching on".into(),
        Ask::PowerOff => "Switching off".into(),
        Ask::Shutter(true) => "Shutter closed".into(),
        Ask::Shutter(false) => "Shutter open".into(),
        Ask::Status => {
            let power = match answers.first().map(String::as_str) {
                Some("0") => "Standby",
                Some("1") => "On",
                Some("2") => "Cooling down",
                Some("3") => "Warming up",
                _ => "Unknown",
            };
            let name = answers.get(1).filter(|n| !n.is_empty()).map(|n| format!(" · {n}")).unwrap_or_default();
            let shutter = if answers.get(2).is_some_and(|a| a.ends_with('1')) { " · shutter closed" } else { "" };
            format!("{power}{name}{shutter}")
        }
    }
}

/// Every output's projector status, and the commands in flight.
pub struct Projectors {
    tx: Sender<(u64, String)>,
    rx: Receiver<(u64, String)>,
    /// Per output: the last answer in words.
    pub status: std::collections::HashMap<u64, String>,
    /// The last shutter command sent to all of them (for the Shutter toggle control).
    pub shutter_closed: bool,
}

impl Default for Projectors {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self { tx, rx, status: Default::default(), shutter_closed: false }
    }
}

impl Projectors {
    /// Asks output `output`'s projector (off the UI thread).
    pub fn ask(&mut self, output: u64, p: &Projector, ask: Ask) {
        if p.host.trim().is_empty() {
            return;
        }
        self.status.insert(output, "…".into());
        let (host, password, tx) = (p.host.trim().to_string(), p.password.clone(), self.tx.clone());
        std::thread::Builder::new()
            .name("pjlink".into())
            .spawn(move || {
                let words = match exchange(&host, &password, ask.commands()) {
                    Ok(a) => says(ask, &a),
                    Err(e) => e,
                };
                let _ = tx.send((output, words));
            })
            .ok();
    }

    /// Asks every output that has a projector.
    pub fn ask_all(&mut self, outputs: &[phosphor_engine::OutputConfig], ask: Ask) {
        if let Ask::Shutter(closed) = ask {
            self.shutter_closed = closed;
        }
        for o in outputs {
            if let Some(p) = &o.projector {
                self.ask(o.id, p, ask);
            }
        }
    }

    /// Collects finished answers (call once per frame).
    pub fn poll(&mut self) {
        while let Ok((output, words)) = self.rx.try_recv() {
            self.status.insert(output, words);
        }
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// MD5 (RFC 1321): only for PJLink's password check.
pub fn md5(data: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4,
        11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let k: Vec<u32> = (0..64).map(|i| ((i as f64 + 1.0).sin().abs() * 4294967296.0) as u32).collect();
    let mut msg = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_le_bytes());
    let mut h: [u32; 4] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];
    for chunk in msg.chunks_exact(64) {
        let m: Vec<u32> = chunk.chunks_exact(4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]])).collect();
        let [mut a, mut b, mut c, mut d] = h;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        h = [h[0].wrapping_add(a), h[1].wrapping_add(b), h[2].wrapping_add(c), h[3].wrapping_add(d)];
    }
    let mut out = [0u8; 16];
    for (i, w) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpListener;

    #[test]
    fn md5_matches_the_rfc() {
        assert_eq!(hex(&md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex(&md5(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(hex(&md5(b"12345678901234567890123456789012345678901234567890123456789012345678901234567890")), "57edf4a22be3c955ac49da2e2107b67a");
    }

    /// A pretend projector on this machine: the PJLink spec's own example random number and password.
    fn fake_projector(answer: &'static str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let handle = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.write_all(b"PJLINK 1 498e4a67\r").unwrap();
            let mut buf = [0u8; 128];
            let n = s.read(&mut buf).unwrap();
            let got = String::from_utf8_lossy(&buf[..n]).to_string();
            s.write_all(answer.as_bytes()).unwrap();
            got
        });
        (addr, handle)
    }

    #[test]
    fn signs_in_and_switches_on() {
        let (addr, server) = fake_projector("%1POWR=OK\r");
        let answers = exchange(&addr, "JBMIAProjectorLink", Ask::PowerOn.commands()).unwrap();
        assert_eq!(answers, vec!["OK"]);
        // The spec's example digest for that random number and password.
        assert_eq!(server.join().unwrap(), "5d8409bc1c3fa39749434aa3a5c38682%1POWR 1\r");
    }

    #[test]
    fn wrong_password_and_projector_errors_are_said_plainly() {
        let (addr, _server) = fake_projector("PJLINK ERRA\r");
        assert_eq!(exchange(&addr, "nope", Ask::PowerOn.commands()).unwrap_err(), "Wrong projector password");
        assert_eq!(says(Ask::PowerOn, &["ERR3".into()]), "Projector says: busy or warming up; try again in a moment");
        assert_eq!(says(Ask::Status, &["1".into(), "Hall left".into(), "31".into()]), "On · Hall left · shutter closed");
        assert!(exchange("127.0.0.1:1", "", &["POWR ?"]).unwrap_err().starts_with("Can't reach"));
    }
}
