//! HTTPS for the session: a self-signed certificate made once and kept in the app's folder, and a
//! connection type that is either plain TCP or TLS so the same page/socket code serves both.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

/// The TLS setup: a self-signed certificate kept as `session-cert.der` / `session-key.der` next to the
/// projects (remade when this Mac's address changes). Phones show a warning once and go on.
pub fn server_config() -> Result<Arc<rustls::ServerConfig>, String> {
    let dir = crate::show::storage_dir().ok_or("no app folder")?;
    let (cert_path, key_path, names_path) = (dir.join("session-cert.der"), dir.join("session-key.der"), dir.join("session-cert.names"));
    let ip = crate::control::local_ip().unwrap_or_else(|| "127.0.0.1".into());
    let names = format!("localhost,{ip},phosphor.local");
    let same_names = std::fs::read_to_string(&names_path).is_ok_and(|n| n.trim() == names);
    let (cert, key) = match (same_names, std::fs::read(&cert_path), std::fs::read(&key_path)) {
        (true, Ok(c), Ok(k)) => (c, k),
        _ => {
            let made = rcgen::generate_simple_self_signed(names.split(',').map(str::to_string).collect::<Vec<_>>()).map_err(|e| e.to_string())?;
            let c = made.cert.der().to_vec();
            let k = made.key_pair.serialize_der();
            std::fs::write(&cert_path, &c).map_err(|e| e.to_string())?;
            std::fs::write(&key_path, &k).map_err(|e| e.to_string())?;
            std::fs::write(&names_path, &names).map_err(|e| e.to_string())?;
            (c, k)
        }
    };
    let certs = vec![CertificateDer::from(cert)];
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key));
    let config = rustls::ServerConfig::builder().with_no_client_auth().with_single_cert(certs, key).map_err(|e| e.to_string())?;
    Ok(Arc::new(config))
}

/// A client connection, plain or encrypted.
pub enum Conn {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ServerConnection, TcpStream>>),
}

impl Conn {
    pub fn new(stream: TcpStream, tls: Option<&Arc<rustls::ServerConfig>>) -> Result<Self, String> {
        match tls {
            None => Ok(Conn::Plain(stream)),
            Some(cfg) => {
                let conn = rustls::ServerConnection::new(cfg.clone()).map_err(|e| e.to_string())?;
                Ok(Conn::Tls(Box::new(rustls::StreamOwned::new(conn, stream))))
            }
        }
    }
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Conn::Plain(s) => s.read(buf),
            Conn::Tls(s) => s.read(buf),
        }
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Conn::Plain(s) => s.write(buf),
            Conn::Tls(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Conn::Plain(s) => s.flush(),
            Conn::Tls(s) => s.flush(),
        }
    }
}

/// A stream whose first bytes were already read (the request head): hands them out again first.
pub struct Replay {
    head: Vec<u8>,
    pos: usize,
    pub inner: Conn,
}

impl Replay {
    pub fn new(head: Vec<u8>, inner: Conn) -> Self {
        Self { head, pos: 0, inner }
    }
}

impl Read for Replay {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos < self.head.len() {
            let n = (self.head.len() - self.pos).min(buf.len());
            buf[..n].copy_from_slice(&self.head[self.pos..self.pos + n]);
            self.pos += n;
            return Ok(n);
        }
        self.inner.read(buf)
    }
}

impl Write for Replay {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
