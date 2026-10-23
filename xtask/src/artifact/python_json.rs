//! JSON decoding and value equality retained by the original Python evidence contracts.
use crate::{Result, statistics};
use serde_json::{Value, json};
use std::borrow::Cow;

// Preserve the previous controller's recursive Python equality, including exact
// mixed integer/float comparisons and its bool/numeric equivalence.
pub(crate) fn equivalent(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(_), Value::Number(_)) => {
            if let (Some(a), Some(b)) = (a.as_i64(), b.as_i64()) {
                a == b
            } else if let Some(a) = a.as_i64().filter(|a| *a < 0) {
                statistics::compare(&json!(a.unsigned_abs()), &json!(-b.as_f64().unwrap())).is_eq()
            } else if let Some(b) = b.as_i64().filter(|b| *b < 0) {
                statistics::compare(&json!(-a.as_f64().unwrap()), &json!(b.unsigned_abs())).is_eq()
            } else if a.is_f64() && b.is_f64() {
                a.as_f64() == b.as_f64()
            } else {
                statistics::compare(a, b).is_eq()
            }
        }
        (Value::Bool(a), Value::Number(_)) => statistics::compare(&json!(u64::from(*a)), b).is_eq(),
        (Value::Number(_), Value::Bool(b)) => statistics::compare(a, &json!(u64::from(*b))).is_eq(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equivalent(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, a)| b.get(k).is_some_and(|b| equivalent(a, b)))
        }
        _ => a == b,
    }
}

// Python's JSON integer parser maps the complete token -0 to integer zero.
// Preserve float negative zero and string contents; never rewrite partial tokens.
fn integer_zeros(text: &str) -> Cow<'_, str> {
    if !text.contains("-0") {
        return Cow::Borrowed(text);
    }
    let bytes = text.as_bytes();
    let (mut quoted, mut escaped, mut copied) = (false, false, 0);
    let mut output: Option<String> = None;
    for (index, &byte) in bytes.iter().enumerate() {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
        } else if byte == b'"' {
            quoted = true;
        } else if byte == b'-'
            && bytes.get(index + 1) == Some(&b'0')
            && (index == 0
                || matches!(
                    bytes[index - 1],
                    b'[' | b',' | b':' | b' ' | b'\t' | b'\r' | b'\n'
                ))
            && bytes
                .get(index + 2)
                .is_none_or(|b| matches!(b, b']' | b'}' | b',' | b' ' | b'\t' | b'\r' | b'\n'))
        {
            output
                .get_or_insert_with(|| String::with_capacity(text.len()))
                .push_str(&text[copied..index]);
            copied = index + 1;
        }
    }
    if let Some(mut output) = output {
        output.push_str(&text[copied..]);
        Cow::Owned(output)
    } else {
        Cow::Borrowed(text)
    }
}

pub(crate) fn parse(text: &str) -> Result<Value> {
    Ok(serde_json::from_str(&integer_zeros(text))?)
}

#[cfg(test)]
mod integer_zero_tests {
    use super::*;
    #[test]
    fn integer_negative_zero_preserves_python_types_without_rewriting_floats_or_strings() {
        let text = r#"{"integer":-0,"float":-0.0,"exponent":-0e0,"text":"-0","escaped":"\\\"-0"}"#;
        let value: Value = serde_json::from_str(&integer_zeros(text)).unwrap();
        assert_eq!(value["integer"].as_u64(), Some(0));
        assert!(value["float"].is_f64());
        assert!(value["float"].as_f64().unwrap().is_sign_negative());
        assert!(value["exponent"].is_f64());
        assert!(value["exponent"].as_f64().unwrap().is_sign_negative());
        assert_eq!(value["text"], "-0");
        let original: Value = serde_json::from_str(text).unwrap();
        assert_eq!(value["escaped"], original["escaped"]);
        for invalid in ["[--0]", "[-00]", "[-0true]", "[-0-0]", "{\"k\":--0}"] {
            assert!(serde_json::from_str::<Value>(&integer_zeros(invalid)).is_err());
        }
    }
}

#[cfg(test)]
mod decode_tests {
    use super::*;
    #[test]
    fn original_objects_keep_last_keys_integer_zero_and_finite_float_types() {
        let value = parse(r#"{"index":9,"index":-0,"float":-0.0}"#).unwrap();
        assert_eq!(value["index"].as_u64(), Some(0));
        assert!(value["float"].is_f64());
        assert!(parse("{\"n\":NaN}").is_err());
        assert!(parse("{\"n\":Infinity}").is_err());
    }
}
