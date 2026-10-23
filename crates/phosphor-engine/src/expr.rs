//! Tiny arithmetic for multi-pass buffer sizes (ISF `WIDTH` / `HEIGHT`): numbers, variables
//! (`$WIDTH`, `$HEIGHT`, input names), `+ - * / %`, parentheses, unary minus, comparisons and
//! `floor ceil round abs min max pow sqrt`, and for control formulas `sin cos` (a turn per 1.0, 0..1 out),
//! `fract clamp mix`.

pub fn eval(src: &str, var: &dyn Fn(&str) -> Option<f64>) -> Option<f64> {
    let mut p = Parser { s: src.as_bytes(), i: 0, var };
    let v = p.cmp()?;
    p.ws();
    (p.i == p.s.len()).then_some(v)
}

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

    fn cmp(&mut self) -> Option<f64> {
        let a = self.add()?;
        if self.eat(b'<') {
            let b = self.add()?;
            return Some((a < b) as u8 as f64);
        }
        if self.eat(b'>') {
            let b = self.add()?;
            return Some((a > b) as u8 as f64);
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
            let v = self.cmp()?;
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
                        args.push(self.cmp()?);
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
                    "clamp" => Some(a(0)?.clamp(a(1)?, a(2)?.max(a(1)?))),
                    "mix" => Some(a(0)? + (a(1)? - a(0)?) * a(2)?),
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
    }
}
