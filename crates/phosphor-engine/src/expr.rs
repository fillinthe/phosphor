//! Tiny arithmetic for multi-pass buffer sizes (ISF `WIDTH` / `HEIGHT`): numbers, variables
//! (`$WIDTH`, `$HEIGHT`, input names), `+ - * / %`, parentheses, unary minus, comparisons and
//! `floor ceil round abs min max pow sqrt`, and for control formulas `sin cos` (a turn per 1.0, 0..1 out),
//! `fract clamp mix`, choices (`if(c, a, b)`, `not`, `< <= > >= == !=`, `&&`, `||`; true = 1, false = 0) and
//! named steps: `x = $VAL * 2; if(x > 1, 2 - x, x)` (each `name = …;` can be used by the ones after it).

pub fn eval(src: &str, var: &dyn Fn(&str) -> Option<f64>) -> Option<f64> {
    // (Brackets nest as deep as the text is long: a limit on the length keeps a pasted monster off the stack.)
    if src.len() > MAX_LEN {
        return None;
    }
    let mut parts: Vec<&str> = src.split(';').collect();
    // (A `;` at the very end is allowed.)
    if parts.len() > 1 && parts.last().is_some_and(|p| p.trim().is_empty()) {
        parts.pop();
    }
    let last = parts.pop()?;
    let mut named: Vec<(&str, f64)> = Vec::new();
    for step in parts {
        let (name, body) = assignment(step)?;
        let v = {
            let lookup = |n: &str| named.iter().rev().find(|(k, _)| *k == n).map(|(_, v)| *v).or_else(|| var(n));
            one(body, &lookup)?
        };
        named.push((name, v));
    }
    let lookup = |n: &str| named.iter().rev().find(|(k, _)| *k == n).map(|(_, v)| *v).or_else(|| var(n));
    one(last, &lookup)
}

/// `name = expression` (a single `=`, not part of `==` `<=` `>=` `!=`), the name a plain word.
fn assignment(step: &str) -> Option<(&str, &str)> {
    let b = step.as_bytes();
    let at = (0..b.len()).find(|&i| b[i] == b'=' && b.get(i + 1) != Some(&b'=') && (i == 0 || !b"=<>!".contains(&b[i - 1])))?;
    let name = step[..at].trim();
    let word = name.bytes().enumerate().all(|(i, c)| c == b'_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit()) || (i == 0 && c == b'$'));
    (word && !name.is_empty()).then(|| (name, &step[at + 1..]))
}

fn one(src: &str, var: &dyn Fn(&str) -> Option<f64>) -> Option<f64> {
    let mut p = Parser { s: src.as_bytes(), i: 0, var };
    let v = p.or()?;
    p.ws();
    // (Not a number, or infinite: no answer, rather than a value that poisons whatever uses it.)
    (p.i == p.s.len() && v.is_finite()).then_some(v)
}

/// The longest formula read.
const MAX_LEN: usize = 1024;

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    var: &'a dyn Fn(&str) -> Option<f64>,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) -> bool {
        self.ws();
        if self.s.get(self.i) == Some(&c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn eat2(&mut self, two: &[u8; 2]) -> bool {
        self.ws();
        if self.s[self.i.min(self.s.len())..].starts_with(two) {
            self.i += 2;
            true
        } else {
            false
        }
    }

    fn or(&mut self) -> Option<f64> {
        let mut v = self.and()?;
        while self.eat2(b"||") {
            let b = self.and()?;
            v = (v != 0.0 || b != 0.0) as u8 as f64;
        }
        Some(v)
    }

    fn and(&mut self) -> Option<f64> {
        let mut v = self.cmp()?;
        while self.eat2(b"&&") {
            let b = self.cmp()?;
            v = (v != 0.0 && b != 0.0) as u8 as f64;
        }
        Some(v)
    }

    fn cmp(&mut self) -> Option<f64> {
        let a = self.add()?;
        let truth = |t: bool| Some(t as u8 as f64);
        if self.eat2(b"<=") {
            return truth(a <= self.add()?);
        }
        if self.eat2(b">=") {
            return truth(a >= self.add()?);
        }
        if self.eat2(b"==") {
            return truth(a == self.add()?);
        }
        if self.eat2(b"!=") {
            return truth(a != self.add()?);
        }
        if self.eat(b'<') {
            return truth(a < self.add()?);
        }
        if self.eat(b'>') {
            return truth(a > self.add()?);
        }
        Some(a)
    }

    fn add(&mut self) -> Option<f64> {
        let mut v = self.mul()?;
        loop {
            if self.eat(b'+') {
                v += self.mul()?;
            } else if self.eat(b'-') {
                v -= self.mul()?;
            } else {
                return Some(v);
            }
        }
    }

    fn mul(&mut self) -> Option<f64> {
        let mut v = self.unary()?;
        loop {
            if self.eat(b'*') {
                v *= self.unary()?;
            } else if self.eat(b'/') {
                let d = self.unary()?;
                v = if d == 0.0 { 0.0 } else { v / d };
            } else if self.eat(b'%') {
                let d = self.unary()?;
                v = if d == 0.0 { 0.0 } else { v % d };
            } else {
                return Some(v);
            }
        }
    }

    fn unary(&mut self) -> Option<f64> {
        if self.eat(b'-') {
            return Some(-self.unary()?);
        }
        if self.eat(b'+') {
            return self.unary();
        }
        self.atom()
    }

    fn atom(&mut self) -> Option<f64> {
        self.ws();
        if self.eat(b'(') {
            let v = self.or()?;
            return self.eat(b')').then_some(v);
        }
        let start = self.i;
        let c = *self.s.get(self.i)?;
        if c.is_ascii_digit() || c == b'.' {
            while self.i < self.s.len() && (self.s[self.i].is_ascii_digit() || self.s[self.i] == b'.') {
                self.i += 1;
            }
            return std::str::from_utf8(&self.s[start..self.i]).ok()?.parse().ok();
        }
        if c == b'$' || c.is_ascii_alphabetic() || c == b'_' {
            self.i += 1;
            while self.i < self.s.len() && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'_') {
                self.i += 1;
            }
            let name = std::str::from_utf8(&self.s[start..self.i]).ok()?;
            if self.eat(b'(') {
                let mut args = Vec::new();
                if !self.eat(b')') {
                    loop {
                        args.push(self.or()?);
                        if self.eat(b')') {
                            break;
                        }
                        if !self.eat(b',') {
                            return None;
                        }
                    }
                }
                let a = |k: usize| args.get(k).copied();
                return match name {
                    "floor" => a(0).map(f64::floor),
                    "ceil" => a(0).map(f64::ceil),
                    "round" => a(0).map(f64::round),
                    "abs" => a(0).map(f64::abs),
                    "sqrt" => a(0).map(|v| v.max(0.0).sqrt()),
                    "min" => Some(a(0)?.min(a(1)?)),
                    "max" => Some(a(0)?.max(a(1)?)),
                    "pow" => Some(a(0)?.powf(a(1)?)),
                    // One turn per 1.0 (so `sin(beat)` goes round once a beat), 0..1 out.
                    "sin" => a(0).map(|v| (v * std::f64::consts::TAU).sin() * 0.5 + 0.5),
                    "cos" => a(0).map(|v| (v * std::f64::consts::TAU).cos() * 0.5 + 0.5),
                    "fract" => a(0).map(|v| v.rem_euclid(1.0)),
                    // (`f64::clamp` panics on a limit that isn't a number.)
                    "clamp" => Some(a(0)?.max(a(1)?).min(a(2)?.max(a(1)?))),
                    "mix" => Some(a(0)? + (a(1)? - a(0)?) * a(2)?),
                    "if" => Some(if a(0)? != 0.0 { a(1)? } else { a(2)? }),
                    "not" => a(0).map(|v| (v == 0.0) as u8 as f64),
                    _ => None,
                };
            }
            return (self.var)(name);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn evaluates_isf_size_expressions() {
        let v = |n: &str| match n {
            "$WIDTH" => Some(1920.0),
            "$HEIGHT" => Some(1080.0),
            "blurAmount" => Some(4.0),
            _ => None,
        };
        assert_eq!(super::eval("$WIDTH/4.0", &v), Some(480.0));
        assert_eq!(super::eval("floor($HEIGHT / (blurAmount + 1.0))", &v), Some(216.0));
        assert_eq!(super::eval("max(1.0, -2 * 3)", &v), Some(1.0));
        assert_eq!(super::eval("$WIDTH > 1000", &v), Some(1.0));
        assert_eq!(super::eval("nonsense(", &v), None);
        // A limit that isn't a number, an endless answer, a monster of brackets: no answer and no crash.
        assert_eq!(super::eval("clamp(2, pow(-1, 0.5), 1)", &v), Some(1.0));
        assert_eq!(super::eval("clamp(5, 0, 1)", &v), Some(1.0));
        assert_eq!(super::eval("clamp(-5, 0, 1)", &v), Some(0.0));
        assert_eq!(super::eval("pow(-1, 0.5)", &v), None);
        assert_eq!(super::eval(&format!("{}1{}", "(".repeat(100_000), ")".repeat(100_000)), &v), None);
    }

    #[test]
    fn choices_comparisons_and_named_steps() {
        let v = |n: &str| (n == "$VAL").then_some(0.75);
        let e = |s: &str| super::eval(s, &v);
        assert_eq!(e("if($VAL > 0.5, 1, 0)"), Some(1.0));
        assert_eq!(e("if($VAL <= 0.5, 1, 0.25)"), Some(0.25));
        assert_eq!(e("$VAL >= 0.75"), Some(1.0));
        assert_eq!(e("$VAL == 0.75 && $VAL != 1"), Some(1.0));
        assert_eq!(e("$VAL < 0.1 || $VAL > 0.9"), Some(0.0));
        assert_eq!(e("not($VAL > 0.5)"), Some(0.0));
        // Named steps: each can use the ones before it; a `;` at the end is fine.
        assert_eq!(e("x = $VAL * 2; if(x > 1, 2 - x, x)"), Some(0.5));
        assert_eq!(e("x = $VAL; y = x * 2; y + x;"), Some(2.25));
        // A step that isn't `name = …`, an unknown name, a step that doesn't add up: no answer.
        assert_eq!(e("$VAL * 2; 1"), None);
        assert_eq!(e("x = 1; z"), None);
        assert_eq!(e("x = pow(; x"), None);
        assert_eq!(e("2x = 1; 1"), None);
    }
}
