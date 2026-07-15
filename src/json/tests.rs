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
