//! Recursive value equality retained by the original Python diagnostic contracts.
use crate::statistics;
use serde_json::{Value, json};

// Preserve the previous controller's recursive Python equality, including exact
// mixed integer/float comparisons and its bool/numeric equivalence.
pub(crate) fn python(a: &Value, b: &Value) -> bool {
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
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| python(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, a)| b.get(k).is_some_and(|b| python(a, b)))
        }
        _ => a == b,
    }
}
