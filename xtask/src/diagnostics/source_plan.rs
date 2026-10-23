//! Prepare observer source and its byte bindings together, before any mutation.
use super::{dynamic, stream, templates};
use crate::{Result, artifact::snapshot::digest};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Variant {
    VerifyScratch,
    StreamCore,
    Dynamic,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Output {
    #[default]
    Plan,
    Files,
    Bindings,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    variant: Variant,
    original: BTreeMap<String, String>,
    tool: PathBuf,
    #[serde(default)]
    output: Output,
}
impl Request {
    pub(crate) fn execute(self) -> Result<Value> {
        let source = |name: &str| {
            self.original
                .get(name)
                .map(String::as_str)
                .ok_or_else(|| format!("missing diagnostic source: {name}"))
        };
        let files: BTreeMap<String, String> = match self.variant {
            Variant::Dynamic => dynamic::patch(&self.original, &self.tool)?,
            Variant::VerifyScratch | Variant::StreamCore => {
                let (module, template) = if matches!(self.variant, Variant::VerifyScratch) {
                    ("__rse_verify_scratch", "verify")
                } else {
                    ("__rse_stream_core", "stream")
                };
                let mut files = BTreeMap::from([
                    (
                        "src/lib.rs".into(),
                        format!(
                            "{}\n#[doc(hidden)]\npub mod {module};\n",
                            source("src/lib.rs")?
                        ),
                    ),
                    (
                        format!("src/{module}.rs"),
                        templates::native(template, &self.tool)?,
                    ),
                ]);
                if matches!(self.variant, Variant::VerifyScratch) {
                    files.insert(
                        "src/core/verify.rs".into(),
                        stream::verify(source("src/core/verify.rs")?)?,
                    );
                } else {
                    files.insert(
                        "src/core/stream.rs".into(),
                        stream::patch(source("src/core/stream.rs")?)?,
                    );
                    files.insert(
                        "src/core/verify.rs".into(),
                        stream::verify_par(source("src/core/verify.rs")?)?,
                    );
                }
                files
            }
        };
        if matches!(self.output, Output::Files) {
            return Ok(serde_json::to_value(files)?);
        }
        let bindings: BTreeMap<_, _> = files
            .iter()
            .map(|(name, text)| {
                (
                    name,
                    json!({"original_sha256": self.original.get(name).map(|s| digest(s.as_bytes())),
                          "diagnostic_sha256": digest(text.as_bytes())}),
                )
            })
            .collect();
        if matches!(self.output, Output::Bindings) {
            Ok(serde_json::to_value(bindings)?)
        } else {
            Ok(json!({"files":files,"bindings":bindings}))
        }
    }
}

#[cfg(test)]
mod tests;
