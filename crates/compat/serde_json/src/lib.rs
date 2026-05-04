use std::collections::BTreeMap;
use std::fmt;
use std::ops::Index;
use std::str::FromStr;

pub type Map = BTreeMap<String, Value>;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(String);

impl Error {
    fn new<T: Into<String>>(message: T) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Value>),
    Object(Map),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Number(String);

impl fmt::Display for Number {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for Number {
    type Err = Error;
    fn from_str(input: &str) -> Result<Self> {
        let mut parser = Parser::new(input);
        let (number, _, _) = parser.parse_number()?;
        if !parser.is_eof() {
            return Err(parser.err("trailing characters after JSON number"));
        }
        Ok(Number(number))
    }
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(map) => map.get(key),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(value) => Some(value),
            _ => None,
        }
    }
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(value) => Some(*value),
            _ => None,
        }
    }
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::Number(value) => value.0.parse::<u64>().ok(),
            _ => None,
        }
    }
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }
    pub fn pointer(&self, pointer: &str) -> Option<&Value> {
        if pointer.is_empty() {
            return Some(self);
        }
        let mut current = self;
        for raw in pointer.strip_prefix('/')?.split('/') {
            let key = raw.replace("~1", "/").replace("~0", "~");
            current = match current {
                Value::Object(map) => map.get(&key)?,
                Value::Array(items) => items.get(key.parse::<usize>().ok()?)?,
                _ => return None,
            };
        }
        Some(current)
    }
}

static NULL_VALUE: Value = Value::Null;

impl Index<&str> for Value {
    type Output = Value;
    fn index(&self, key: &str) -> &Self::Output {
        match self {
            Value::Object(map) => map.get(key).unwrap_or(&NULL_VALUE),
            _ => &NULL_VALUE,
        }
    }
}

impl Index<usize> for Value {
    type Output = Value;
    fn index(&self, index: usize) -> &Self::Output {
        match self {
            Value::Array(items) => items.get(index).unwrap_or(&NULL_VALUE),
            _ => &NULL_VALUE,
        }
    }
}

pub trait FromJson: Sized {
    fn from_json_text(input: &str) -> Result<Self>;
}
impl FromJson for Value {
    fn from_json_text(input: &str) -> Result<Self> {
        parse_value(input)
    }
}
impl FromJson for String {
    fn from_json_text(input: &str) -> Result<Self> {
        match parse_value(input)? {
            Value::String(value) => Ok(value),
            _ => Err(Error::new("expected JSON string")),
        }
    }
}

pub fn from_str<T: FromJson>(input: &str) -> Result<T> {
    T::from_json_text(input)
}
pub fn to_string(value: &Value) -> Result<String> {
    Ok(write_value(value, false, 0))
}
pub fn to_string_pretty(value: &Value) -> Result<String> {
    Ok(write_value(value, true, 0))
}

pub trait IntoValue {
    fn into_value(self) -> Value;
}
impl IntoValue for Value {
    fn into_value(self) -> Value {
        self
    }
}
impl IntoValue for &Value {
    fn into_value(self) -> Value {
        self.clone()
    }
}
impl IntoValue for String {
    fn into_value(self) -> Value {
        Value::String(self)
    }
}
impl IntoValue for &String {
    fn into_value(self) -> Value {
        Value::String(self.clone())
    }
}
impl IntoValue for &str {
    fn into_value(self) -> Value {
        Value::String(self.to_string())
    }
}
impl IntoValue for bool {
    fn into_value(self) -> Value {
        Value::Bool(self)
    }
}
impl IntoValue for i32 {
    fn into_value(self) -> Value {
        Value::Number(Number(self.to_string()))
    }
}
impl IntoValue for i64 {
    fn into_value(self) -> Value {
        Value::Number(Number(self.to_string()))
    }
}
impl IntoValue for u16 {
    fn into_value(self) -> Value {
        Value::Number(Number(self.to_string()))
    }
}
impl IntoValue for u64 {
    fn into_value(self) -> Value {
        Value::Number(Number(self.to_string()))
    }
}
impl IntoValue for usize {
    fn into_value(self) -> Value {
        Value::Number(Number(self.to_string()))
    }
}
impl<T: IntoValue> IntoValue for Option<T> {
    fn into_value(self) -> Value {
        self.map(IntoValue::into_value).unwrap_or(Value::Null)
    }
}
pub fn to_value<T: IntoValue>(value: T) -> Value {
    value.into_value()
}

#[macro_export]
macro_rules! json {
    (null) => { $crate::Value::Null };
    ({ $($key:literal : $value:expr),* $(,)? }) => {{
        let mut map = $crate::Map::new();
        $( map.insert($key.to_string(), $crate::to_value($value)); )*
        $crate::Value::Object(map)
    }};
    ([ $($value:expr),* $(,)? ]) => {{
        $crate::Value::Array(vec![$($crate::to_value($value)),*])
    }};
    ($value:expr) => { $crate::to_value($value) };
}

fn parse_value(input: &str) -> Result<Value> {
    let mut parser = Parser::new(input);
    let value = parser.parse_value()?;
    parser.skip_ws();
    if !parser.is_eof() {
        return Err(parser.err("trailing characters after JSON value"));
    }
    Ok(value)
}

struct Parser<'a> {
    input: &'a str,
    index: usize,
}
impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, index: 0 }
    }
    fn is_eof(&self) -> bool {
        self.index >= self.input.len()
    }
    fn current(&self) -> Option<char> {
        self.input[self.index..].chars().next()
    }
    fn bump(&mut self) -> Option<char> {
        let ch = self.current()?;
        self.index += ch.len_utf8();
        Some(ch)
    }
    fn skip_ws(&mut self) {
        while matches!(self.current(), Some(' ' | '\n' | '\r' | '\t')) {
            self.bump();
        }
    }
    fn err(&self, message: &str) -> Error {
        Error::new(format!("{message} at byte {}", self.index))
    }
    fn expect(&mut self, literal: &str) -> Result<()> {
        if self.input[self.index..].starts_with(literal) {
            self.index += literal.len();
            Ok(())
        } else {
            Err(self.err(&format!("expected {literal}")))
        }
    }
    fn parse_value(&mut self) -> Result<Value> {
        self.skip_ws();
        match self.current() {
            Some('n') => {
                self.expect("null")?;
                Ok(Value::Null)
            }
            Some('t') => {
                self.expect("true")?;
                Ok(Value::Bool(true))
            }
            Some('f') => {
                self.expect("false")?;
                Ok(Value::Bool(false))
            }
            Some('"') => self.parse_string().map(Value::String),
            Some('[') => self.parse_array(),
            Some('{') => self.parse_object(),
            Some('-' | '0'..='9') => self
                .parse_number()
                .map(|(n, _, _)| Value::Number(Number(n))),
            Some(_) => Err(self.err("unexpected character while parsing JSON")),
            None => Err(self.err("unexpected end of input")),
        }
    }
    fn parse_array(&mut self) -> Result<Value> {
        self.bump();
        self.skip_ws();
        let mut values = Vec::new();
        if matches!(self.current(), Some(']')) {
            self.bump();
            return Ok(Value::Array(values));
        }
        loop {
            values.push(self.parse_value()?);
            self.skip_ws();
            match self.current() {
                Some(',') => {
                    self.bump();
                }
                Some(']') => {
                    self.bump();
                    return Ok(Value::Array(values));
                }
                _ => return Err(self.err("expected ',' or ']'")),
            }
        }
    }
    fn parse_object(&mut self) -> Result<Value> {
        self.bump();
        self.skip_ws();
        let mut map = Map::new();
        if matches!(self.current(), Some('}')) {
            self.bump();
            return Ok(Value::Object(map));
        }
        loop {
            if !matches!(self.current(), Some('"')) {
                return Err(self.err("expected object key"));
            }
            let key = self.parse_string()?;
            self.skip_ws();
            if !matches!(self.current(), Some(':')) {
                return Err(self.err("expected ':'"));
            }
            self.bump();
            map.insert(key, self.parse_value()?);
            self.skip_ws();
            match self.current() {
                Some(',') => {
                    self.bump();
                    self.skip_ws();
                }
                Some('}') => {
                    self.bump();
                    return Ok(Value::Object(map));
                }
                _ => return Err(self.err("expected ',' or '}'")),
            }
        }
    }
    fn parse_string(&mut self) -> Result<String> {
        if self.bump() != Some('"') {
            return Err(self.err("expected string"));
        }
        let mut out = String::new();
        loop {
            let ch = self.bump().ok_or_else(|| self.err("unterminated string"))?;
            match ch {
                '"' => return Ok(out),
                '\\' => self.parse_escape(&mut out)?,
                ch if ch <= '\u{1F}' => return Err(self.err("control character in string")),
                ch => out.push(ch),
            }
        }
    }
    fn parse_escape(&mut self, out: &mut String) -> Result<()> {
        match self.bump().ok_or_else(|| self.err("unterminated escape"))? {
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            '/' => out.push('/'),
            'b' => out.push('\u{0008}'),
            'f' => out.push('\u{000C}'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'u' => {
                let code = self.parse_hex_quad()?;
                self.push_unicode(code, out)?;
            }
            _ => return Err(self.err("invalid string escape")),
        }
        Ok(())
    }
    fn push_unicode(&mut self, code: u32, out: &mut String) -> Result<()> {
        if (0xD800..=0xDBFF).contains(&code) {
            if self.bump() != Some('\\') || self.bump() != Some('u') {
                return Err(self.err("unpaired high surrogate"));
            }
            let low = self.parse_hex_quad()?;
            if !(0xDC00..=0xDFFF).contains(&low) {
                return Err(self.err("invalid low surrogate"));
            }
            let scalar = 0x10000 + (((code - 0xD800) << 10) | (low - 0xDC00));
            out.push(char::from_u32(scalar).ok_or_else(|| self.err("invalid unicode scalar"))?);
        } else if (0xDC00..=0xDFFF).contains(&code) {
            return Err(self.err("unpaired low surrogate"));
        } else {
            out.push(char::from_u32(code).ok_or_else(|| self.err("invalid unicode scalar"))?);
        }
        Ok(())
    }
    fn parse_hex_quad(&mut self) -> Result<u32> {
        let mut value = 0;
        for _ in 0..4 {
            let ch = self
                .bump()
                .ok_or_else(|| self.err("truncated unicode escape"))?;
            value = value * 16
                + ch.to_digit(16)
                    .ok_or_else(|| self.err("invalid unicode escape"))?;
        }
        Ok(value)
    }
    fn parse_number(&mut self) -> Result<(String, bool, bool)> {
        let start = self.index;
        if matches!(self.current(), Some('-')) {
            self.bump();
        }
        match self.current() {
            Some('0') => {
                self.bump();
                if matches!(self.current(), Some('0'..='9')) {
                    return Err(self.err("leading zero in number"));
                }
            }
            Some('1'..='9') => {
                while matches!(self.current(), Some('0'..='9')) {
                    self.bump();
                }
            }
            _ => return Err(self.err("expected number digits")),
        }
        let mut frac = false;
        if matches!(self.current(), Some('.')) {
            frac = true;
            self.bump();
            if !matches!(self.current(), Some('0'..='9')) {
                return Err(self.err("expected fraction digits"));
            }
            while matches!(self.current(), Some('0'..='9')) {
                self.bump();
            }
        }
        let mut exp = false;
        if matches!(self.current(), Some('e' | 'E')) {
            exp = true;
            self.bump();
            let _ = matches!(self.current(), Some('+' | '-')).then(|| self.bump());
            if !matches!(self.current(), Some('0'..='9')) {
                return Err(self.err("expected exponent digits"));
            }
            while matches!(self.current(), Some('0'..='9')) {
                self.bump();
            }
        }
        Ok((
            normalize_number(&self.input[start..self.index], frac, exp),
            frac,
            exp,
        ))
    }
}

fn normalize_number(raw: &str, frac: bool, exp: bool) -> String {
    if !frac && !exp {
        return raw.to_string();
    }
    let (mantissa, exponent) = raw
        .find(['e', 'E'])
        .map(|pos| (&raw[..pos], Some(&raw[pos + 1..])))
        .unwrap_or((raw, None));
    let mantissa = if let Some(dot) = mantissa.find('.') {
        let integer = &mantissa[..dot];
        let fraction = mantissa[dot + 1..].trim_end_matches('0');
        if fraction.is_empty() {
            integer.to_string()
        } else {
            format!("{integer}.{fraction}")
        }
    } else {
        mantissa.to_string()
    };
    if let Some(exponent) = exponent {
        let (sign, digits) = match exponent.as_bytes().first().copied() {
            Some(b'+') => ("+", &exponent[1..]),
            Some(b'-') => ("-", &exponent[1..]),
            _ => ("+", exponent),
        };
        let digits = exponent_digits(digits);
        format!(
            "{mantissa}{}{digits}",
            if sign == "-" && digits != "0" {
                "e-"
            } else {
                "e+"
            }
        )
    } else {
        mantissa
    }
}
fn exponent_digits(digits: &str) -> &str {
    let trimmed = digits.trim_start_matches('0');
    if trimmed.is_empty() {
        "0"
    } else {
        trimmed
    }
}

fn write_value(value: &Value, pretty: bool, indent: usize) -> String {
    let mut out = String::new();
    write_into(value, &mut out, pretty, indent);
    out
}
fn write_into(value: &Value, out: &mut String, pretty: bool, indent: usize) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(v) => out.push_str(if *v { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&n.0),
        Value::String(s) => write_string(s, out),
        Value::Array(items) => {
            out.push('[');
            if !items.is_empty() {
                if pretty {
                    out.push('\n');
                }
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                        if !pretty {}
                    }
                    if pretty {
                        out.push_str(&" ".repeat(indent + 2));
                    }
                    write_into(item, out, pretty, indent + 2);
                    if pretty {
                        out.push('\n');
                    }
                }
                if pretty {
                    out.push_str(&" ".repeat(indent));
                }
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            if !map.is_empty() {
                if pretty {
                    out.push('\n');
                }
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                        if pretty {
                            out.push('\n');
                        }
                    }
                    if pretty {
                        out.push_str(&" ".repeat(indent + 2));
                    }
                    write_string(k, out);
                    out.push(':');
                    if pretty {
                        out.push(' ');
                    }
                    write_into(v, out, pretty, indent + 2);
                }
                if pretty {
                    out.push('\n');
                    out.push_str(&" ".repeat(indent));
                }
            }
            out.push('}');
        }
    }
}
fn write_string(value: &str, out: &mut String) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            ch if ch <= '\u{001F}' => out.push_str(&format!("\\u{:04X}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
}
