use crate::Result;
use serde_json::{Value, json};
use std::cmp::Ordering;

pub(crate) fn uint(value: &Value) -> Result<u64> {
    value
        .as_u64()
        .ok_or_else(|| "invalid unsigned layout counter".into())
}
pub(crate) fn positive(value: &Value) -> Result<f64> {
    value
        .as_f64()
        .filter(|n| n.is_finite() && *n > 0.0)
        .ok_or_else(|| "invalid positive layout interval".into())
}
fn mixed(integer: u64, float: f64) -> Ordering {
    if float >= 18446744073709551616.0 {
        return Ordering::Less;
    }
    if float < 0.0 {
        return Ordering::Greater;
    }
    integer.cmp(&(float as u64)).then_with(|| {
        if float.fract() > 0.0 {
            Ordering::Less
        } else {
            Ordering::Equal
        }
    })
}
pub(crate) fn compare(a: &Value, b: &Value) -> Ordering {
    match (a.as_u64(), b.as_u64()) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(a), None) => mixed(a, b.as_f64().unwrap_or(f64::INFINITY)),
        (None, Some(b)) => mixed(b, a.as_f64().unwrap_or(f64::INFINITY)).reverse(),
        _ => a
            .as_f64()
            .unwrap_or(f64::INFINITY)
            .total_cmp(&b.as_f64().unwrap_or(f64::INFINITY)),
    }
}
// Match exact integer true division, avoiding rounding the operands first.
fn integer_ratio(n: u64, d: u64) -> f64 {
    if n == 0 {
        return 0.0;
    }
    let (n, d) = (u128::from(n), u128::from(d));
    let mut exponent = (128 - n.leading_zeros()) as i32 - (128 - d.leading_zeros()) as i32;
    if if exponent >= 0 {
        n < d << exponent
    } else {
        n << -exponent < d
    } {
        exponent -= 1;
    }
    let shift = 52 - exponent;
    let (n, d) = if shift >= 0 {
        (n << shift, d)
    } else {
        (n, d << -shift)
    };
    let (mut q, r) = (n / d, n % d);
    if r * 2 > d || r * 2 == d && q % 2 != 0 {
        q += 1;
    }
    q as f64 * 2.0_f64.powi(exponent - 52)
}
pub(crate) fn ratio(a: &Value, b: &Value) -> Result<f64> {
    positive(b)?;
    let value = match (a.as_u64(), b.as_u64()) {
        (Some(a), Some(b)) => integer_ratio(a, b),
        _ => a.as_f64().ok_or("invalid layout numerator")? / positive(b)?,
    };
    value
        .is_finite()
        .then_some(value)
        .ok_or_else(|| "nonfinite measurement ratio".into())
}
pub(crate) fn difference(a: &Value, b: &Value) -> Value {
    match (a.as_u64(), b.as_u64()) {
        (Some(a), Some(b)) => json!(a.abs_diff(b)),
        _ => json!((a.as_f64().unwrap_or(0.0) - b.as_f64().unwrap_or(0.0)).abs()),
    }
}
pub(crate) fn close_product(metric: &Value, count: u64, total: &Value) -> Result<bool> {
    let a = positive(metric)?;
    let b = positive(total)?;
    let product = metric.as_u64().map_or(a * count as f64, |v| {
        (u128::from(v) * u128::from(count)) as f64
    });
    if !product.is_finite() {
        return Ok(false);
    }
    Ok((product - b).abs() <= 1.0_f64.max(1e-12 * product.abs()).max(1e-12 * b.abs()))
}
/// Stable, exact ordering of the fixed nine observations; keep scalar types.
pub(crate) fn ordered<'a>(
    mut values: impl Iterator<Item = &'a Value>,
    allow_zero: bool,
) -> Result<[&'a Value; 9]> {
    let mut result = [&Value::Null; 9];
    for slot in &mut result {
        *slot = values.next().ok_or("all nine measurements are required")?;
        if !slot
            .as_f64()
            .is_some_and(|n| n.is_finite() && if allow_zero { n >= 0.0 } else { n > 0.0 })
        {
            return Err("invalid measurement distribution".into());
        }
    }
    if values.next().is_some() {
        return Err("all nine measurements are required".into());
    }
    result.sort_by(|a, b| compare(a, b));
    Ok(result)
}
pub(crate) fn center<'a>(values: impl Iterator<Item = &'a Value>) -> Result<(Value, f64)> {
    let values = ordered(values, false)?;
    let median = values[4].clone();
    let spread = ratio(&difference(values[7], values[1]), &median)?;
    Ok((median, spread))
}
/// Python preserves integer subtraction until one operand is floating point.
pub(crate) fn subtract(a: &Value, b: &Value) -> Result<Value> {
    if let (Some(a), Some(b)) = (a.as_u64(), b.as_u64()) {
        let difference = i128::from(a) - i128::from(b);
        Ok(if difference >= 0 {
            json!(difference as u64)
        } else {
            json!(i64::try_from(difference).map_err(|_| "signed measurement difference overflow")?)
        })
    } else {
        let value = a.as_f64().ok_or("invalid measurement difference")?
            - b.as_f64().ok_or("invalid measurement difference")?;
        if !value.is_finite() {
            return Err("nonfinite measurement difference".into());
        }
        Ok(json!(value))
    }
}
