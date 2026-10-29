//! Qt 4 `QDataStream` values (`QVariant`s), the way MadMapper saves its projects (.mad): every type found in real
//! projects is read; anything else stops the read with an error (never a panic, whatever the bytes).

/// One value.
#[derive(Clone, Debug, PartialEq)]
pub enum Qv {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    List(Vec<Qv>),
    Map(Vec<(String, Qv)>),
    /// A byte array holding points (a count, then x, y doubles): MadMapper's corners and polygons.
    Points(Vec<[f64; 2]>),
    Bytes(Vec<u8>),
    /// A point, size or rectangle (2 or 4 numbers).
    Nums(Vec<f64>),
    /// Anything kept only so the read can go on (pictures, matrices, colours…).
    Other,
}

impl Qv {
    pub fn get(&self, key: &str) -> Option<&Qv> {
        match self {
            Qv::Map(items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn str(&self) -> Option<&str> {
        match self {
            Qv::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn num(&self) -> Option<f64> {
        match self {
            Qv::Num(n) => Some(*n),
            Qv::Bool(b) => Some(f64::from(u8::from(*b))),
            _ => None,
        }
    }

    pub fn boolean(&self) -> Option<bool> {
        match self {
            Qv::Bool(b) => Some(*b),
            Qv::Num(n) => Some(*n != 0.0),
            _ => None,
        }
    }

    pub fn list(&self) -> &[Qv] {
        match self {
            Qv::List(v) => v,
            _ => &[],
        }
    }

    pub fn points(&self) -> &[[f64; 2]] {
        match self {
            Qv::Points(p) => p,
            _ => &[],
        }
    }

    pub fn nums(&self) -> &[f64] {
        match self {
            Qv::Nums(n) => n,
            _ => &[],
        }
    }
}

/// The marker a MadMapper project starts with.
pub const MAGIC: u32 = 0x0BAD_BABE;

/// A whole MadMapper-style file: the marker, two header numbers, then named values to the end.
pub fn read_file(bytes: &[u8]) -> Result<Vec<(String, Qv)>, String> {
    let mut r = Reader { b: bytes, i: 0 };
    if r.u32()? != MAGIC {
        return Err("not a MadMapper project".into());
    }
    r.u32()?;
    r.u32()?;
    let mut out = Vec::new();
    while r.i < bytes.len() {
        let key = r.string()?.unwrap_or_default();
        let value = r.variant(0)?;
        out.push((key, value));
    }
    Ok(out)
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self.i.checked_add(n).filter(|e| *e <= self.b.len()).ok_or_else(|| format!("the file ends early (at byte {})", self.i))?;
        let s = &self.b[self.i..end];
        self.i = end;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn f64(&mut self) -> Result<f64, String> {
        Ok(f64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn nums(&mut self, n: usize) -> Result<Vec<f64>, String> {
        (0..n).map(|_| self.f64()).collect()
    }

    /// A QString: a byte length (all ones = null), then UTF-16 big-endian.
    fn string(&mut self) -> Result<Option<String>, String> {
        let n = self.u32()?;
        if n == u32::MAX {
            return Ok(None);
        }
        let raw = self.take(n as usize)?;
        let units: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        Ok(Some(String::from_utf16_lossy(&units)))
    }

    fn bytes(&mut self) -> Result<Vec<u8>, String> {
        let n = self.u32()?;
        if n == u32::MAX {
            return Ok(Vec::new());
        }
        Ok(self.take(n as usize)?.to_vec())
    }

    fn count(&mut self) -> Result<usize, String> {
        let n = self.u32()? as usize;
        // (Each item takes at least a byte: a count past what's left is a broken file, not a huge list.)
        if n > self.b.len() - self.i {
            return Err(format!("a list of {n} items at byte {} is longer than the file", self.i));
        }
        Ok(n)
    }

    fn variant(&mut self, depth: usize) -> Result<Qv, String> {
        if depth > 64 {
            return Err("values nested too deep".into());
        }
        let t = self.u32()?;
        self.u8()?;
        self.value(t, depth)
    }

    fn value(&mut self, t: u32, depth: usize) -> Result<Qv, String> {
        Ok(match t {
            // An empty QVariant is written as a null string.
            0 => {
                self.string()?;
                Qv::Null
            }
            1 => Qv::Bool(self.u8()? != 0),
            2 => Qv::Num(f64::from(self.i32()?)),
            3 => Qv::Num(f64::from(self.u32()?)),
            4 | 5 | 129 | 132 => Qv::Num(i64::from_be_bytes(self.take(8)?.try_into().unwrap()) as f64),
            130 | 133 => Qv::Num(f64::from(i16::from_be_bytes(self.take(2)?.try_into().unwrap()))),
            131 | 134 => Qv::Num(f64::from(self.u8()?)),
            // Double, and Qt 4's float (written as a double).
            6 | 135 => Qv::Num(self.f64()?),
            8 => {
                let n = self.count()?;
                let mut items = Vec::with_capacity(n);
                for _ in 0..n {
                    let k = self.string()?.unwrap_or_default();
                    items.push((k, self.variant(depth + 1)?));
                }
                Qv::Map(items)
            }
            9 => {
                let n = self.count()?;
                Qv::List((0..n).map(|_| self.variant(depth + 1)).collect::<Result<_, _>>()?)
            }
            10 => Qv::Str(self.string()?.unwrap_or_default()),
            11 => {
                let n = self.count()?;
                Qv::List((0..n).map(|_| self.string().map(|s| Qv::Str(s.unwrap_or_default()))).collect::<Result<_, _>>()?)
            }
            12 | 145 => points_or_bytes(self.bytes()?),
            19 => Qv::Nums((0..4).map(|_| self.i32().map(f64::from)).collect::<Result<_, _>>()?),
            20 => Qv::Nums(self.nums(4)?),
            21 | 25 => Qv::Nums((0..2).map(|_| self.i32().map(f64::from)).collect::<Result<_, _>>()?),
            22 | 26 => Qv::Nums(self.nums(2)?),
            // A colour: its spec, then alpha, red, green, blue and padding (16 bits each).
            67 => {
                self.take(11)?;
                Qv::Other
            }
            // A picture: a flag, then a PNG (no length: read its chunks to the end marker).
            70 => {
                if self.i32()? != 0 {
                    if self.take(8)? != b"\x89PNG\r\n\x1a\n" {
                        return Err("a picture that isn't a PNG".into());
                    }
                    loop {
                        let n = self.u32()? as usize;
                        let kind = self.take(4)?.to_vec();
                        self.take(n + 4)?;
                        if kind == b"IEND" {
                            break;
                        }
                    }
                }
                Qv::Other
            }
            80 => Qv::Nums(self.nums(6)?),
            81 => Qv::Nums(self.nums(9)?),
            82 => Qv::Nums(self.nums(16)?),
            83..=86 => Qv::Nums(self.nums(t as usize - 81)?),
            // A type of the app's own: its name, then its data.
            127 => {
                let name = String::from_utf8_lossy(&self.bytes()?).trim_end_matches('\0').to_string();
                self.user(&name)?
            }
            other => return Err(format!("a value of an unknown kind ({other}) at byte {}", self.i)),
        })
    }

    fn user(&mut self, name: &str) -> Result<Qv, String> {
        Ok(match name {
            // A colour space name and four numbers (red, green, blue, alpha).
            "MadColor" => {
                self.string()?;
                Qv::Nums(self.nums(4)?)
            }
            "IndexFloatMap" => {
                let n = self.count()?;
                for _ in 0..n {
                    self.i32()?;
                    self.f64()?;
                }
                Qv::Other
            }
            "FilePath" => Qv::Str(self.string()?.unwrap_or_default()),
            _ if name.starts_with("QVector<") || name.starts_with("QList<") => {
                let n = self.count()?;
                if name.contains("int") {
                    Qv::Nums((0..n).map(|_| self.i32().map(f64::from)).collect::<Result<_, _>>()?)
                } else if name.contains("float") || name.contains("double") {
                    Qv::Nums(self.nums(n)?)
                } else {
                    return Err(format!("a list of {name}"));
                }
            }
            other => return Err(format!("a value of MadMapper's own kind {other:?}")),
        })
    }
}

/// A byte array that holds points (a count, then x, y doubles) becomes points; anything else stays bytes.
fn points_or_bytes(b: Vec<u8>) -> Qv {
    if b.len() >= 4 {
        let n = u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize;
        if n.checked_mul(16).is_some_and(|m| m + 4 == b.len()) {
            let at = |k: usize| f64::from_be_bytes(b[k..k + 8].try_into().unwrap());
            return Qv::Points((0..n).map(|k| [at(4 + 16 * k), at(12 + 16 * k)]).collect());
        }
    }
    Qv::Bytes(b)
}

/// Writing, for tests: values in the same layout.
#[cfg(test)]
pub mod write {
    pub fn string(out: &mut Vec<u8>, s: &str) {
        let units: Vec<u16> = s.encode_utf16().collect();
        out.extend((units.len() as u32 * 2).to_be_bytes());
        for u in units {
            out.extend(u.to_be_bytes());
        }
    }
    fn head(out: &mut Vec<u8>, t: u32) {
        out.extend(t.to_be_bytes());
        out.push(0);
    }
    pub fn str_v(out: &mut Vec<u8>, s: &str) {
        head(out, 10);
        string(out, s);
    }
    pub fn num(out: &mut Vec<u8>, n: f64) {
        head(out, 6);
        out.extend(n.to_be_bytes());
    }
    pub fn int(out: &mut Vec<u8>, n: i32) {
        head(out, 2);
        out.extend(n.to_be_bytes());
    }
    pub fn boolean(out: &mut Vec<u8>, b: bool) {
        head(out, 1);
        out.push(u8::from(b));
    }
    pub fn point(out: &mut Vec<u8>, x: f64, y: f64) {
        head(out, 26);
        out.extend(x.to_be_bytes());
        out.extend(y.to_be_bytes());
    }
    pub fn size(out: &mut Vec<u8>, w: f64, h: f64) {
        head(out, 22);
        out.extend(w.to_be_bytes());
        out.extend(h.to_be_bytes());
    }
    pub fn rect(out: &mut Vec<u8>, r: [i32; 4]) {
        head(out, 19);
        for v in r {
            out.extend(v.to_be_bytes());
        }
    }
    pub fn points(out: &mut Vec<u8>, pts: &[[f64; 2]]) {
        head(out, 12);
        out.extend((4 + 16 * pts.len() as u32).to_be_bytes());
        out.extend((pts.len() as u32).to_be_bytes());
        for p in pts {
            out.extend(p[0].to_be_bytes());
            out.extend(p[1].to_be_bytes());
        }
    }
    /// A map: its items written by `items` (each a key then a value), `n` of them.
    pub fn map(out: &mut Vec<u8>, n: u32, items: impl FnOnce(&mut Vec<u8>)) {
        head(out, 8);
        out.extend(n.to_be_bytes());
        items(out);
    }
    pub fn list(out: &mut Vec<u8>, n: u32, items: impl FnOnce(&mut Vec<u8>)) {
        head(out, 9);
        out.extend(n.to_be_bytes());
        items(out);
    }
    pub fn key(out: &mut Vec<u8>, k: &str) {
        string(out, k);
    }
    pub fn file(sections: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
        let mut out = super::MAGIC.to_be_bytes().to_vec();
        out.extend(12u32.to_be_bytes());
        out.extend(44u32.to_be_bytes());
        sections(&mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Values written the way Qt does come back; a cut-short file or a nonsense count is an error, not a panic.
    #[test]
    fn reads_qt_values_and_refuses_broken_files() {
        let bytes = write::file(|o| {
            write::key(o, "name");
            write::str_v(o, "Gig");
            write::key(o, "things");
            write::list(o, 2, |o| {
                write::map(o, 2, |o| {
                    write::key(o, "opacity");
                    write::num(o, 0.5);
                    write::key(o, "corners");
                    write::points(o, &[[0.0, 1.0], [2.0, 3.0]]);
                });
                write::boolean(o, true);
            });
        });
        let v = read_file(&bytes).unwrap();
        assert_eq!(v[0], ("name".into(), Qv::Str("Gig".into())));
        let things = v[1].1.list();
        assert_eq!(things[0].get("opacity").and_then(Qv::num), Some(0.5));
        assert_eq!(things[0].get("corners").unwrap().points(), &[[0.0, 1.0], [2.0, 3.0]]);
        assert_eq!(things[1].boolean(), Some(true));
        for cut in [3, 9, bytes.len() - 3] {
            assert!(read_file(&bytes[..cut]).is_err(), "cut at {cut}");
        }
        let mut huge = write::file(|o| {
            write::key(o, "x");
            write::list(o, 0, |_| {});
        });
        let at = huge.len() - 4;
        huge[at..].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(read_file(&huge).is_err());
        assert!(read_file(b"not a project at all").is_err());
    }
}
