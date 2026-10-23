//! One borrowed pass over original prefixed worker output.
use crate::Result;
use serde_json::Value;
use std::borrow::Cow;

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

pub(crate) fn visit(
    text: &str,
    prefixes: &[&str],
    mut record: impl FnMut(usize, Value) -> Result<()>,
) -> Result<()> {
    let mut remaining = text;
    while !remaining.is_empty() {
        let end = remaining
            .char_indices()
            .find_map(|(i, c)| {
                matches!(
                    c,
                    '\n' | '\r'
                        | '\u{b}'
                        | '\u{c}'
                        | '\u{1c}'
                        | '\u{1d}'
                        | '\u{1e}'
                        | '\u{85}'
                        | '\u{2028}'
                        | '\u{2029}'
                )
                .then(|| {
                    i + c.len_utf8()
                        + usize::from(c == '\r' && remaining[i + 1..].starts_with('\n'))
                })
            })
            .unwrap_or(remaining.len());
        let (line, tail) = remaining.split_at(end);
        remaining = tail;
        let Some((index, payload)) = prefixes
            .iter()
            .enumerate()
            .find_map(|(i, prefix)| line.strip_prefix(prefix).map(|s| (i, s)))
        else {
            continue;
        };
        match serde_json::from_str(&integer_zeros(payload)) {
            Ok(value) => record(index, value)?,
            Err(_) if tail.is_empty() && !line.ends_with('\n') => {}
            Err(e) => return Err(format!("raw original worker journal differs: {e}").into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn parse(text: &str) -> Result<Vec<(usize, Value)>> {
        let mut records = Vec::new();
        visit(text, &["RSE_SAMPLE ", "RSE_TIMING "], |kind, value| {
            records.push((kind, value));
            Ok(())
        })?;
        Ok(records)
    }
    #[test]
    fn interleaved_prefixes_duplicate_keys_and_completed_final_rows_are_retained() {
        for newline in ["\n", "\r\n", "\r"] {
            let text = format!(
                "ignored{newline}RSE_TIMING {{\"index\":9,\"index\":0}}{newline}RSE_SAMPLE {{\"n\":18446744073709551615}}{newline}RSE_SAMPLE {{"
            );
            assert_eq!(
                parse(&text).unwrap(),
                vec![(1, json!({"index":0})), (0, json!({"n":u64::MAX}))]
            );
        }
        assert_eq!(parse("RSE_SAMPLE [1,2]").unwrap(), vec![(0, json!([1, 2]))]);
    }
    #[test]
    fn only_an_interrupted_final_fragment_is_ignored() {
        assert!(parse("RSE_SAMPLE {\n").is_err());
        assert!(parse("RSE_SAMPLE {\nignored").is_err());
        for separator in [
            '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}', '\u{2029}',
        ] {
            assert!(parse(&format!("RSE_SAMPLE {{}}{separator}ignored")).is_err());
            assert!(
                parse(&format!("RSE_SAMPLE {{}}{separator}"))
                    .unwrap()
                    .is_empty()
            );
        }
    }
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
