//! Whole-record VM diagnosis over shared typed snapshots, without device I/O.
mod clock;
mod process;
use crate::{Args, Result};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum Request {
    Contract,
    Basic {
        records: Vec<Value>,
        #[serde(default)]
        native_service_clock: bool,
    },
    Prepare {
        records: Vec<Value>,
        profile: String,
        #[serde(default)]
        native_service_clock: bool,
    },
    Analyze {
        records: Vec<Value>,
        profile: String,
        #[serde(default)]
        native_service_clock: bool,
    },
    Point {
        point: Value,
    },
    Window {
        record: Value,
    },
    Frequency {
        before: Value,
        after: Value,
        cpus: Vec<u64>,
        start: u64,
        end: u64,
    },
    Probe {
        text: String,
        cpu: u64,
    },
}
impl Request {
    pub(crate) fn execute(self) -> Result<Value> {
        match self {
            Self::Contract => Ok(clock::contract()),
            Self::Basic {
                records,
                native_service_clock,
            } => {
                process::validate(&records, native_service_clock)?;
                process::strip(records)
            }
            Self::Prepare {
                records,
                profile,
                native_service_clock,
            } => {
                let analysis = process::analyze(&records, &profile, native_service_clock, true)?;
                Ok(Value::Object(serde_json::Map::from_iter([
                    ("basic".into(), process::strip(records)?),
                    ("analysis".into(), analysis),
                ])))
            }
            Self::Analyze {
                records,
                profile,
                native_service_clock,
            } => process::analyze(&records, &profile, native_service_clock, false),
            Self::Point { point } => {
                clock::point(&point)?;
                Ok(Value::Null)
            }
            Self::Window { record } => clock::window(&record, clock::pair(&record, false)?),
            Self::Frequency {
                before,
                after,
                cpus,
                start,
                end,
            } => clock::frequency(&before, &after, &cpus, start, end),
            Self::Probe { text, cpu } => {
                let mut lines = text
                    .split_inclusive('\n')
                    .filter_map(|line| line.strip_prefix("RSE_NATIVE_CLOCK_PROBE "));
                let line = lines
                    .next()
                    .filter(|s| s.ends_with('\n'))
                    .ok_or("one complete zero-codec clock probe is required")?;
                if lines.next().is_some() {
                    return Err("one complete zero-codec clock probe is required".into());
                }
                let crate::artifact::StrictJson(raw) = serde_json::from_str(line)?;
                if !clock::keys(&raw, &["wall_ns", "before", "after"]) {
                    return Err("invalid zero-codec clock probe".into());
                }
                let record = json!({"wall_ns":raw["wall_ns"],"placement":{"cpu":cpu},"before":{"process":raw["before"]},"after":{"process":raw["after"]}});
                let summary = clock::window(&record, clock::pair(&record, false)?)?;
                if summary["interpretation_qualified"] != true {
                    return Err(
                        "zero-codec native service-clock capability was not established".into(),
                    );
                }
                Ok(summary)
            }
        }
    }
}
pub(crate) fn run(args: &Args) -> Result<()> {
    let request: Request =
        crate::artifact::read_json(std::path::Path::new(args.required("--input")?))?;
    println!("{}", serde_json::to_string(&request.execute()?)?);
    Ok(())
}
#[cfg(test)]
mod tests;
