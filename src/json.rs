use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<JsonValue>),
    Object(BTreeMap<String, JsonValue>),
}

impl JsonValue {
    pub fn object<K, I>(entries: I) -> Self
    where
        K: Into<String>,
        I: IntoIterator<Item = (K, JsonValue)>,
    {
        let mut map = BTreeMap::new();
        for (key, value) in entries {
            map.insert(key.into(), value);
        }
        JsonValue::Object(map)
    }

    pub fn array<I>(items: I) -> Self
    where
        I: IntoIterator<Item = JsonValue>,
    {
        JsonValue::Array(items.into_iter().collect())
    }

    pub fn string<T: Into<String>>(value: T) -> Self {
        JsonValue::String(value.into())
    }

    pub fn bool(value: bool) -> Self {
        JsonValue::Bool(value)
    }

    pub fn number<T: ToString>(value: T) -> Self {
        JsonValue::Number(value.to_string())
    }

    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        match self {
            JsonValue::Object(map) => map.get(key),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&BTreeMap<String, JsonValue>> {
        match self {
            JsonValue::Object(map) => Some(map),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[JsonValue]> {
        match self {
            JsonValue::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            JsonValue::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            JsonValue::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            JsonValue::Number(value) => value.parse::<i64>().ok(),
            _ => None,
        }
    }

    pub fn to_pretty_string(&self) -> String {
        let mut output = String::new();
        self.write_pretty(&mut output, 0);
        output
    }

    pub fn to_compact_string(&self) -> String {
        let mut output = String::new();
        self.write_compact(&mut output);
        output
    }

    fn write_pretty(&self, output: &mut String, indent: usize) {
        match self {
            JsonValue::Null => output.push_str("null"),
            JsonValue::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
            JsonValue::Number(value) => output.push_str(value),
            JsonValue::String(value) => write_json_string(output, value),
            JsonValue::Array(items) => {
                if items.is_empty() {
                    output.push_str("[]");
                    return;
                }
                output.push('[');
                output.push('\n');
                for (index, item) in items.iter().enumerate() {
                    write_indent(output, indent + 2);
                    item.write_pretty(output, indent + 2);
                    if index + 1 < items.len() {
                        output.push(',');
                    }
                    output.push('\n');
                }
                write_indent(output, indent);
                output.push(']');
            }
            JsonValue::Object(map) => {
                if map.is_empty() {
                    output.push_str("{}");
                    return;
                }
                output.push('{');
                output.push('\n');
                let len = map.len();
                for (index, (key, value)) in map.iter().enumerate() {
                    write_indent(output, indent + 2);
                    write_json_string(output, key);
                    output.push_str(": ");
                    value.write_pretty(output, indent + 2);
                    if index + 1 < len {
                        output.push(',');
                    }
                    output.push('\n');
                }
                write_indent(output, indent);
                output.push('}');
            }
        }
    }

    fn write_compact(&self, output: &mut String) {
        match self {
            JsonValue::Null => output.push_str("null"),
            JsonValue::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
            JsonValue::Number(value) => output.push_str(value),
            JsonValue::String(value) => write_json_string(output, value),
            JsonValue::Array(items) => {
                output.push('[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    item.write_compact(output);
                }
                output.push(']');
            }
            JsonValue::Object(map) => {
                output.push('{');
                for (index, (key, value)) in map.iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    write_json_string(output, key);
                    output.push(':');
                    value.write_compact(output);
                }
                output.push('}');
            }
        }
    }
}

pub fn parse_str(input: &str) -> Result<JsonValue, String> {
    let mut parser = Parser {
        input: input.as_bytes(),
        index: 0,
    };
    let value = parser.parse_value()?;
    parser.skip_whitespace();
    if parser.index != parser.input.len() {
        return Err(parser.error("unexpected trailing content"));
    }
    Ok(value)
}

struct Parser<'a> {
    input: &'a [u8],
    index: usize,
}

impl<'a> Parser<'a> {
    fn parse_value(&mut self) -> Result<JsonValue, String> {
        self.skip_whitespace();
        let Some(byte) = self.peek() else {
            return Err(self.error("unexpected end of input"));
        };

        match byte {
            b'{' => self.parse_object(),
            b'[' => self.parse_array(),
            b'"' => self.parse_string().map(JsonValue::String),
            b't' => {
                self.expect_literal("true")?;
                Ok(JsonValue::Bool(true))
            }
            b'f' => {
                self.expect_literal("false")?;
                Ok(JsonValue::Bool(false))
            }
            b'n' => {
                self.expect_literal("null")?;
                Ok(JsonValue::Null)
            }
            b'-' | b'0'..=b'9' => self.parse_number().map(JsonValue::Number),
            _ => Err(self.error("unexpected character")),
        }
    }

    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.consume(b'{')?;
        self.skip_whitespace();
        let mut map = BTreeMap::new();
        if self.peek() == Some(b'}') {
            self.index += 1;
            return Ok(JsonValue::Object(map));
        }

        loop {
            self.skip_whitespace();
            let key = self.parse_string()?;
            self.skip_whitespace();
            self.consume(b':')?;
            let value = self.parse_value()?;
            map.insert(key, value);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => {
                    self.index += 1;
                }
                Some(b'}') => {
                    self.index += 1;
                    break;
                }
                _ => return Err(self.error("expected ',' or '}' in object")),
            }
        }

        Ok(JsonValue::Object(map))
    }

    fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.consume(b'[')?;
        self.skip_whitespace();
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.index += 1;
            return Ok(JsonValue::Array(items));
        }

        loop {
            let value = self.parse_value()?;
            items.push(value);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => {
                    self.index += 1;
                }
                Some(b']') => {
                    self.index += 1;
                    break;
                }
                _ => return Err(self.error("expected ',' or ']' in array")),
            }
        }

        Ok(JsonValue::Array(items))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.consume(b'"')?;
        let mut output = String::new();

        while let Some(byte) = self.peek() {
            self.index += 1;
            match byte {
                b'"' => return Ok(output),
                b'\\' => {
                    let Some(escape) = self.peek() else {
                        return Err(self.error("unterminated escape sequence"));
                    };
                    self.index += 1;
                    match escape {
                        b'"' => output.push('"'),
                        b'\\' => output.push('\\'),
                        b'/' => output.push('/'),
                        b'b' => output.push('\u{0008}'),
                        b'f' => output.push('\u{000C}'),
                        b'n' => output.push('\n'),
                        b'r' => output.push('\r'),
                        b't' => output.push('\t'),
                        b'u' => output.push(self.parse_unicode_escape()?),
                        _ => return Err(self.error("unsupported escape sequence")),
                    }
                }
                0x00..=0x1F => return Err(self.error("control character in string")),
                _ => {
                    if byte.is_ascii() {
                        output.push(byte as char);
                    } else {
                        let remaining = std::str::from_utf8(&self.input[self.index - 1..])
                            .map_err(|_| self.error("invalid utf-8 in string"))?;
                        let ch = remaining
                            .chars()
                            .next()
                            .ok_or_else(|| self.error("invalid utf-8 in string"))?;
                        output.push(ch);
                        self.index += ch.len_utf8() - 1;
                    }
                }
            }
        }

        Err(self.error("unterminated string"))
    }

    fn parse_unicode_escape(&mut self) -> Result<char, String> {
        let start = self.index;
        let end = start + 4;
        if end > self.input.len() {
            return Err(self.error("incomplete unicode escape"));
        }
        let raw = std::str::from_utf8(&self.input[start..end])
            .map_err(|_| self.error("invalid unicode escape"))?;
        self.index = end;
        let value =
            u16::from_str_radix(raw, 16).map_err(|_| self.error("invalid unicode escape"))? as u32;
        char::from_u32(value).ok_or_else(|| self.error("invalid unicode codepoint"))
    }

    fn parse_number(&mut self) -> Result<String, String> {
        let start = self.index;
        if self.peek() == Some(b'-') {
            self.index += 1;
        }
        self.parse_digits()?;
        if self.peek() == Some(b'.') {
            self.index += 1;
            self.parse_digits()?;
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.index += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.index += 1;
            }
            self.parse_digits()?;
        }
        let raw = std::str::from_utf8(&self.input[start..self.index])
            .map_err(|_| self.error("invalid number"))?;
        Ok(raw.to_string())
    }

    fn parse_digits(&mut self) -> Result<(), String> {
        let start = self.index;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.index += 1;
        }
        if self.index == start {
            return Err(self.error("expected digits"));
        }
        Ok(())
    }

    fn consume(&mut self, expected: u8) -> Result<(), String> {
        match self.peek() {
            Some(byte) if byte == expected => {
                self.index += 1;
                Ok(())
            }
            _ => Err(self.error(&format!("expected '{}'", expected as char))),
        }
    }

    fn expect_literal(&mut self, literal: &str) -> Result<(), String> {
        let bytes = literal.as_bytes();
        if self.input.get(self.index..self.index + bytes.len()) == Some(bytes) {
            self.index += bytes.len();
            Ok(())
        } else {
            Err(self.error(&format!("expected {}", literal)))
        }
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.index += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.index).copied()
    }

    fn error(&self, message: &str) -> String {
        format!("{} at byte {}", message, self.index)
    }
}

fn write_indent(output: &mut String, indent: usize) {
    for _ in 0..indent {
        output.push(' ');
    }
}

fn write_json_string(output: &mut String, value: &str) {
    output.push('"');
    for ch in value.chars() {
        match ch {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0C}' => output.push_str("\\f"),
            ch if ch < '\u{20}' => output.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => output.push(ch),
        }
    }
    output.push('"');
}

#[cfg(test)]
mod tests {
    use super::{parse_str, JsonValue};

    #[test]
    fn parses_and_round_trips_basic_json() {
        let value = parse_str(
            r#"{
  "name": "mcpace", "ok": true, "items": [1, "two", null]
}"#,
        )
        .expect("parse basic json");
        let pretty = value.to_pretty_string();
        assert!(pretty.contains("\"name\": \"mcpace\""));
        assert!(matches!(value.get("ok"), Some(JsonValue::Bool(true))));
    }
}
