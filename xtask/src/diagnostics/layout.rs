//! Whole-worker layout analysis with the original diagnostic-only schemas.
mod cpu;
mod inventory;
use crate::statistics as numeric;
mod slots;
mod verify;
use crate::{Args, Result};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum Request {
    Inventory {
        request: inventory::Request,
    },
    Protocol,
    CpuPlan {
        cpus: Vec<u64>,
    },
    Cpu {
        records: Vec<Value>,
        sequence: Vec<u64>,
        iterations: u64,
        page_size: u64,
    },
    Slots {
        records: Vec<Value>,
        iterations: u64,
        page_size: u64,
    },
    Verify {
        records: Vec<Value>,
        iterations: u64,
        page_size: Option<u64>,
        measuring_cpu: Option<u64>,
    },
    Paired {
        phases: BTreeMap<String, Vec<Value>>,
        iterations: u64,
        page_size: Value,
        measuring_cpu: Value,
        #[serde(default)]
        diagnostic: bool,
    },
}
impl Request {
    pub(crate) fn execute(self) -> Result<Value> {
        match self {
            Self::Inventory { request } => request.execute(),
            Self::Protocol => Ok(verify::protocol()),
            Self::CpuPlan { cpus } => Ok(cpu::plan(cpus)),
            Self::Cpu {
                records,
                sequence,
                iterations,
                page_size,
            } => cpu::analyze(&records, &sequence, iterations, page_size),
            Self::Slots {
                records,
                iterations,
                page_size,
            } => slots::analyze(&records, iterations, page_size),
            Self::Verify {
                records,
                iterations,
                page_size,
                measuring_cpu,
            } => verify::analyze(&records, iterations, page_size, measuring_cpu),
            Self::Paired {
                phases,
                iterations,
                page_size,
                measuring_cpu,
                diagnostic,
            } => verify::paired(
                &phases,
                iterations,
                page_size
                    .as_u64()
                    .ok_or("paired analysis requires actual page size and measuring CPU")?,
                measuring_cpu
                    .as_u64()
                    .ok_or("paired analysis requires actual page size and measuring CPU")?,
                diagnostic,
            ),
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
