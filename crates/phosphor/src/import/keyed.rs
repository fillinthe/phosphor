//! Archived Cocoa objects (`NSKeyedArchiver` binary property lists, as VDMX keeps its layers and effects): the
//! object table resolved into one tree of plain values. Dictionaries and arrays become `Ar::Dict` / `Ar::List`;
//! other archived objects become a `Dict` of their fields (with `$class`). Loops and very deep nesting stop at
//! `Ar::Null` (never a panic or a hang, whatever the bytes).

/// A resolved value.
#[derive(Clone, Debug, PartialEq)]
pub enum Ar {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Data(Vec<u8>),
    List(Vec<Ar>),
    Dict(Vec<(String, Ar)>),
}

impl Ar {
    pub fn get(&self, key: &str) -> Option<&Ar> {
        match self {
            Ar::Dict(items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// A key's value, two or more levels down ("CmpCtrlr", "opacity", "val").
    pub fn at(&self, path: &[&str]) -> Option<&Ar> {
        path.iter().try_fold(self, |v, k| v.get(k))
    }

    pub fn str(&self) -> Option<&str> {
        match self {
            Ar::Str(s) => Some(s),
            _ => None,
        }
    }

    /// A number (a number in a string counts: VDMX keeps some values as text).
    pub fn num(&self) -> Option<f64> {
        match self {
            Ar::Num(n) => Some(*n),
            Ar::Bool(b) => Some(f64::from(u8::from(*b))),
            Ar::Str(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    pub fn list(&self) -> &[Ar] {
        match self {
            Ar::List(v) => v,
            _ => &[],
        }
    }

    pub fn dict(&self) -> &[(String, Ar)] {
        match self {
            Ar::Dict(v) => v,
            _ => &[],
        }
    }
}

/// Unarchives `bytes` (a binary or XML property list written by `NSKeyedArchiver`) from its root object.
pub fn unarchive(bytes: &[u8]) -> Result<Ar, String> {
    let top = plist::Value::from_reader(std::io::Cursor::new(bytes)).map_err(|e| format!("not a property list ({e})"))?;
    let top = top.as_dictionary().ok_or("not an archive")?;
    let objects = top.get("$objects").and_then(plist::Value::as_array).ok_or("not an archive (no objects)")?;
    let root = top.get("$top").and_then(plist::Value::as_dictionary).and_then(|t| t.get("root").or_else(|| t.values().next())).ok_or("not an archive (no root)")?;
    let mut r = Resolver { objects, busy: vec![false; objects.len()], steps: 0 };
    Ok(r.value(root, 0))
}

struct Resolver<'a> {
    objects: &'a [plist::Value],
    /// Objects being resolved now (a reference back to one is a loop: it becomes Null).
    busy: Vec<bool>,
    /// Values made so far (a cap keeps a huge shared table from blowing up when it's resolved many times).
    steps: usize,
}

const MAX_DEPTH: usize = 48;
const MAX_STEPS: usize = 2_000_000;

impl Resolver<'_> {
    fn value(&mut self, v: &plist::Value, depth: usize) -> Ar {
        self.steps += 1;
        if depth > MAX_DEPTH || self.steps > MAX_STEPS {
            return Ar::Null;
        }
        match v {
            plist::Value::Uid(uid) => {
                let i = uid.get() as usize;
                if i >= self.objects.len() || self.busy[i] {
                    return Ar::Null;
                }
                self.busy[i] = true;
                let out = self.object(&self.objects[i], depth + 1);
                self.busy[i] = false;
                out
            }
            other => self.object(other, depth + 1),
        }
    }

    fn object(&mut self, o: &plist::Value, depth: usize) -> Ar {
        match o {
            plist::Value::String(s) if s == "$null" => Ar::Null,
            plist::Value::String(s) => Ar::Str(s.clone()),
            plist::Value::Boolean(b) => Ar::Bool(*b),
            plist::Value::Integer(n) => Ar::Num(n.as_signed().map_or_else(|| n.as_unsigned().unwrap_or(0) as f64, |n| n as f64)),
            plist::Value::Real(n) => Ar::Num(*n),
            plist::Value::Data(d) => Ar::Data(d.clone()),
            plist::Value::Array(a) => Ar::List(a.iter().map(|x| self.value(x, depth)).collect()),
            plist::Value::Dictionary(d) => {
                let keys = d.get("NS.keys").and_then(plist::Value::as_array);
                let objects = d.get("NS.objects").and_then(plist::Value::as_array);
                match (keys, objects) {
                    // NSDictionary: keys and values side by side.
                    (Some(k), Some(v)) => Ar::Dict(
                        k.iter()
                            .zip(v)
                            .map(|(k, v)| {
                                let key = match self.value(k, depth) {
                                    Ar::Str(s) => s,
                                    other => format!("{other:?}"),
                                };
                                (key, self.value(v, depth))
                            })
                            .collect(),
                    ),
                    // NSArray / NSSet.
                    (None, Some(v)) => Ar::List(v.iter().map(|x| self.value(x, depth)).collect()),
                    _ => {
                        if let Some(s) = d.get("NS.string").and_then(plist::Value::as_string) {
                            return Ar::Str(s.to_string());
                        }
                        if let Some(b) = d.get("NS.bytes").and_then(plist::Value::as_data).or_else(|| d.get("NS.data").and_then(plist::Value::as_data)) {
                            return Ar::Data(b.to_vec());
                        }
                        // Any other archived object: its fields, and its class's name.
                        let mut fields: Vec<(String, Ar)> = Vec::new();
                        for (k, v) in d.iter() {
                            if k == "$class" {
                                let name = self.value(v, depth).get("$classname").and_then(Ar::str).unwrap_or("").to_string();
                                fields.push(("$class".into(), Ar::Str(name)));
                            } else {
                                fields.push((k.clone(), self.value(v, depth)));
                            }
                        }
                        Ar::Dict(fields)
                    }
                }
            }
            _ => Ar::Null,
        }
    }
}

#[cfg(test)]
pub mod sample {
    use plist::{Dictionary, Uid, Value};

    /// Builds an archive the way NSKeyedArchiver lays one out: `$objects` (0 = "$null"), `$top.root`.
    pub struct Archive {
        pub objects: Vec<Value>,
    }

    impl Archive {
        pub fn new() -> Self {
            Archive { objects: vec![Value::String("$null".into())] }
        }
        pub fn add(&mut self, v: Value) -> Value {
            self.objects.push(v);
            Value::Uid(Uid::new(self.objects.len() as u64 - 1))
        }
        fn class(&mut self, name: &str) -> Value {
            let mut d = Dictionary::new();
            d.insert("$classname".into(), Value::String(name.into()));
            d.insert("$classes".into(), Value::Array(vec![Value::String(name.into())]));
            self.add(Value::Dictionary(d))
        }
        pub fn string(&mut self, s: &str) -> Value {
            self.add(Value::String(s.into()))
        }
        pub fn dict(&mut self, items: Vec<(&str, Value)>) -> Value {
            let mut keys = Vec::new();
            let mut vals = Vec::new();
            for (k, v) in items {
                keys.push(self.string(k));
                vals.push(v);
            }
            let class = self.class("NSMutableDictionary");
            let mut d = Dictionary::new();
            d.insert("NS.keys".into(), Value::Array(keys));
            d.insert("NS.objects".into(), Value::Array(vals));
            d.insert("$class".into(), class);
            self.add(Value::Dictionary(d))
        }
        pub fn array(&mut self, items: Vec<Value>) -> Value {
            let class = self.class("NSMutableArray");
            let mut d = Dictionary::new();
            d.insert("NS.objects".into(), Value::Array(items));
            d.insert("$class".into(), class);
            self.add(Value::Dictionary(d))
        }
        /// The archive's bytes (a binary property list) with `root` at the top.
        pub fn bytes(self, root: Value) -> Vec<u8> {
            let mut top = Dictionary::new();
            top.insert("root".into(), root);
            let mut d = Dictionary::new();
            d.insert("$version".into(), Value::Integer(100000.into()));
            d.insert("$archiver".into(), Value::String("NSKeyedArchiver".into()));
            d.insert("$top".into(), Value::Dictionary(top));
            d.insert("$objects".into(), Value::Array(self.objects));
            let mut out = Vec::new();
            Value::Dictionary(d).to_writer_binary(&mut out).unwrap();
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::sample::Archive;
    use super::*;
    use plist::Value;

    /// Dictionaries, arrays, strings and numbers come back as one tree; a loop stops at Null; junk is an error.
    #[test]
    fn unarchives_nested_objects() {
        let mut a = Archive::new();
        let name = a.string("Headliner");
        let val = a.dict(vec![("val", Value::Real(0.25))]);
        let layer = a.dict(vec![("name", name), ("opacity", val)]);
        let layers = a.array(vec![layer]);
        let root = a.dict(vec![("lyrs", layers)]);
        // A loop: the root's array also holds the root itself.
        let Value::Uid(root_uid) = root else { unreachable!() };
        let arr_index = a.objects.iter().position(|o| o.as_dictionary().is_some_and(|d| d.contains_key("NS.objects") && !d.contains_key("NS.keys"))).unwrap();
        if let Some(Value::Array(items)) = a.objects[arr_index].as_dictionary_mut().and_then(|d| d.get_mut("NS.objects")) {
            items.push(Value::Uid(root_uid));
        }
        let tree = unarchive(&a.bytes(Value::Uid(root_uid))).unwrap();
        let first = &tree.get("lyrs").unwrap().list()[0];
        assert_eq!(first.get("name").and_then(Ar::str), Some("Headliner"));
        assert_eq!(first.at(&["opacity", "val"]).and_then(Ar::num), Some(0.25));
        assert_eq!(tree.get("lyrs").unwrap().list()[1], Ar::Null, "a loop back to the root");
        assert!(unarchive(b"bplist00 not really").is_err());
        assert!(unarchive(b"").is_err());
    }
}
