//! Compact JSON parser for edit input — hand-rolled, zero-dependency.
//! Objects preserve insertion order and duplicate keys are rejected.

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    I64(i64),
    F64(f64),
    Str(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::I64(n) => Some(*n),
            Value::F64(f) if f.fract() == 0.0 && f.is_finite() => Some(*f as i64),
            _ => None,
        }
    }
    /// Render a scalar edit value as the plain string the writers store.
    pub fn scalar_string(&self) -> Option<String> {
        match self {
            Value::Str(s) => Some(s.clone()),
            Value::I64(n) => Some(n.to_string()),
            Value::F64(f) if f.is_finite() => Some(f.to_string()),
            Value::Bool(b) => Some(b.to_string()),
            _ => None,
        }
    }
}

pub const MAX_DEPTH: usize = 64;

pub fn parse(text: &str) -> Result<Value, String> {
    let mut p = Parser {
        bytes: text.as_bytes(),
        pos: 0,
    };
    p.skip_ws();
    let v = p.value(0)?;
    p.skip_ws();
    if p.pos != p.bytes.len() {
        return Err(format!("trailing characters at byte {}", p.pos));
    }
    Ok(v)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }
    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }
    fn expect(&mut self, b: u8) -> Result<(), String> {
        if self.peek() == Some(b) {
            self.pos += 1;
            Ok(())
        } else {
            Err(format!("expected '{}' at byte {}", b as char, self.pos))
        }
    }
    fn value(&mut self, depth: usize) -> Result<Value, String> {
        if depth > MAX_DEPTH {
            return Err("nesting too deep".into());
        }
        match self.peek() {
            Some(b'{') => {
                self.pos += 1;
                let mut pairs: Vec<(String, Value)> = Vec::new();
                self.skip_ws();
                if self.peek() == Some(b'}') {
                    self.pos += 1;
                    return Ok(Value::Object(pairs));
                }
                loop {
                    self.skip_ws();
                    let key = self.string()?;
                    if pairs.iter().any(|(k, _)| *k == key) {
                        return Err(format!("duplicate key \"{key}\""));
                    }
                    self.skip_ws();
                    self.expect(b':')?;
                    self.skip_ws();
                    let v = self.value(depth + 1)?;
                    pairs.push((key, v));
                    self.skip_ws();
                    match self.peek() {
                        Some(b',') => self.pos += 1,
                        Some(b'}') => {
                            self.pos += 1;
                            return Ok(Value::Object(pairs));
                        }
                        _ => return Err(format!("expected ',' or '}}' at byte {}", self.pos)),
                    }
                }
            }
            Some(b'[') => {
                self.pos += 1;
                let mut items = Vec::new();
                self.skip_ws();
                if self.peek() == Some(b']') {
                    self.pos += 1;
                    return Ok(Value::Array(items));
                }
                loop {
                    self.skip_ws();
                    items.push(self.value(depth + 1)?);
                    self.skip_ws();
                    match self.peek() {
                        Some(b',') => self.pos += 1,
                        Some(b']') => {
                            self.pos += 1;
                            return Ok(Value::Array(items));
                        }
                        _ => return Err(format!("expected ',' or ']' at byte {}", self.pos)),
                    }
                }
            }
            Some(b'"') => Ok(Value::Str(self.string()?)),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'n') => self.literal("null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(format!("unexpected character at byte {}", self.pos)),
        }
    }
    fn literal(&mut self, lit: &str, v: Value) -> Result<Value, String> {
        if self.bytes[self.pos..].starts_with(lit.as_bytes()) {
            self.pos += lit.len();
            Ok(v)
        } else {
            Err(format!("invalid literal at byte {}", self.pos))
        }
    }
    fn number(&mut self) -> Result<Value, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while matches!(
            self.peek(),
            Some(b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
        ) {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos]).unwrap_or("");
        if let Ok(n) = text.parse::<i64>() {
            return Ok(Value::I64(n));
        }
        match text.parse::<f64>() {
            Ok(f) if f.is_finite() => Ok(Value::F64(f)),
            _ => Err(format!("invalid number at byte {start}")),
        }
    }
    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let b = self
                .peek()
                .ok_or_else(|| "unterminated string".to_string())?;
            self.pos += 1;
            match b {
                b'"' => return Ok(out),
                b'\\' => {
                    let e = self
                        .peek()
                        .ok_or_else(|| "unterminated escape".to_string())?;
                    self.pos += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000C}'),
                        b'u' => {
                            let hex = self
                                .bytes
                                .get(self.pos..self.pos + 4)
                                .ok_or("bad \\u escape")?;
                            let s = std::str::from_utf8(hex).map_err(|_| "bad \\u escape")?;
                            let cp = u32::from_str_radix(s, 16).map_err(|_| "bad \\u escape")?;
                            self.pos += 4;
                            // Surrogate pairs are combined only in the
                            // common BMP-followed-by-low case; a lone
                            // surrogate becomes U+FFFD.
                            if (0xD800..0xDC00).contains(&cp) {
                                if self.bytes.get(self.pos..self.pos + 2) == Some(b"\\u") {
                                    let hex2 = self.bytes.get(self.pos + 2..self.pos + 6);
                                    if let Some(h2) = hex2 {
                                        if let Ok(lo) = u32::from_str_radix(
                                            std::str::from_utf8(h2).unwrap_or(""),
                                            16,
                                        ) {
                                            if (0xDC00..0xE000).contains(&lo) {
                                                self.pos += 6;
                                                let c =
                                                    0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                                                out.push(char::from_u32(c).unwrap_or('\u{FFFD}'));
                                                continue;
                                            }
                                        }
                                    }
                                }
                                out.push('\u{FFFD}');
                            } else {
                                out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
                            }
                        }
                        _ => return Err(format!("bad escape at byte {}", self.pos)),
                    }
                }
                _ => {
                    // Reassemble the UTF-8 sequence starting at b.
                    let start = self.pos - 1;
                    let len = if b < 0x80 {
                        1
                    } else if b >= 0xF0 {
                        4
                    } else if b >= 0xE0 {
                        3
                    } else {
                        2
                    };
                    let end = (start + len).min(self.bytes.len());
                    match std::str::from_utf8(&self.bytes[start..end]) {
                        Ok(s) => {
                            out.push_str(s);
                            self.pos = end;
                        }
                        Err(_) => out.push('\u{FFFD}'),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_object_and_rejects_duplicates() {
        let v = parse(r#"{"a":"x","n":3}"#).unwrap();
        assert_eq!(v.get("a").unwrap().as_str(), Some("x"));
        assert_eq!(v.get("n").unwrap().as_i64(), Some(3));
        assert!(parse(r#"{"a":1,"a":2}"#).is_err());
        assert!(parse(r#"{"a":}"#).is_err());
        assert!(parse("").is_err());
    }
}
