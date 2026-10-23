//! Private JSON bridge for legacy controllers; never invokes Python.
use crate::{
    Args, Result, artifact,
    diagnostics::{dynamic, overlays, sampler, stream, templates},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum Request {
    BaselineReplay {
        request: crate::diagnostics::baseline::replay::Request,
    },
    RuntimeValidate {
        value: serde_json::Value,
    },
    VmAnalysis {
        request: crate::diagnostics::vm::Request,
    },
    BaselineAnalysis {
        request: crate::diagnostics::baseline::Request,
    },
    DiagnosticLayout {
        request: crate::diagnostics::layout::Request,
    },
    DiagnosticWorker {
        request: crate::diagnostics::worker::Request,
    },
    ReleaseRows {
        request: crate::release_rows::Request,
    },
    DiagnosticLedger {
        request: crate::diagnostics::ledger::Request,
    },
    StreamViews {
        request: crate::diagnostics::views::Request,
    },
    Sampling {
        request: crate::sampling_contract::Request,
    },
    CompatibleRows {
        profile: String,
        left: BTreeMap<String, serde_json::Value>,
        right: BTreeMap<String, serde_json::Value>,
    },
    ObservationContracts {
        profile: String,
        records: Vec<serde_json::Value>,
        reference: Option<BTreeMap<String, (serde_json::Value, serde_json::Value)>>,
        #[serde(default)]
        collect: bool,
    },
    CpuIdentity {
        text: String,
        machine: String,
        reserved_cpus: Vec<u64>,
    },
    BenchmarkRows {
        rows: serde_json::Value,
        source: String,
        #[serde(default)]
        strict_provenance: bool,
    },
    HotspotRows {
        rows: serde_json::Value,
        metric: String,
    },
    BenchmarkProvenance {
        row: serde_json::Value,
    },
    ProvenanceField {
        row: serde_json::Map<String, serde_json::Value>,
        field: String,
    },
    MemoryPolicyCapture,
    MemoryPolicyValidate {
        value: serde_json::Value,
        base_page_size: Option<u64>,
    },
    DiagnosticRender {
        root: PathBuf,
        tool: PathBuf,
        variant: String,
        #[serde(default)]
        allocation_check: bool,
    },
    SourcePlan {
        request: crate::diagnostics::source_plan::Request,
    },
    MatrixSource {
        text: String,
    },
    DecodeSource {
        text: String,
    },
    SamplerRender {
        root: PathBuf,
        tool: PathBuf,
    },
    ExcludeSetup {
        text: String,
        name: String,
        count: usize,
    },
    ReserveOutputs {
        text: String,
    },
    Template {
        variant: String,
        tool: PathBuf,
    },
    StreamSource {
        text: String,
    },
    VerifySource {
        text: String,
    },
}
#[derive(Serialize)]
#[serde(untagged)]
enum Response {
    Data(serde_json::Value),
    Text(String),
    Files(BTreeMap<String, String>),
}
impl Request {
    fn execute(self) -> Result<Response> {
        Ok(match self {
            Self::BaselineReplay { request } => Response::Data(request.execute()?),
            Self::RuntimeValidate { value } => {
                crate::runtime::validate_runtime(&value)?;
                Response::Data(serde_json::Value::Null)
            }
            Self::VmAnalysis { request } => Response::Data(request.execute()?),
            Self::BaselineAnalysis { request } => Response::Data(request.execute()?),
            Self::DiagnosticLayout { request } => Response::Data(request.execute()?),
            Self::DiagnosticWorker { request } => Response::Data(request.execute()?),
            Self::ReleaseRows { request } => Response::Data(request.records()?),
            Self::DiagnosticLedger { request } => Response::Data(request.execute()?),
            Self::StreamViews { request } => Response::Data(request.execute()?),
            Self::Sampling { request } => Response::Data(request.execute()?),
            Self::CompatibleRows {
                profile,
                left,
                right,
            } => {
                crate::benchmark_contract::compatible(&profile, &left, &right)?;
                Response::Data(serde_json::Value::Null)
            }
            Self::ObservationContracts {
                profile,
                records,
                reference,
                collect,
            } => Response::Data(crate::benchmark_contract::observations(
                &profile, records, reference, collect,
            )?),
            Self::CpuIdentity {
                text,
                machine,
                reserved_cpus,
            } => Response::Text(crate::runtime::cpu_identity(
                &text,
                &machine,
                &reserved_cpus,
            )?),
            Self::BenchmarkRows {
                rows,
                source,
                strict_provenance,
            } => Response::Data(crate::benchmark_contract::library_rows(
                rows,
                &source,
                strict_provenance,
            )?),
            Self::HotspotRows { rows, metric } => {
                Response::Data(crate::benchmark_contract::library_hotspots(rows, &metric)?)
            }
            Self::BenchmarkProvenance { row } => {
                Response::Data(crate::benchmark_contract::provenance_outcomes(row)?)
            }
            Self::ProvenanceField { row, field } => Response::Data(
                crate::benchmark_contract::provenance_value(&row, &field)?
                    .map_or(serde_json::Value::Null, serde_json::Value::String),
            ),
            Self::MemoryPolicyCapture => {
                Response::Data(serde_json::to_value(crate::memory_policy::capture()?)?)
            }
            Self::MemoryPolicyValidate {
                value,
                base_page_size,
            } => {
                crate::memory_policy::parse(value, base_page_size)?;
                Response::Data(serde_json::Value::Null)
            }
            Self::DiagnosticRender {
                root,
                tool,
                variant,
                allocation_check,
            } => Response::Files(overlays::render(&root, &tool, &variant, allocation_check)?),
            Self::SourcePlan { request } => Response::Data(request.execute()?),
            Self::MatrixSource { text } => Response::Text(dynamic::matrix(&text)?),
            Self::DecodeSource { text } => Response::Text(dynamic::decode(&text)?),
            Self::SamplerRender { root, tool } => Response::Files(sampler::render(&root, &tool)?),
            Self::ExcludeSetup { text, name, count } => {
                Response::Text(sampler::exclude_setup(&text, &name, count)?)
            }
            Self::ReserveOutputs { text } => Response::Text(sampler::reserve_outputs(&text)?),
            Self::Template { variant, tool } => Response::Text(templates::native(&variant, &tool)?),
            Self::StreamSource { text } => Response::Text(stream::patch(&text)?),
            Self::VerifySource { text } => Response::Text(stream::verify(&text)?),
        })
    }
}

// A complete 32 MiB artifact plus the small typed request envelope/newline.
const REQUEST_LIMIT: usize = 32 * 1024 * 1024 + 64 * 1024;
const RESPONSE_LIMIT: usize = 32 * 1024 * 1024;

fn decode(bytes: &[u8]) -> Result<Request> {
    let artifact::StrictJson(value) = serde_json::from_slice(bytes)?;
    Ok(serde_json::from_value(value)?)
}

#[derive(Serialize)]
#[serde(untagged)]
enum Reply {
    Success { ok: bool, value: Response },
    Failure { ok: bool, error: String },
}

fn serve(
    reader: &mut impl std::io::BufRead,
    writer: &mut impl std::io::Write,
    request_limit: usize,
    response_limit: usize,
) -> Result<()> {
    use std::io::{BufRead, Read};
    loop {
        let mut bytes = Vec::new();
        let read = reader
            .by_ref()
            .take(request_limit as u64 + 1)
            .read_until(b'\n', &mut bytes)?;
        if read == 0 {
            return Ok(());
        }
        if read > request_limit {
            return Err("native request exceeds artifact transport byte budget".into());
        }
        let result = decode(&bytes).and_then(Request::execute);
        let reply = match result {
            Ok(value) => Reply::Success { ok: true, value },
            Err(error) => Reply::Failure {
                ok: false,
                error: error.to_string(),
            },
        };
        let mut output = serde_json::to_vec(&reply)?;
        if output.len() > response_limit {
            output = serde_json::to_vec(&Reply::Failure {
                ok: false,
                error: "native response exceeds 32 MiB".into(),
            })?;
        }
        writer.write_all(&output)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
    }
}

pub(crate) fn run(args: &Args) -> Result<()> {
    use std::io::{Read, Write};
    if args.0.contains_key("--serve") {
        if args.0.contains_key("--input") {
            return Err("--serve cannot be combined with --input".into());
        }
        return serve(
            &mut std::io::stdin().lock(),
            &mut std::io::BufWriter::new(std::io::stdout().lock()),
            REQUEST_LIMIT,
            RESPONSE_LIMIT,
        );
    }
    let path = args.get("--input", "-");
    let request: Request = if path == "-" {
        let mut bytes = Vec::new();
        std::io::stdin()
            .take(REQUEST_LIMIT as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > REQUEST_LIMIT {
            return Err("native source request exceeds artifact transport byte budget".into());
        }
        decode(&bytes)?
    } else {
        artifact::read_json(std::path::Path::new(path))?
    };
    let result = request.execute()?;
    let mut writer = std::io::BufWriter::new(std::io::stdout().lock());
    serde_json::to_writer(&mut writer, &result)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    #[test]
    fn worker_keeps_frames_separate_and_recovers_after_invalid_requests() {
        let requests = [
            json!({"kind":"provenance-field","row":{"backend":"scalar"},"field":"backend"})
                .to_string(),
            r#"{"kind":"provenance-field","row":{},"field":"backend","field":"compiler"}"#.into(),
            r#"{"kind":"not-a-request"}"#.into(),
            json!({"kind":"provenance-field","row":{"backend":"rust-neon"},"field":"backend"})
                .to_string(),
        ]
        .join("\n")
            + "\n";
        let mut output = Vec::new();
        serve(
            &mut std::io::Cursor::new(requests),
            &mut output,
            REQUEST_LIMIT,
            RESPONSE_LIMIT,
        )
        .unwrap();
        let replies: Vec<Value> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(replies.len(), 4);
        assert_eq!(replies[0], json!({"ok":true,"value":"scalar"}));
        assert_eq!(replies[1]["ok"], false);
        assert_eq!(replies[2]["ok"], false);
        assert_eq!(replies[3], json!({"ok":true,"value":"rust-neon"}));
    }
    #[test]
    fn worker_input_bounds_are_inclusive_and_oversized_output_is_not_returned() {
        let mut output = Vec::new();
        assert!(
            serve(
                &mut std::io::Cursor::new(b"0\n"),
                &mut output,
                1,
                RESPONSE_LIMIT
            )
            .is_err()
        );
        assert!(output.is_empty());
        serve(&mut std::io::Cursor::new(b"0\n"), &mut output, 2, 1).unwrap();
        let reply: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(reply["ok"], false);
        assert!(
            reply["error"]
                .as_str()
                .unwrap()
                .contains("response exceeds")
        );
    }
    #[test]
    fn strict_json_retains_float_bits_across_the_native_bridge() {
        let raw=br#"{"kind":"benchmark-rows","source":"test","rows":[{"operation":"encode","data_shards":4,"parity_shards":2,"shard_size":1024,"throughput_mb_s":2866592468.8449306,"ns_per_iter":8.344125545873929e-19}]}"#;
        let value = decode(raw).unwrap().execute().unwrap();
        let result = serde_json::to_value(value).unwrap();
        assert_eq!(
            result[0]["throughput_mb_s"].as_f64().unwrap().to_bits(),
            2866592468.8449306f64.to_bits()
        );
        assert_eq!(
            result[0]["ns_per_iter"].as_f64().unwrap().to_bits(),
            8.344125545873929e-19f64.to_bits()
        );
    }
}
