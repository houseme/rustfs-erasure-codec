//! One borrowed pass over original prefixed worker output.
use crate::Result;
use serde_json::Value;

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
        match crate::artifact::python_json::parse(payload) {
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
