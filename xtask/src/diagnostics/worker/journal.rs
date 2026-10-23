//! Bind numeric journals to their original bytes in the existing worker frame.
use super::Data;
use crate::{
    Result,
    artifact::{python_json::equivalent as python, snapshot},
    sampling_contract::output,
};
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Journal {
    path: PathBuf,
    sha256: String,
}
const SAMPLE: &str = "RSE_SAMPLE ";
const TIMING: &str = "RSE_TIMING ";
const FIELDS: &[&str] = &[
    "case",
    "index",
    "iterations",
    "wall_ns",
    "timed_ns",
    "fixture",
    "workload",
    "placement",
    "before",
    "after",
];
impl Journal {
    pub(super) fn worker(&self, expected: &[Value], data: &mut Data) -> Result<Vec<Value>> {
        let text = snapshot::text_member(&self.path, &self.sha256)?;
        let prefix = match data {
            Data::VerifyScratch { .. } => "RSE_VERIFY_SCRATCH ",
            Data::StreamCore { .. } => "RSE_STREAM_CORE ",
            Data::Dynamic { .. } => "RSE_DYNAMIC_BUFFERS ",
        };
        let mut counts = [0usize; 4];
        let mut rows = Vec::with_capacity(expected.len());
        let mut original_views = Vec::new();
        // Unrelated prefixes must be ignored before attempting JSON decoding.
        let prefixes = [SAMPLE, TIMING, prefix, "RSE_STREAM_VIEWS "];
        let width = if matches!(data, Data::StreamCore { .. }) {
            4
        } else {
            3
        };
        output::visit(&text, &prefixes[..width], |kind, value| {
            let index = counts[kind];
            counts[kind] += 1;
            let valid = match kind {
                0 => {
                    let valid = expected.get(index).is_some_and(|v| python(&value, v));
                    if valid {
                        rows.push(value);
                    }
                    valid
                }
                1 => expected.get(index).is_some_and(|row| {
                    FIELDS.iter().all(|key| {
                        value
                            .get(*key)
                            .zip(row.get(*key))
                            .is_some_and(|(a, b)| python(a, b))
                    })
                }),
                2 if index == 0 => {
                    let expected = match data {
                        Data::VerifyScratch { ledger } => serde_json::to_value(ledger)?,
                        Data::StreamCore { ledger, .. } => serde_json::to_value(ledger)?,
                        Data::Dynamic { ledger, .. } => serde_json::to_value(ledger)?,
                    };
                    python(&value, &expected)
                }
                3 => {
                    let Data::StreamCore { views, .. } = data else {
                        return Err("invalid stream journal".into());
                    };
                    let valid = views
                        .get(index)
                        .is_some_and(|expected| python(&value, expected));
                    if valid {
                        original_views.push(value);
                    }
                    valid
                }
                _ => false,
            };
            if !valid {
                return Err("diagnostic raw rows differ from original journals".into());
            }
            Ok(())
        })?;
        if counts[0] != expected.len() || counts[1] != 12 || counts[2] != 1 {
            return Err("incomplete original diagnostic journal".into());
        }
        if let Data::StreamCore { views, .. } = data {
            if counts[3] != views.len() {
                return Err("incomplete original stream views".into());
            }
            *views = original_views;
        }
        Ok(rows)
    }
    pub(crate) fn supplement(
        &self,
        header: &Value,
        ledger: &super::dynamic_ledger::Ledger,
    ) -> Result<()> {
        let text = snapshot::text_member(&self.path, &self.sha256)?;
        let expected = serde_json::to_value(ledger)?;
        let mut counts = [0usize; 2];
        output::visit(
            &text,
            &["RSE_DYNAMIC_SUPPLEMENT ", "RSE_DYNAMIC_BUFFERS "],
            |kind, value| {
                counts[kind] += 1;
                if counts[kind] != 1 || !python(&value, if kind == 0 { header } else { &expected })
                {
                    return Err("decoder supplement journal differs".into());
                }
                Ok(())
            },
        )?;
        if counts != [1, 1] {
            return Err("incomplete decoder supplement journal".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
