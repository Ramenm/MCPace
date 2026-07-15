use std::collections::BTreeMap;

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonParseError {
    message: String,
}

impl JsonParseError {
    pub fn new(message: impl Into<String>) -> Self {
        JsonParseError {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    #[cfg(test)]
    pub fn contains(&self, needle: &str) -> bool {
        self.message.contains(needle)
    }
}

impl fmt::Display for JsonParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for JsonParseError {}

impl From<JsonParseError> for String {
    fn from(error: JsonParseError) -> Self {
        error.to_string()
    }
}

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

pub fn parse_str(input: &str) -> Result<JsonValue, JsonParseError> {
    serde_json::from_str::<serde_json::Value>(input)
        .map(JsonValue::from_serde_value)
        .map_err(|error| JsonParseError::new(error.to_string()))
}

#[cfg(test)]
mod tests;
