//! Source-bound workload and retained-buffer contracts, shared with the renderer.
use crate::{Args, Result, artifact, diagnostics::catalog};
use serde::Deserialize;
use std::borrow::Borrow;

pub(crate) mod batch;
pub(crate) mod calibration_profile;
pub(crate) mod preparation;
mod protocol;
use serde_json::{Value, json};

#[path = "../../scripts/release-sampling/service_window.rs"]
mod service_window;
pub(crate) const CENTRAL_SPREAD_LIMIT: f64 = 0.10;
pub(crate) const REPEAT_DRIFT_LIMIT: f64 = 0.05;
pub(crate) const MINIMUM_WALL_NS: u64 = service_window::MINIMUM_WALL_NS;
pub(crate) const MINIMUM_TIMED_NS: u64 = service_window::MINIMUM_TIMED_NS;
pub(crate) fn protocol_value() -> Value {
    protocol::value()
}

const _: () = {
    use service_window::*;
    assert!(
        0 < MINIMUM_TIMED_NS
            && MINIMUM_TIMED_NS <= TARGET_TIMED_NS
            && TARGET_TIMED_NS <= TARGET_WALL_NS
    );
    assert!(0 < MINIMUM_WALL_NS && MINIMUM_WALL_NS <= TARGET_WALL_NS);
};

const BASE_SEED: u64 = 0x00EC_5EED_2026_0524;
const COUNTERS: &[&str] = &[
    "cpu",
    "run_ns",
    "wait_ns",
    "switches",
    "process_minor_faults",
    "process_major_faults",
];

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum Request {
    Catalog,
    Batch {
        request: batch::Request,
    },
    CalibrationProfile {
        profile: String,
        revision: String,
        records: Vec<Value>,
        expected_page_size: Option<u64>,
        allowed_cpus: Option<Vec<u64>>,
    },
    WorkloadContract {
        profile: String,
        case: String,
    },
    FixtureContract {
        profile: String,
        case: String,
    },
    ValidateWorkload {
        profile: String,
        record: Value,
    },
    ValidateWorkloadSequence {
        profile: String,
        records: Vec<Value>,
    },
    ValidatePlacement {
        record: Value,
        allowed_cpus: Option<Vec<u64>>,
    },
    ValidateTelemetry {
        record: Value,
        allowed_cpus: Option<Vec<u64>>,
    },
    ValidateFixture {
        profile: String,
        record: Value,
        expected_page_size: Option<u64>,
    },
    ValidateTiming {
        profile: String,
        record: Value,
        count: u64,
    },
    CalibrationBatch {
        records: Vec<Value>,
        count: u64,
        batch: u64,
    },
    ValidateCalibrationWorker {
        profile: String,
        records: Vec<Value>,
        count: u64,
        batch: u64,
        expected_page_size: Option<u64>,
    },
    ValidateWorker {
        profile: String,
        records: Vec<Value>,
        count: u64,
        allowed_cpus: Option<Vec<u64>>,
        expected_page_size: Option<u64>,
    },
}

impl Request {
    pub(crate) fn execute(self) -> Result<Value> {
        match self {
            Self::Batch { request } => return request.execute(),
            Self::CalibrationProfile {
                profile,
                revision,
                records,
                expected_page_size,
                allowed_cpus,
            } => {
                return calibration_profile::analyze(
                    &records,
                    &profile,
                    &revision,
                    expected_page_size,
                    allowed_cpus.as_deref(),
                );
            }
            Self::Catalog => {
                return Ok(json!({
                    "protocol": protocol::value(),
                    "repeat_drift_limit": REPEAT_DRIFT_LIMIT,
                    "service_window": {
                        "target_wall_ns": service_window::TARGET_WALL_NS,
                        "target_timed_ns": service_window::TARGET_TIMED_NS,
                        "minimum_wall_ns": service_window::MINIMUM_WALL_NS,
                        "minimum_timed_ns": service_window::MINIMUM_TIMED_NS,
                    },
                    "hotspot_cases": catalog::HOTSPOT_CASES.iter().map(|c|
                        (c.data, c.parity, c.name, c.missing, c.required)).collect::<Vec<_>>(),
                    "cases": case_catalog(),
                }));
            }
            Self::WorkloadContract { profile, case } => return workload_contract(&profile, &case),
            Self::FixtureContract { profile, case } => return fixture_contract(&profile, &case),
            Self::ValidateWorkload { profile, record } => validate_workload(&record, &profile)?,
            Self::ValidateWorkloadSequence { profile, records } => {
                workload_sequence(&records, &profile)?
            }
            Self::ValidatePlacement {
                record,
                allowed_cpus,
            } => placement(&record, allowed_cpus.as_deref())?,
            Self::ValidateTelemetry {
                record,
                allowed_cpus,
            } => telemetry(&record, allowed_cpus.as_deref())?,
            Self::ValidateFixture {
                profile,
                record,
                expected_page_size,
            } => fixture(&record, &profile, expected_page_size)?,
            Self::ValidateTiming {
                profile,
                record,
                count,
            } => timing(&record, &profile, count)?,
            Self::CalibrationBatch {
                records,
                count,
                batch,
            } => return calibration(&records, count, batch),
            Self::ValidateCalibrationWorker {
                profile,
                records,
                count,
                batch,
                expected_page_size,
            } => {
                return calibration_worker(
                    &records,
                    &profile,
                    count,
                    batch,
                    expected_page_size,
                    None,
                );
            }
            Self::ValidateWorker {
                profile,
                records,
                count,
                allowed_cpus,
                expected_page_size,
            } => worker(
                &records,
                &profile,
                count,
                allowed_cpus.as_deref(),
                expected_page_size,
            )?,
        }
        Ok(Value::Null)
    }
}

pub(crate) fn run(args: &Args) -> Result<()> {
    let value = artifact::read_json(std::path::Path::new(args.required("--input")?))?;
    let request: Request = serde_json::from_value(value)?;
    println!("{}", serde_json::to_string(&request.execute()?)?);
    Ok(())
}

fn keys(value: &Value, expected: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|o| o.len() == expected.len() && expected.iter().all(|k| o.contains_key(*k)))
}
fn text<'a>(value: &'a Value, message: &str) -> Result<&'a str> {
    value.as_str().ok_or_else(|| message.into())
}
fn uint(value: &Value, message: &str) -> Result<u64> {
    value.as_u64().ok_or_else(|| message.into())
}
fn positive(value: &Value, message: &str) -> Result<f64> {
    value
        .as_f64()
        .filter(|v| v.is_finite() && *v > 0.0)
        .ok_or_else(|| message.into())
}
fn same_contract(value: &Value, expected: &Value, extra: &[&str]) -> bool {
    let (Some(actual), Some(expected)) = (value.as_object(), expected.as_object()) else {
        return false;
    };
    actual.len() == expected.len() + extra.len()
        && expected
            .iter()
            .all(|(key, value)| actual.get(key) == Some(value))
        && extra.iter().all(|key| actual.contains_key(*key))
}

struct Case<'a> {
    operation: &'a str,
    data: u32,
    parity: u32,
    size: u64,
}
fn shape<'a>(profile: &str, case: &'a str, max_shards: u32) -> Result<Case<'a>> {
    let parts: Vec<_> = case.split(':').collect();
    if parts.len() != if profile == "stream" { 5 } else { 4 } {
        return Err("invalid caller-fixture/workload case".into());
    }
    let data: u32 = parts[1].parse()?;
    let parity: u32 = parts[2].parse()?;
    let size: u64 = parts[3].parse()?;
    if data == 0
        || parity == 0
        || data.checked_add(parity).is_none_or(|v| v > max_shards)
        || size == 0
    {
        return Err("invalid caller-fixture shape".into());
    }
    Ok(Case {
        operation: parts[0],
        data,
        parity,
        size,
    })
}
fn api(entry: &str, policy: &str, missing: u64, required: u64) -> Value {
    json!({"call_entry":entry,"output_policy":policy,"missing_mask":missing,"required_mask":required})
}
pub(crate) fn fixture_contract(profile: &str, case: &str) -> Result<Value> {
    let allowed = match profile {
        "small_file" => catalog::SMALL_KERNELS,
        "stream" => catalog::STREAM_KERNELS,
        _ => return Ok(Value::Null),
    };
    let c = shape(profile, case, 32)?;
    if !allowed
        .iter()
        .any(|(_, operation)| *operation == c.operation)
    {
        return Err("unexpected caller-fixture operation".into());
    }
    let contract = catalog::fixture(c.operation)?;
    Ok(api(
        contract.entry,
        contract.policy,
        contract.missing.value(c.data, c.parity),
        contract.required.value(c.data, c.parity),
    ))
}
fn hotspot(case: &str) -> Result<&'static catalog::HotspotCase> {
    catalog::HOTSPOT_CASES
        .iter()
        .find(|c| c.name == case)
        .ok_or_else(|| "unknown hotspot workload".into())
}

pub(crate) fn case_catalog() -> std::collections::BTreeMap<&'static str, Vec<String>> {
    let mut result = std::collections::BTreeMap::new();
    result.insert(
        "hotspot",
        catalog::HOTSPOT_CASES
            .iter()
            .map(|c| c.name.to_owned())
            .collect(),
    );
    let smoke = [(4, 2, 65536), (10, 4, 1048576), (32, 16, 1048576)];
    let mut small: Vec<_> = [1, 4, 16, 64, 128, 256, 512]
        .iter()
        .map(|size| (4, 2, size * 1024))
        .collect();
    small.extend([16, 64, 256, 512].iter().map(|size| (10, 4, size * 1024)));
    for (profile, shapes, operations) in [
        (
            "smoke",
            smoke.as_slice(),
            vec![
                "encode",
                "update",
                "verify",
                "reconstruct",
                "reconstruct_data",
            ],
        ),
        (
            "small_file",
            small.as_slice(),
            catalog::SMALL_KERNELS.iter().map(|(_, op)| *op).collect(),
        ),
    ] {
        result.insert(
            profile,
            operations
                .iter()
                .flat_map(|op| {
                    shapes
                        .iter()
                        .map(move |(data, parity, size)| format!("{op}:{data}:{parity}:{size}"))
                })
                .collect(),
        );
    }
    let stream = [
        (4, 2, 65536, 65536),
        (4, 2, 1048576, 262144),
        (10, 4, 65536, 65536),
        (10, 4, 1048576, 1048576),
        (10, 4, 16777216, 4194304),
    ];
    result.insert(
        "stream",
        catalog::STREAM_KERNELS
            .iter()
            .flat_map(|(_, op)| {
                stream.iter().map(move |(data, parity, size, block)| {
                    format!("{op}:{data}:{parity}:{size}:{block}")
                })
            })
            .collect(),
    );
    result
}

pub(crate) fn known_case(profile: &str, case: &str) -> bool {
    static CASES: std::sync::LazyLock<std::collections::BTreeMap<&'static str, Vec<String>>> =
        std::sync::LazyLock::new(case_catalog);
    CASES
        .get(profile)
        .is_some_and(|cases| cases.iter().any(|value| value == case))
}
fn seed(operation: &str, data: u32, parity: u32, size: u64) -> Result<u64> {
    let tag = match operation {
        "encode" => 0x11,
        "verify" => 0x22,
        "reconstruct" => 0x33,
        "reconstruct_data" => 0x44,
        "update" => 0x55,
        _ => return Err("unknown workload operation".into()),
    };
    Ok(BASE_SEED ^ tag ^ (u64::from(data) << 48) ^ (u64::from(parity) << 32) ^ size)
}
fn mask(indices: &[usize]) -> u64 {
    indices.iter().fold(0, |mask, index| mask | (1 << index))
}
pub(crate) fn workload_contract(profile: &str, case: &str) -> Result<Value> {
    let (mut contract, seed, lifetime, construction, generator) = if profile == "hotspot" {
        let c = hotspot(case)?;
        let hash = case.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
        (
            api(
                "Vec<Option<Vec<u8>>>",
                "iteration-random-source-cloned-baseline-and-candidate-missing-outputs-untracked",
                mask(c.missing),
                c.required.map_or((1 << c.data) - 1, mask),
            ),
            seed("reconstruct_data", c.data as u32, c.parity as u32, 1048576)? ^ hash,
            "iteration",
            "codec-before-iteration-source",
            "worker-seeded-SmallRng-u32-low-byte",
        )
    } else {
        let c = shape(profile, case, if profile == "smoke" { 63 } else { 32 })?;
        match profile {
            "smoke" => {
                let (entry, policy, missing, required) = match c.operation {
                    "encode" => (
                        "Vec<Vec<u8>>",
                        "iteration-generated-source-parity-in-place",
                        0,
                        0,
                    ),
                    "update" => (
                        "Vec<&Vec<u8>>+Vec<Option<&Vec<u8>>>->Vec<&mut Vec<u8>>",
                        "iteration-generated-source-cloned-old-updated-and-parity",
                        0,
                        0,
                    ),
                    "verify" => (
                        "Vec<Vec<u8>>",
                        "iteration-generated-source-internal-scratch-untracked",
                        0,
                        0,
                    ),
                    "reconstruct" => (
                        "Vec<Option<Vec<u8>>>",
                        "iteration-generated-source-moved-options-drop-missing-new-output-untracked",
                        1 | (1 << c.data),
                        (1 << (c.data + c.parity)) - 1,
                    ),
                    "reconstruct_data" => (
                        "Vec<Option<Vec<u8>>>",
                        "iteration-generated-source-moved-options-drop-missing-new-output-untracked",
                        3,
                        (1 << c.data) - 1,
                    ),
                    _ => return Err("unknown smoke workload".into()),
                };
                (
                    api(entry, policy, missing, required),
                    seed(c.operation, c.data, c.parity, c.size)?,
                    "iteration",
                    "codec-before-iteration-source",
                    "per-iteration-seeded-SmallRng-random-u8",
                )
            }
            "small_file" | "stream" => {
                let operation = c.operation.strip_suffix("_stream").unwrap_or(c.operation);
                let operation = match operation {
                    "verify_with_buffer" => "verify",
                    "reconstruct_opt" | "reconstruct_shard_slot" => "reconstruct",
                    "reconstruct_some_data_only" => "reconstruct_data",
                    op => op,
                };
                (
                    fixture_contract(profile, case)?,
                    seed(operation, c.data, c.parity, c.size)?,
                    "worker",
                    "source-before-codec",
                    "worker-seeded-SmallRng-random-u8",
                )
            }
            _ => return Err("unknown workload profile".into()),
        }
    };
    let values = contract.as_object_mut().ok_or("missing API contract")?;
    values.extend([
        (
            "changed_mask".into(),
            json!(u64::from(profile == "smoke" && case.starts_with("update:"))),
        ),
        ("codec_lifetime".into(), json!("worker")),
        ("source_lifetime".into(), json!(lifetime)),
        ("construction".into(), json!(construction)),
        ("generator".into(), json!(generator)),
        ("seed".into(), json!(seed)),
    ]);
    Ok(contract)
}
fn pattern(indices: &[usize], data: usize) -> String {
    indices
        .iter()
        .map(|i| {
            if *i < data {
                format!("d{i}")
            } else {
                format!("p{}", i - data)
            }
        })
        .collect::<Vec<_>>()
        .join("|")
}
fn validate_workload(record: &Value, profile: &str) -> Result<()> {
    let case = text(&record["case"], "missing workload case")?;
    let expected = workload_contract(profile, case)?;
    workload_matches(record, profile, case, &expected)
}

fn workload_matches(record: &Value, profile: &str, case: &str, expected: &Value) -> Result<()> {
    let workload = &record["workload"];
    if !same_contract(workload, expected, &["rng"]) {
        return Err("missing or incompatible API/seed/preparation workload contract".into());
    }
    let rows = if record["rows"].is_null() {
        &[][..]
    } else {
        record["rows"]
            .as_array()
            .ok_or("invalid raw workload rows")?
    };
    if profile != "hotspot" {
        if !workload["rng"].is_null() {
            return Err("unexpected dynamic random stream".into());
        }
        if rows.iter().any(|row| row["seed"] != expected["seed"]) {
            return Err("raw row seed differs from the actual workload".into());
        }
        return Ok(());
    }
    let c = hotspot(case)?;
    let useful = c.required.map_or_else(
        || {
            c.missing
                .iter()
                .copied()
                .filter(|i| *i < c.data)
                .collect::<Vec<_>>()
        },
        <[usize]>::to_vec,
    );
    let wanted = json!({"data_shards":c.data,"parity_shards":c.parity,"shard_size":1048576,
        "useful_shards":useful.len(),"baseline_operation":if c.required.is_none() {"reconstruct"} else {"reconstruct_data"},
        "candidate_operation":if c.required.is_none() {"reconstruct_data"} else {"reconstruct_some"},
        "missing_pattern":pattern(c.missing, c.data),"required_pattern":pattern(&useful, c.data)});
    if rows.iter().any(|row| {
        wanted
            .as_object()
            .is_some_and(|fields| fields.iter().any(|(k, v)| row.get(k) != Some(v)))
    }) {
        return Err("hotspot raw row differs from the actual API/mask workload".into());
    }
    let count = uint(&record["iterations"], "invalid random stream observation")?;
    let index = uint(&record["index"], "invalid random stream observation")?;
    if count == 0 || index >= 12 {
        return Err("invalid random stream observation".into());
    }
    let step = count
        .checked_mul((c.data + c.parity) as u64 * 1048576)
        .ok_or("random input volume overflow")?;
    let trace = &workload["rng"];
    if !keys(
        trace,
        &["bytes_before", "bytes_after", "next_before", "next_after"],
    ) {
        return Err("missing random stream state witness".into());
    }
    for (key, multiplier) in [("bytes_before", index), ("bytes_after", index + 1)] {
        let bytes = step
            .checked_mul(multiplier)
            .ok_or("random input volume overflow")?;
        if trace[key].as_u64() != Some(bytes) {
            return Err(
                "random input consumption differs from the original iteration workload".into(),
            );
        }
    }
    for key in ["next_before", "next_after"] {
        if !trace[key]
            .as_array()
            .is_some_and(|v| v.len() == 2 && v.iter().all(|word| word.as_u64().is_some()))
        {
            return Err("invalid random stream preview".into());
        }
    }
    Ok(())
}
fn workload_sequence<T: Borrow<Value>>(records: &[T], profile: &str) -> Result<()> {
    use std::collections::{BTreeMap, btree_map::Entry};
    let mut contracts = BTreeMap::new();
    for record in records {
        let record = record.borrow();
        let case = text(&record["case"], "missing workload case")?;
        let expected = match contracts.entry(case) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(workload_contract(profile, case)?),
        };
        workload_matches(record, profile, case, expected)?;
    }
    if profile == "hotspot"
        && records.windows(2).any(|pair| {
            pair[0].borrow()["workload"]["rng"]["next_after"]
                != pair[1].borrow()["workload"]["rng"]["next_before"]
        })
    {
        return Err("random stream was reset or changed inside a worker".into());
    }
    Ok(())
}

fn placement(record: &Value, allowed: Option<&[u64]>) -> Result<()> {
    let p = &record["placement"];
    if !keys(
        p,
        &[
            "cpu",
            "constructor_budget",
            "constructor_affinity",
            "pool_threads",
            "affinity_checked",
        ],
    ) {
        return Err("actual measuring-thread placement is required".into());
    }
    let cpu = uint(&p["cpu"], "invalid prepared codec/pool placement")?;
    let budget = uint(
        &p["constructor_budget"],
        "invalid prepared codec/pool placement",
    )?;
    if budget == 0 || p["pool_threads"].as_u64() != Some(4) || p["affinity_checked"] != true {
        return Err("invalid prepared codec/pool placement".into());
    }
    let raw = p["constructor_affinity"]
        .as_array()
        .ok_or("invalid actual constructor/pool CPU mask")?;
    let mask: Vec<_> = raw
        .iter()
        .map(|v| uint(v, "invalid actual constructor/pool CPU mask"))
        .collect::<Result<_>>()?;
    if mask.is_empty()
        || mask.len() > 65536
        || mask.windows(2).any(|v| v[0] >= v[1])
        || !mask.contains(&cpu)
        || budget > mask.len() as u64
    {
        return Err("invalid actual constructor/pool CPU mask".into());
    }
    if let Some(allowed) = allowed {
        let mut reserved = allowed.to_vec();
        reserved.sort_unstable();
        if reserved.first() != Some(&cpu) || reserved != mask {
            return Err("measuring-thread placement differs from the reservation".into());
        }
    }
    if ["before", "after"]
        .iter()
        .any(|key| record[*key]["cpu"].as_u64() != Some(cpu))
    {
        return Err("sample CPU differs from the fixed measuring thread".into());
    }
    Ok(())
}
pub(crate) fn telemetry(record: &Value, allowed: Option<&[u64]>) -> Result<()> {
    for point in [&record["before"], &record["after"]] {
        if !keys(point, COUNTERS) {
            return Err("per-sample scheduling telemetry is required".into());
        }
        if COUNTERS.iter().any(|key| point[*key].as_u64().is_none()) {
            return Err("invalid scheduling counters".into());
        }
        if allowed.is_some_and(|cpus| !cpus.iter().any(|cpu| point["cpu"].as_u64() == Some(*cpu))) {
            return Err("sample escaped the reserved CPU set".into());
        }
    }
    if COUNTERS[1..]
        .iter()
        .any(|key| record["after"][*key].as_u64() < record["before"][*key].as_u64())
    {
        return Err("scheduling counters moved backwards".into());
    }
    Ok(())
}
pub(crate) fn valid_page_size(page: u64) -> bool {
    (1024..=1048576).contains(&page) && page.is_power_of_two()
}
pub(crate) fn fixture(record: &Value, profile: &str, page: Option<u64>) -> Result<()> {
    let contract = fixture_contract(profile, text(&record["case"], "missing fixture case")?)?;
    let f = &record["fixture"];
    if contract.is_null() {
        return if f.is_null() {
            Ok(())
        } else {
            Err("unexpected fixture layout declaration".into())
        };
    }
    if !same_contract(
        f,
        &contract,
        &["entry", "lifetime", "page_size", "source", "scratch"],
    ) {
        return Err("API entry or destination allocation contract differs".into());
    }
    if f["entry"] != "Vec<Vec<u8>>" || f["lifetime"] != "worker" {
        return Err("caller storage requires a retained Vec fixture layout".into());
    }
    let actual = uint(
        &f["page_size"],
        "fixture page size differs from the recorded runtime",
    )?;
    if !valid_page_size(actual) || page.is_some_and(|p| p != actual) {
        return Err("fixture page size differs from the recorded runtime".into());
    }
    let c = shape(
        profile,
        record["case"].as_str().ok_or("missing fixture case")?,
        32,
    )?;
    let source_count = if c.operation == "encode_stream" {
        c.data
    } else {
        c.data + c.parity
    };
    for (role, count) in [
        ("source", source_count),
        (
            "scratch",
            if c.operation == "verify_with_buffer" {
                c.parity
            } else {
                0
            },
        ),
    ] {
        positions(&f[role], count as usize, c.size, actual)?;
    }
    Ok(())
}
pub(crate) fn positions(value: &Value, count: usize, size: u64, page: u64) -> Result<()> {
    if !valid_page_size(page) {
        return Err("invalid fixture page size".into());
    }
    let buffers = value
        .as_array()
        .filter(|v| v.len() == count)
        .ok_or("incomplete fixture buffer layout")?;
    for buffer in buffers {
        if !keys(
            buffer,
            &["len", "capacity", "offset_64", "offset_4096", "offset_page"],
        ) {
            return Err("invalid fixture buffer position".into());
        }
        let n = |key: &str| uint(&buffer[key], "invalid fixture buffer position");
        let (len, cap, small, large, offset) = (
            n("len")?,
            n("capacity")?,
            n("offset_64")?,
            n("offset_4096")?,
            n("offset_page")?,
        );
        let modulus = page.min(4096);
        if len != size
            || cap < size
            || large >= 4096
            || small >= 64
            || large % 64 != small
            || offset >= page
            || offset % modulus != large % modulus
        {
            return Err("invalid fixture shape or offset".into());
        }
    }
    Ok(())
}

fn timing(record: &Value, profile: &str, count: u64) -> Result<()> {
    let timed = positive(&record["timed_ns"], "invalid measured interval")?;
    positive(&record["wall_ns"], "invalid measured interval")?;
    let row = record["rows"]
        .as_array()
        .and_then(|v| v.first())
        .ok_or("missing raw measurement")?;
    let (expected, tolerance) = if profile == "hotspot" {
        let useful = positive(&row["useful_shards"], "invalid useful shard count")?;
        let size = positive(&row["shard_size"], "invalid shard size")?;
        let throughput = positive(&row["candidate_mb_s"], "invalid measured throughput")?;
        let expected = useful * size * 1e9 * count as f64 / (1048576.0 * throughput);
        (expected, expected * 0.00001 + 1.0)
    } else {
        let expected = positive(&row["ns_per_iter"], "invalid measured latency")? * count as f64;
        (expected, 0.006 * count as f64 + expected * 0.00001)
    };
    if !expected.is_finite() || (expected - timed).abs() > tolerance {
        return Err("timed interval differs from the raw exported measurement".into());
    }
    Ok(())
}
fn calibration_worker<T: Borrow<Value>>(
    records: &[T],
    profile: &str,
    count: u64,
    batch: u64,
    page: Option<u64>,
    cpus: Option<&[u64]>,
) -> Result<Value> {
    workload_sequence(records, profile)?;
    for record in records {
        observation(record.borrow(), profile, count, cpus, page)?;
    }
    calibration(records, count, batch)
}
fn calibration<T: Borrow<Value>>(records: &[T], count: u64, batch: u64) -> Result<Value> {
    if !(1..=1_000_000_000).contains(&count)
        || !(1..=protocol::MAX_CALIBRATION_BATCHES).contains(&batch)
        || records.len() != protocol::WIDTH
    {
        return Err("invalid bounded calibration batch".into());
    }
    for (index, record) in records.iter().enumerate() {
        let record = record.borrow();
        let kind = if index < protocol::WARMUPS {
            "calibration_warmup"
        } else {
            "calibration"
        };
        if record["kind"] != kind
            || record["index"].as_u64() != Some(index as u64)
            || record["iterations"].as_u64() != Some(count)
            || record["calibration_batch"].as_u64() != Some(batch)
            || record["isolated_case"] != true
        {
            return Err(
                "calibration requires the complete fixed warmup and rehearsal schedule".into(),
            );
        }
        let wall = positive(&record["wall_ns"], "invalid calibration interval")?;
        let timed = positive(&record["timed_ns"], "invalid calibration interval")?;
        let met = wall >= service_window::TARGET_WALL_NS as f64
            && timed >= service_window::TARGET_TIMED_NS as f64;
        if record["target_met"].as_bool() != Some(met) {
            return Err("calibration target flag differs from measured intervals".into());
        }
    }
    if records
        .iter()
        .any(|r| r.borrow()["fixture"] != records[0].borrow()["fixture"])
    {
        return Err("fixture layout changed inside a calibration worker".into());
    }
    let complete = records[protocol::WARMUPS..]
        .iter()
        .all(|r| r.borrow()["target_met"] == true);
    if records.iter().enumerate().any(|(i, r)| {
        r.borrow()["complete"].as_bool() != Some(complete && i == protocol::WIDTH - 1)
    }) {
        return Err("calibration lacks the complete measurement rehearsal".into());
    }
    if complete {
        return Ok(Value::Null);
    }
    let desired = records[protocol::WARMUPS..]
        .iter()
        .map(|r| {
            let r = r.borrow();
            (service_window::TARGET_WALL_NS as f64 / r["wall_ns"].as_f64().unwrap_or(1.0))
                .max(service_window::TARGET_TIMED_NS as f64 / r["timed_ns"].as_f64().unwrap_or(1.0))
        })
        .fold(0.0_f64, f64::max)
        * 1.25;
    let following = count * desired.clamp(2.0, 64.0).ceil() as u64;
    if following > 1_000_000_000 {
        return Err("calibration iteration budget exceeded".into());
    }
    Ok(json!(following))
}
fn worker<T: Borrow<Value>>(
    records: &[T],
    profile: &str,
    count: u64,
    cpus: Option<&[u64]>,
    page: Option<u64>,
) -> Result<()> {
    workload_sequence(records, profile)?;
    if records.len() != 12
        || records.iter().enumerate().any(|(i, r)| {
            let r = r.borrow();
            r["kind"] != if i < 3 { "warmup" } else { "sample" }
                || r["index"].as_u64() != Some(i as u64)
        })
    {
        return Err("every fixed warmup and sample is required".into());
    }
    for record in records {
        let record = record.borrow();
        observation(record, profile, count, cpus, page)?;
        if record["fixture"] != records[0].borrow()["fixture"] {
            return Err("fixture layout changed inside a measurement worker".into());
        }
        if record["iterations"].as_u64() != Some(count)
            || record["calibration_batch"].as_u64() != Some(0)
            || record["isolated_case"] != true
            || !record["rows"].as_array().is_some_and(|v| v.len() == 1)
        {
            return Err("sample count/iterations mismatch".into());
        }
    }
    Ok(())
}

/// Diagnostic-only schedule. Formal workers still require native telemetry.
pub(crate) fn diagnostic_worker(
    records: &[Value],
    profile: &str,
    page: u64,
    cpus: Option<&[u64]>,
    calibrate: bool,
) -> Result<()> {
    workload_sequence(records, profile)?;
    if records.len() != 12 {
        return Err("require complete diagnostic mode observations".into());
    }
    for (index, r) in records.iter().enumerate() {
        let kind = match (calibrate, index < 3) {
            (true, true) => "calibration_warmup",
            (true, false) => "calibration",
            (false, true) => "warmup",
            (false, false) => "sample",
        };
        if r["kind"] != kind
            || r["index"].as_u64() != Some(index as u64)
            || r["iterations"].as_u64() != Some(2)
            || r["calibration_batch"].as_u64() != Some(u64::from(calibrate))
            || r["isolated_case"] != true
            || !r["rows"].as_array().is_some_and(|rows| rows.len() == 1)
        {
            return Err("wrong original diagnostic observation schedule".into());
        }
        fixture(r, profile, Some(page))?;
        timing(r, profile, 2)?;
        if cpus.is_some() {
            telemetry(r, cpus)?;
            placement(r, cpus)?;
        } else if ["before", "after", "placement"]
            .iter()
            .any(|key| r.get(*key) != Some(&Value::Null))
        {
            return Err("native observations require recorded affinity".into());
        }
    }
    if calibrate {
        calibration(records, 2, 1)?;
    }
    Ok(())
}

fn observation(
    record: &Value,
    profile: &str,
    count: u64,
    cpus: Option<&[u64]>,
    page: Option<u64>,
) -> Result<()> {
    telemetry(record, cpus)?;
    placement(record, cpus)?;
    fixture(record, profile, page)?;
    timing(record, profile, count)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;

    pub(crate) fn record(profile: &str, case: &str, index: u64) -> Value {
        let mut workload = workload_contract(profile, case).unwrap();
        let mut row = json!({"seed":workload["seed"],"ns_per_iter":200_000_000.0});
        let mut timed = 200_000_000.0;
        workload["rng"] = Value::Null;
        if profile == "hotspot" {
            let c = hotspot(case).unwrap();
            let useful = c.required.map_or_else(
                || {
                    c.missing
                        .iter()
                        .copied()
                        .filter(|i| *i < c.data)
                        .collect::<Vec<_>>()
                },
                <[usize]>::to_vec,
            );
            row = json!({"data_shards":c.data,"parity_shards":c.parity,"shard_size":1048576,
                "useful_shards":useful.len(),"baseline_operation":if c.required.is_none() {"reconstruct"} else {"reconstruct_data"},
                "candidate_operation":if c.required.is_none() {"reconstruct_data"} else {"reconstruct_some"},
                "missing_pattern":pattern(c.missing,c.data),"required_pattern":pattern(&useful,c.data),"candidate_mb_s":5.0});
            timed = useful.len() as f64 * 1e9 / 5.0;
            let step = (c.data + c.parity) as u64 * 1048576;
            workload["rng"] = json!({"bytes_before":index*step,"bytes_after":(index+1)*step,
                "next_before":[index,index],"next_after":[index+1,index+1]});
        }
        let mut fixture = fixture_contract(profile, case).unwrap();
        if !fixture.is_null() {
            let c = shape(profile, case, 32).unwrap();
            let position = json!({"len":c.size,"capacity":c.size,"offset_64":16,"offset_4096":16,"offset_page":16});
            fixture.as_object_mut().unwrap().extend([
                ("entry".into(), json!("Vec<Vec<u8>>")),
                ("lifetime".into(), json!("worker")),
                ("page_size".into(), json!(4096)),
                (
                    "source".into(),
                    json!(vec![
                        position.clone();
                        if c.operation == "encode_stream" {
                            c.data
                        } else {
                            c.data + c.parity
                        } as usize
                    ]),
                ),
                (
                    "scratch".into(),
                    json!(vec![
                        position;
                        if c.operation == "verify_with_buffer" {
                            c.parity
                        } else {
                            0
                        } as usize
                    ]),
                ),
            ]);
        }
        let before = json!({"cpu":0,"run_ns":100,"wait_ns":5,"switches":1,"process_minor_faults":0,"process_major_faults":0});
        json!({"case":case,"kind":if index < 3 {"warmup"} else {"sample"},"index":index,"iterations":1,
            "calibration_batch":0,"isolated_case":true,"fixture":fixture,"workload":workload,"rows":[row],
            "before":before,"after":before,"wall_ns":2_400_000_000_u64,"timed_ns":timed,
            "placement":{"cpu":0,"constructor_budget":8,"constructor_affinity":[0,1,2,3,4,5,6,7],"pool_threads":4,"affinity_checked":true}})
    }

    #[test]
    fn portable_diagnostic_workers_do_not_relax_formal_telemetry() {
        let mut records: Vec<_> = (0..12)
            .map(|index| record("small_file", "verify_with_buffer:4:2:1024", index))
            .collect();
        for r in &mut records {
            r["iterations"] = json!(2);
            r["timed_ns"] = json!(r["timed_ns"].as_f64().unwrap() * 2.0);
            for key in ["before", "after", "placement"] {
                r[key] = Value::Null;
            }
        }
        diagnostic_worker(&records, "small_file", 4096, None, false).unwrap();
        assert!(worker(&records, "small_file", 2, None, Some(4096)).is_err());
        records[0]["before"] = json!({"cpu":0});
        assert!(diagnostic_worker(&records, "small_file", 4096, None, false).is_err());
    }

    #[test]
    fn all_126_workload_and_fixture_contracts_match_the_retired_python_model() {
        let mut values = BTreeMap::new();
        let mut count = 0;
        for (profile, cases) in case_catalog() {
            let mut contracts = BTreeMap::new();
            for case in cases {
                contracts.insert(
                    case.clone(),
                    json!({
                        "workload-contract":workload_contract(profile,&case).unwrap(),
                        "fixture-contract":fixture_contract(profile,&case).unwrap(),
                    }),
                );
                let records: Vec<_> = (0..12).map(|i| record(profile, &case, i)).collect();
                worker(
                    &records,
                    profile,
                    1,
                    Some(&[0, 1, 2, 3, 4, 5, 6, 7]),
                    Some(4096),
                )
                .unwrap();
                let mut rehearsal = records.clone();
                for (i, r) in rehearsal.iter_mut().enumerate() {
                    r["kind"] = json!(if i < 3 {
                        "calibration_warmup"
                    } else {
                        "calibration"
                    });
                    r["calibration_batch"] = json!(1);
                    r["target_met"] = json!(true);
                    r["complete"] = json!(i == 11);
                }
                assert_eq!(
                    Request::ValidateCalibrationWorker {
                        profile: profile.into(),
                        records: rehearsal,
                        count: 1,
                        batch: 1,
                        expected_page_size: Some(4096)
                    }
                    .execute()
                    .unwrap(),
                    Value::Null
                );
                for field in [
                    "seed",
                    "codec_lifetime",
                    "source_lifetime",
                    "construction",
                    "generator",
                    "call_entry",
                    "output_policy",
                    "missing_mask",
                    "required_mask",
                    "changed_mask",
                ] {
                    let mut changed = records[0].clone();
                    changed["workload"][field] = json!(true);
                    assert!(
                        validate_workload(&changed, profile).is_err(),
                        "{case}: accepted {field}"
                    );
                }
                for (field, value) in [
                    ("cpu", json!(true)),
                    ("constructor_budget", json!(0)),
                    ("constructor_budget", json!(9)),
                    ("pool_threads", json!(1)),
                    ("affinity_checked", json!(false)),
                    ("constructor_affinity", json!([0])),
                    ("constructor_affinity", Value::Null),
                ] {
                    let mut changed = records[0].clone();
                    changed["placement"][field] = value;
                    assert!(
                        placement(&changed, Some(&[0, 1, 2, 3, 4, 5, 6, 7])).is_err(),
                        "{case}: accepted {field}"
                    );
                }
                count += 1;
            }
            values.insert(profile, contracts);
        }
        assert_eq!(count, 126);
        // SHA-256 of sorted compact JSON from the original implementation.
        assert_eq!(
            Sha256::digest(serde_json::to_vec(&values).unwrap())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
            "2682f20aaae7a7f6cd8564a845683fd940f60dfe12e02f9c4b4d4bb4b24bf54c"
        );
    }

    #[test]
    fn workload_masks_seed_and_preparation_are_exact_and_integer_typed() {
        let original = record("small_file", "reconstruct_shard_slot:4:2:1024", 0);
        assert_eq!(original["workload"]["missing_mask"], 17);
        assert_eq!(original["workload"]["required_mask"], 63);
        assert_eq!(original["workload"]["seed"], 65406575726428439_u64);
        for key in [
            "seed",
            "call_entry",
            "codec_lifetime",
            "source_lifetime",
            "construction",
            "generator",
            "output_policy",
            "missing_mask",
            "required_mask",
            "changed_mask",
        ] {
            let mut changed = original.clone();
            changed["workload"][key] = json!(true);
            assert!(
                validate_workload(&changed, "small_file").is_err(),
                "accepted {key}"
            );
        }
        let mut changed = original.clone();
        changed["rows"][0]["seed"] = json!(0);
        assert!(
            validate_workload(&changed, "small_file")
                .unwrap_err()
                .to_string()
                .contains("row seed")
        );
        let mut changed = original;
        changed["workload"]["extra"] = Value::Null;
        assert!(validate_workload(&changed, "small_file").is_err());
    }

    #[test]
    fn hotspot_rng_volume_previews_and_continuity_cover_nonmedian_observations() {
        for c in catalog::HOTSPOT_CASES {
            let original: Vec<_> = (0..12).map(|i| record("hotspot", c.name, i)).collect();
            for (field, value) in [
                ("bytes_after", json!(0)),
                ("next_before", json!([0, 0])),
                ("next_after", json!([true, 1])),
            ] {
                let mut changed = original.clone();
                changed[3]["workload"]["rng"][field] = value;
                assert!(workload_sequence(&changed, "hotspot").is_err());
            }
            for field in [
                "data_shards",
                "parity_shards",
                "shard_size",
                "useful_shards",
                "missing_pattern",
                "required_pattern",
                "baseline_operation",
                "candidate_operation",
            ] {
                let mut changed = original[0].clone();
                changed["rows"][0][field] = Value::Null;
                assert!(
                    validate_workload(&changed, "hotspot")
                        .unwrap_err()
                        .to_string()
                        .contains("actual API/mask")
                );
            }
            let mut changed = original[11].clone();
            changed["iterations"] = json!(u64::MAX);
            changed["workload"]["rng"]["bytes_after"] = Value::Null;
            assert!(validate_workload(&changed, "hotspot").is_err());
            for value in [
                Value::Null,
                json!([1]),
                json!([1, true]),
                json!([-1, 1]),
                json!([18446744073709551616.0, 1]),
            ] {
                let mut changed = original[0].clone();
                changed["workload"]["rng"]["next_after"] = value;
                assert!(validate_workload(&changed, "hotspot").is_err());
            }
        }
    }

    #[test]
    fn fixed_cpu_requires_sorted_full_constructor_mask_and_monotonic_telemetry() {
        let original = record("smoke", "encode:4:2:65536", 0);
        for (field, value) in [
            ("cpu", json!(true)),
            ("constructor_budget", json!(9)),
            ("constructor_affinity", json!([1, 0])),
            ("pool_threads", json!(1)),
            ("affinity_checked", json!(false)),
        ] {
            let mut changed = original.clone();
            changed["placement"][field] = value;
            assert!(placement(&changed, Some(&[0, 1, 2, 3, 4, 5, 6, 7])).is_err());
        }
        assert!(placement(&original, Some(&[1, 2, 3, 4, 5, 6, 7])).is_err());
        for field in COUNTERS {
            let mut changed = original.clone();
            changed["after"][*field] = json!(true);
            assert!(telemetry(&changed, None).is_err());
        }
        let mut changed = original.clone();
        changed["after"]["run_ns"] = json!(99);
        assert!(
            telemetry(&changed, None)
                .unwrap_err()
                .to_string()
                .contains("backwards")
        );
        let mut changed = original;
        changed["after"]["cpu"] = json!(1);
        assert!(
            placement(&changed, None)
                .unwrap_err()
                .to_string()
                .contains("fixed measuring thread")
        );
    }

    #[test]
    fn fixture_layout_retains_all_buffers_and_real_page_moduli() {
        let original = record("small_file", "verify_with_buffer:4:2:1024", 0);
        assert_eq!(original["fixture"]["scratch"].as_array().unwrap().len(), 2);
        for (field, value) in [
            ("capacity", json!(0)),
            ("offset_64", json!(true)),
            ("offset_4096", json!(4096)),
            ("offset_page", json!(32)),
            ("len", json!(1023)),
        ] {
            let mut changed = original.clone();
            changed["fixture"]["source"][0][field] = value;
            assert!(fixture(&changed, "small_file", Some(4096)).is_err());
        }
        assert!(fixture(&original, "small_file", Some(65536)).is_err());
        let mut changed = original.clone();
        changed["fixture"]["scratch"] = json!([]);
        assert!(fixture(&changed, "small_file", Some(4096)).is_err());
        let mut changed = original;
        changed["fixture"]["page_size"] = json!(65536);
        changed["fixture"]["source"][0]["offset_page"] = json!(4096 + 16);
        fixture(&changed, "small_file", Some(65536)).unwrap();
    }

    fn calibration_records(wall: u64, timed: u64, count: u64) -> Vec<Value> {
        let met =
            wall >= service_window::TARGET_WALL_NS && timed >= service_window::TARGET_TIMED_NS;
        (0..12)
            .map(|i| {
                json!({"kind":if i < 3 {"calibration_warmup"} else {"calibration"},
            "index":i,"iterations":count,"calibration_batch":1,"isolated_case":true,
            "wall_ns":wall,"timed_ns":timed,"target_met":met,"complete":met && i == 11})
            })
            .collect()
    }

    #[test]
    fn calibration_rehearsal_uses_every_probe_and_keeps_bounded_growth() {
        let complete = calibration_records(2_000_000_000, 100_000_000, 1);
        assert_eq!(calibration(&complete, 1, 1).unwrap(), Value::Null);
        let mut short = complete.clone();
        for record in &mut short {
            record["complete"] = json!(false);
        }
        short[4]["wall_ns"] = json!(200_000_000);
        short[4]["target_met"] = json!(false);
        assert_eq!(calibration(&short, 1, 1).unwrap(), 13);
        let tiny = calibration_records(1, 1, 1);
        assert_eq!(calibration(&tiny, 1, 1).unwrap(), 64);
        let over = calibration_records(1, 1, 1_000_000_000);
        assert!(calibration(&over, 1_000_000_000, 1).is_err());
        let mut invalid = complete;
        invalid[0]["index"] = json!(true);
        assert!(calibration(&invalid, 1, 1).is_err());
    }

    #[test]
    fn worker_checks_all_warmups_and_samples_without_modifying_them() {
        let original: Vec<_> = (0..12)
            .map(|i| record("small_file", "encode:4:2:1024", i))
            .collect();
        for index in [0, 3, 11] {
            for (field, value) in [
                ("timed_ns", json!(1)),
                ("calibration_batch", json!(false)),
                ("iterations", json!(2)),
                ("rows", json!([])),
            ] {
                let mut changed = original.clone();
                changed[index][field] = value;
                assert!(worker(&changed, "small_file", 1, None, Some(4096)).is_err());
            }
            let mut changed = original.clone();
            changed[index]["fixture"]["source"][0]["offset_64"] = json!(32);
            assert!(worker(&changed, "small_file", 1, None, Some(4096)).is_err());
        }
        let mut reordered = original.clone();
        reordered.swap(3, 4);
        assert!(worker(&reordered, "small_file", 1, None, Some(4096)).is_err());
        worker(&original, "small_file", 1, None, Some(4096)).unwrap();
        assert_eq!(original[0]["fixture"]["source"][0]["offset_64"], 16);
    }
}
