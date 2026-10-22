use super::numeric::{center, close_product, difference, positive, ratio, uint};
use crate::{Result, sampling_contract};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
const LENGTHS: [u64; 2] = [1024, 4096];
const OFFSETS: [u64; 4] = [0, 16, 32, 48];
const PHASES: [&str; 4] = ["A1", "B1", "B2", "A2"];
const CENTRAL_SPREAD_LIMIT: f64 = 0.10;
const REPEAT_DRIFT_LIMIT: f64 = 0.05;
const REGRESSION_LIMIT: f64 = 0.12;

pub(super) fn protocol() -> Value {
    json!({"name":"paired-buffered-verify-slices","version":2,
        "offsets":OFFSETS,"lengths":LENGTHS,"passes":["forward","reverse"],
        "warmups":3,"samples":9,"fixture":"same_backing_per_length",
        "page_alignment":"max(os_base_page_size,4096)",
        "iterations":"one-predeclared-count-for-all-cells-and-phases",
        "minimum_wall_ns":sampling_contract::MINIMUM_WALL_NS,
        "minimum_timed_ns":sampling_contract::MINIMUM_TIMED_NS,
        "timing_scope":"wall-and-timed-are-the-same-complete-verify-loop",
        "central_spread_limit":CENTRAL_SPREAD_LIMIT,"repeat_drift_limit":REPEAT_DRIFT_LIMIT,
        "regression_limit":REGRESSION_LIMIT,
        "measuring_affinity":"one-reserved-cpu-before-codec-construction",
        "release_approval":false})
}

#[derive(PartialEq, Eq, Serialize)]
struct Provenance<'a> {
    backend: &'a str,
    backend_id: &'a str,
    backend_kind: &'a str,
    features: &'a str,
    target_arch: &'a str,
    target_os: &'a str,
}

impl<'a> Provenance<'a> {
    fn read(row: &'a Value) -> Result<Self> {
        let field = |key| {
            row[key]
                .as_str()
                .filter(|v| !v.is_empty())
                .ok_or("complete native backend/features/target provenance is required")
        };
        let value = Self {
            backend: field("backend")?,
            backend_id: field("backend_id")?,
            backend_kind: field("backend_kind")?,
            features: field("features")?,
            target_arch: field("target_arch")?,
            target_os: field("target_os")?,
        };
        if value.features != "simd-accel|std" || value.target_os != "linux" {
            return Err("complete native backend/features/target provenance is required".into());
        }
        Ok(value)
    }
}
fn positions(
    row: &Value,
    key: &str,
    count: usize,
    offset: u64,
    page: u64,
    exact: bool,
) -> Result<()> {
    let positions = row[key]
        .as_array()
        .filter(|p| p.len() == count)
        .ok_or("actual buffer offsets differ from the selected layout")?;
    for p in positions {
        let p = uint(p)?;
        if p >= page || p % 64 != offset || exact && p != offset {
            return Err("actual page offsets differ from the controlled layout".into());
        }
    }
    Ok(())
}
fn coordinates(length: u64, pass: u64, cell: usize) -> [u64; 4] {
    let cell = if pass == 0 { cell } else { 15 - cell };
    [length, pass, OFFSETS[cell / 4], OFFSETS[cell % 4]]
}
pub(super) fn analyze(
    records: &[Value],
    iterations: u64,
    page: Option<u64>,
    cpu: Option<u64>,
) -> Result<Value> {
    if !(1..=1_000_000_000).contains(&iterations) {
        return Err("invalid bounded iteration count".into());
    }
    if page.is_some_and(|p| !sampling_contract::valid_page_size(p)) {
        return Err("invalid actual base page size".into());
    }
    if records.len() != 768 {
        return Err("complete forward/reverse grid and every observation are required".into());
    }
    let mut provenance = None;
    let mut cases = Vec::with_capacity(64);
    let mut repeats = Vec::with_capacity(32);
    for (length_index, length) in LENGTHS.into_iter().enumerate() {
        for pass in 0..2 {
            for cell in 0..16 {
                let [_, _, source, scratch] = coordinates(length, pass, cell);
                let start = (length_index * 32 + pass as usize * 16 + cell) * 12;
                let group = &records[start..start + 12];
                for (index, row) in group.iter().enumerate() {
                    for (key, value) in [
                        ("len", length),
                        ("pass", pass),
                        ("source_offset", source),
                        ("scratch_offset", scratch),
                        ("index", index as u64),
                        ("iterations", iterations),
                    ] {
                        if uint(&row[key])? != value {
                            return Err(
                                "layout order or iteration count differs from the complete grid"
                                    .into(),
                            );
                        }
                    }
                    if row["entry"] != "slice_views"
                        || row["fixture_lifetime"] != "same_backing_per_length"
                    {
                        return Err("explicit slice fixture contract is required".into());
                    }
                    if row["kind"] != if index < 3 { "warmup" } else { "sample" } {
                        return Err("all fixed warmups must precede the samples".into());
                    }
                    if uint(&row["wall_ns"])? == 0
                        || !close_product(&row["ns_per_iter"], iterations, &row["wall_ns"])?
                    {
                        return Err("invalid layout timing".into());
                    }
                    positive(&row["ns_per_iter"])?;
                    if let Some(page) = page {
                        if uint(&row["page_size"])? != page {
                            return Err(
                                "actual page size does not match the controlled context".into()
                            );
                        }
                        if Some(uint(&row["cpu"])?) != cpu {
                            return Err(
                                "actual measuring CPU does not match the reserved CPU".into()
                            );
                        }
                        let current = Provenance::read(row)?;
                        if provenance.as_ref().is_some_and(|p| p != &current) {
                            return Err("backend or target changed inside the layout grid".into());
                        }
                        provenance = Some(current);
                        positions(row, "input_4096_offsets", 6, source, 4096, true)?;
                        positions(row, "scratch_4096_offsets", 2, scratch, 4096, true)?;
                    }
                    positions(
                        row,
                        "input_page_offsets",
                        6,
                        source,
                        page.unwrap_or(4096),
                        page.is_some(),
                    )?;
                    positions(
                        row,
                        "scratch_page_offsets",
                        2,
                        scratch,
                        page.unwrap_or(4096),
                        page.is_some(),
                    )?;
                    if row["input_page_offsets"] != group[0]["input_page_offsets"]
                        || row["scratch_page_offsets"] != group[0]["scratch_page_offsets"]
                    {
                        return Err("buffer layout changed inside an observation group".into());
                    }
                }
                let (median, spread) = center(group[3..].iter().map(|r| &r["ns_per_iter"]))?;
                cases.push(json!({"len":length,"pass":pass,"source_offset":source,"scratch_offset":scratch,"median_ns":median,"central_spread":spread}));
            }
        }
        for cell in 0..16 {
            let forward = length_index * 32 + cell;
            let reverse = length_index * 32 + 16 + 15 - cell;
            let (a, b) = (&records[forward * 12], &records[reverse * 12]);
            if a["input_page_offsets"] != b["input_page_offsets"]
                || a["scratch_page_offsets"] != b["scratch_page_offsets"]
            {
                return Err("forward/reverse passes did not reuse the same buffer layout".into());
            }
            repeats.push(json!({"len":length,"source_offset":OFFSETS[cell/4],"scratch_offset":OFFSETS[cell%4],"drift":ratio(&difference(&cases[reverse]["median_ns"],&cases[forward]["median_ns"]),&cases[forward]["median_ns"])?}));
        }
    }
    let short: Vec<_> = records
        .iter()
        .filter(|r| {
            r["kind"] == "sample"
                && r["wall_ns"]
                    .as_u64()
                    .is_some_and(|n| n < sampling_contract::MINIMUM_WALL_NS)
        })
        .collect();
    let wide: Vec<_> = cases
        .iter()
        .filter(|r| r["central_spread"].as_f64().unwrap_or(0.0) > CENTRAL_SPREAD_LIMIT)
        .collect();
    let failed: Vec<_> = repeats
        .iter()
        .filter(|r| r["drift"].as_f64().unwrap_or(0.0) > REPEAT_DRIFT_LIMIT)
        .collect();
    Ok(
        json!({"purpose":"within-version layout diagnosis only; never release acceptance","candidate_attribution":null,"release_approval":false,"provenance":provenance,
        "short_samples":short,"case_count":cases.len(),"formal_samples":cases.len()*9,"cases":cases,"repeat_checks":repeats,"wide_cases":wide,"repeat_failures":failed}),
    )
}
fn passed(report: &Value) -> bool {
    ["short_samples", "wide_cases", "repeat_failures"]
        .iter()
        .all(|key| report[*key].as_array().is_some_and(Vec::is_empty))
}
pub(super) fn paired(
    phases: &BTreeMap<String, Vec<Value>>,
    iterations: u64,
    page: u64,
    cpu: u64,
    diagnostic: bool,
) -> Result<Value> {
    if !sampling_contract::valid_page_size(page) {
        return Err("paired analysis requires actual page size and measuring CPU".into());
    }
    if phases.len() != 4 || PHASES.iter().any(|p| !phases.contains_key(*p)) {
        return Err("complete A1/B1/B2/A2 layout phases are required".into());
    }
    let reports: BTreeMap<_, _> = PHASES
        .into_iter()
        .map(|phase| {
            Ok((
                phase,
                analyze(&phases[phase], iterations, Some(page), Some(cpu))?,
            ))
        })
        .collect::<Result<_>>()?;
    if reports
        .values()
        .any(|r| r["provenance"] != reports["A1"]["provenance"])
    {
        return Err("backend/features/target differ between paired phases".into());
    }
    let mut drift = Vec::with_capacity(128);
    for (left, right) in [("A1", "A2"), ("B1", "B2")] {
        for (index, row) in reports[left]["cases"]
            .as_array()
            .ok_or("missing paired cases")?
            .iter()
            .enumerate()
        {
            let other = &reports[right]["cases"][index];
            drift.push(json!({"pair":format!("{left}/{right}"),"cell":[row["len"],row["pass"],row["source_offset"],row["scratch_offset"]],"drift":ratio(&difference(&other["median_ns"],&row["median_ns"]),&row["median_ns"])?}));
        }
    }
    let qualified = reports.values().all(passed)
        && drift
            .iter()
            .all(|r| r["drift"].as_f64().is_some_and(|d| d <= REPEAT_DRIFT_LIMIT));
    let mut comparisons = Vec::new();
    if qualified || diagnostic {
        for a in ["A1", "A2"] {
            for b in ["B1", "B2"] {
                for (index, row) in reports[a]["cases"]
                    .as_array()
                    .ok_or("missing paired cases")?
                    .iter()
                    .enumerate()
                {
                    comparisons.push(json!({"pair":format!("{a}/{b}"),"cell":[row["len"],row["pass"],row["source_offset"],row["scratch_offset"]],"latency_ratio":ratio(&reports[b]["cases"][index]["median_ns"],&row["median_ns"])?}));
                }
            }
        }
    }
    let regressions: Vec<_> = comparisons
        .iter()
        .filter(|r| {
            qualified
                && r["latency_ratio"]
                    .as_f64()
                    .is_some_and(|r| r > 1.0 + REGRESSION_LIMIT)
        })
        .collect();
    let failed: Vec<_> = drift
        .iter()
        .filter(|r| r["drift"].as_f64().is_some_and(|r| r > REPEAT_DRIFT_LIMIT))
        .collect();
    let attribution = if qualified && regressions.is_empty() && !diagnostic {
        json!(comparisons)
    } else {
        Value::Null
    };
    let mut report = json!({"purpose":"per-layout supplemental acceptance only; original full profiles remain required","release_approval":false,"phase_order":PHASES,"layout_quality_passed":qualified,"layout_gate_passed":qualified && regressions.is_empty(),"phase_reports":reports,"repeat_checks":drift,"repeat_failures":failed,"candidate_attribution":attribution,"regression_failures":regressions});
    if diagnostic {
        let failures: Vec<_> = comparisons
            .iter()
            .filter(|r| {
                r["latency_ratio"]
                    .as_f64()
                    .is_some_and(|r| r > 1.0 + REGRESSION_LIMIT)
            })
            .collect();
        report["diagnostic_only"] = json!(true);
        report["candidate_attribution"] = Value::Null;
        report["unqualified_regression_failures"] = json!(failures);
        report["unqualified_layout_comparisons"] = json!(comparisons);
    }
    Ok(report)
}
