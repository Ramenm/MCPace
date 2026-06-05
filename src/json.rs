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

    pub fn as_object_mut(&mut self) -> Option<&mut BTreeMap<String, JsonValue>> {
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
        serde_json::to_string_pretty(&self.to_serde_value())
            .expect("JsonValue only converts into serializable serde_json values")
    }

    pub fn to_compact_string(&self) -> String {
        serde_json::to_string(&self.to_serde_value())
            .expect("JsonValue only converts into serializable serde_json values")
    }

    fn from_serde_value(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => JsonValue::Null,
            serde_json::Value::Bool(value) => JsonValue::Bool(value),
            serde_json::Value::Number(value) => JsonValue::Number(value.to_string()),
            serde_json::Value::String(value) => JsonValue::String(value),
            serde_json::Value::Array(items) => {
                JsonValue::Array(items.into_iter().map(JsonValue::from_serde_value).collect())
            }
            serde_json::Value::Object(object) => JsonValue::Object(
                object
                    .into_iter()
                    .map(|(key, value)| (key, JsonValue::from_serde_value(value)))
                    .collect(),
            ),
        }
    }

    fn to_serde_value(&self) -> serde_json::Value {
        match self {
            JsonValue::Null => serde_json::Value::Null,
            JsonValue::Bool(value) => serde_json::Value::Bool(*value),
            JsonValue::Number(value) => value
                .parse::<serde_json::Number>()
                .map(serde_json::Value::Number)
                .unwrap_or_else(|_| serde_json::Value::String(value.clone())),
            JsonValue::String(value) => serde_json::Value::String(value.clone()),
            JsonValue::Array(items) => {
                serde_json::Value::Array(items.iter().map(JsonValue::to_serde_value).collect())
            }
            JsonValue::Object(map) => serde_json::Value::Object(
                map.iter()
                    .map(|(key, value)| (key.clone(), value.to_serde_value()))
                    .collect(),
            ),
        }
    }
}

pub fn parse_str(input: &str) -> Result<JsonValue, String> {
    serde_json::from_str::<serde_json::Value>(input)
        .map(JsonValue::from_serde_value)
        .map_err(|error| error.to_string())
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

    #[test]
    fn delegates_escape_and_unicode_handling_to_serde_json() {
        let value = parse_str(r#"{"text":"line\nsnowman \u2603 and emoji \uD83D\uDE80"}"#)
            .expect("parse escaped json");
        assert_eq!(
            value.get("text").and_then(JsonValue::as_str),
            Some("line\nsnowman ☃ and emoji 🚀")
        );
        assert!(value.to_compact_string().contains("emoji"));
    }

    #[test]
    fn preserves_non_ascii_keys_and_values() {
        let value = parse_str(r#"{"ключ":"значение","city":"München","arabic":"مرحبا"}"#)
            .expect("parse non-ascii json");
        assert_eq!(
            value.get("ключ").and_then(JsonValue::as_str),
            Some("значение")
        );
        assert_eq!(
            value.get("city").and_then(JsonValue::as_str),
            Some("München")
        );
        assert_eq!(
            value.get("arabic").and_then(JsonValue::as_str),
            Some("مرحبا")
        );
    }

    #[test]
    fn rejects_malformed_numbers_and_escapes() {
        for input in [
            r#"{"n": 01}"#,
            r#"{"n": 1.}"#,
            r#"{"n": 1e}"#,
            r#"{"text": "\x"}"#,
            r#"{"text": "\uD83D"}"#,
        ] {
            assert!(
                parse_str(input).is_err(),
                "accepted malformed JSON: {input}"
            );
        }
    }

    #[test]
    fn keeps_large_and_decimal_numbers_as_json_number_text() {
        let value = parse_str(r#"{"integer":9007199254740993,"decimal":-12.50,"exp":6.022e23}"#)
            .expect("parse number forms");
        assert_eq!(
            value.get("integer"),
            Some(&JsonValue::Number("9007199254740993".to_string()))
        );
        assert_eq!(
            value.get("decimal"),
            Some(&JsonValue::Number("-12.5".to_string()))
        );
        assert_eq!(
            value.get("exp"),
            Some(&JsonValue::Number("6.022e+23".to_string()))
        );
    }
}
