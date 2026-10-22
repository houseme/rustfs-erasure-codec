//! Descriptive machine reports. Rankings never change runtime dispatch policy.
use crate::{Args, Result, artifact};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

const PRIORITY: [&str; 6] = [
    "rust-gfni-avx512",
    "rust-gfni-avx2",
    "rust-avx2",
    "rust-avx512",
    "rust-ssse3",
    "scalar",
];
const OPS: [&str; 4] = ["encode", "verify", "reconstruct", "reconstruct_data"];
type Smoke = BTreeMap<String, Vec<BTreeMap<String, String>>>;

#[derive(Serialize, Deserialize)]
struct CriterionRow {
    benchmark: String,
    length: Option<String>,
    mean_ns: f64,
    lower_ns: f64,
    upper_ns: f64,
}
#[derive(Serialize)]
struct KernelRank {
    backend_override: String,
    benchmark: String,
    length: Option<String>,
    mean_ns: f64,
}
#[derive(Serialize)]
struct SmokeRank {
    backend: String,
    backend_override: String,
    throughput_mb_s: f64,
    source: String,
}

fn collect_criterion(root: &Path) -> Result<Vec<CriterionRow>> {
    #[derive(Deserialize)]
    struct Metadata {
        group_id: String,
        function_id: Option<String>,
    }
    artifact::criterion_files(&root.join("target/criterion"))?
        .into_iter()
        .map(|path| {
            let metadata: Metadata = artifact::read_json(&path.with_file_name("benchmark.json"))?;
            let [mean_ns, lower_ns, upper_ns] = artifact::criterion_mean(&path)?;
            Ok(CriterionRow {
                benchmark: metadata.group_id,
                length: metadata.function_id,
                mean_ns,
                lower_ns,
                upper_ns,
            })
        })
        .collect()
}

fn collect_smoke(root: &Path) -> Result<Smoke> {
    let mut smoke = BTreeMap::new();
    for entry in artifact::entries(&root.join("target/benchmark-smoke"))? {
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "non-UTF8 smoke artifact")?;
        let backend = name
            .strip_prefix("smoke-results-release-")
            .and_then(|v| v.strip_suffix(".csv"));
        if backend.is_some_and(|v| v == "auto" || PRIORITY.contains(&v)) {
            smoke.insert(name, artifact::csv_rows(&entry.path())?);
        }
    }
    Ok(smoke)
}

fn honored(row: &BTreeMap<String, String>) -> bool {
    row.get("override_honored")
        .is_none_or(|v| v.eq_ignore_ascii_case("true"))
}

fn smoke_rankings(smoke: &Smoke) -> Result<BTreeMap<&'static str, Vec<SmokeRank>>> {
    let mut ranks = BTreeMap::new();
    for op in OPS {
        let mut rows = Vec::new();
        for (source, records) in smoke {
            for row in records {
                let backend = row
                    .get("backend_override")
                    .ok_or("missing backend_override")?;
                if (backend != "auto" && !PRIORITY.contains(&backend.as_str())) || !honored(row) {
                    continue;
                }
                if [
                    ("operation", op),
                    ("data_shards", "10"),
                    ("parity_shards", "4"),
                    ("shard_size", "1048576"),
                ]
                .iter()
                .all(|(key, value)| row.get(*key).is_some_and(|v| v == value))
                {
                    let throughput: f64 = row
                        .get("throughput_mb_s")
                        .ok_or("missing throughput_mb_s")?
                        .parse()?;
                    if !throughput.is_finite() || throughput <= 0.0 {
                        return Err("throughput_mb_s must be finite and positive".into());
                    }
                    rows.push(SmokeRank {
                        backend: row.get("backend").ok_or("missing backend")?.clone(),
                        backend_override: backend.clone(),
                        throughput_mb_s: throughput,
                        source: source.clone(),
                    });
                }
            }
        }
        rows.sort_by(|a, b| b.throughput_mb_s.total_cmp(&a.throughput_mb_s));
        ranks.insert(op, rows);
    }
    Ok(ranks)
}

fn kernel_rankings(rows: &[CriterionRow]) -> BTreeMap<&'static str, Vec<KernelRank>> {
    let mut groups: BTreeMap<(&str, Option<&str>), (f64, usize)> = BTreeMap::new();
    for row in rows {
        let (mean, count) = groups
            .entry((&row.benchmark, row.length.as_deref()))
            .or_default();
        *count += 1;
        *mean += (row.mean_ns - *mean) / *count as f64;
    }
    let mut ranks = BTreeMap::from([
        ("galois_mul_slice", Vec::new()),
        ("galois_mul_slice_xor", Vec::new()),
    ]);
    for ((benchmark, length), (mean_ns, _)) in groups {
        let Some((op, _)) = crate::abba::operation(benchmark) else {
            continue;
        };
        let group = if op == "mul_slice_xor" {
            "galois_mul_slice_xor"
        } else {
            "galois_mul_slice"
        };
        let backend = benchmark
            .split_once("_override_")
            .map_or(benchmark, |(_, v)| v.trim_end_matches('_'));
        ranks.get_mut(group).unwrap().push(KernelRank {
            backend_override: backend.to_owned(),
            benchmark: benchmark.to_owned(),
            length: length.map(str::to_owned),
            mean_ns,
        });
    }
    for rows in ranks.values_mut() {
        rows.sort_by(|a, b| a.mean_ns.total_cmp(&b.mean_ns));
    }
    ranks
}

fn recommendations(
    smoke: &BTreeMap<&str, Vec<SmokeRank>>,
    kernels: &BTreeMap<&str, Vec<KernelRank>>,
) -> (Value, Value) {
    // At most seven backends; insertion order retains the old stable tie rule.
    let mut scores: Vec<(String, f64)> = Vec::new();
    let mut add = |backend: &str, value: f64| {
        if backend != "auto" && !PRIORITY.contains(&backend) {
            return;
        }
        if let Some((_, score)) = scores.iter_mut().find(|(b, _)| b == backend) {
            *score += value;
        } else {
            scores.push((backend.to_owned(), value));
        }
    };
    for (op, weight) in [
        ("encode", 1.0),
        ("verify", 1.2),
        ("reconstruct", 1.5),
        ("reconstruct_data", 1.5),
    ] {
        let rows = &smoke[op];
        if let Some(best) = rows.first() {
            for (index, row) in rows.iter().enumerate() {
                add(
                    &row.backend_override,
                    row.throughput_mb_s / best.throughput_mb_s * weight
                        + (rows.len() - index - 1) as f64 * 0.01,
                );
            }
        }
    }
    for op in ["galois_mul_slice", "galois_mul_slice_xor"] {
        for (length, weight) in [("len_1048576", 0.5), ("len_4194304", 0.75)] {
            let rows: Vec<_> = kernels[op]
                .iter()
                .filter(|r| r.length.as_deref() == Some(length))
                .collect();
            if let Some(best) = rows.first() {
                for (index, row) in rows.iter().enumerate() {
                    add(
                        &row.backend_override,
                        best.mean_ns / row.mean_ns * weight
                            + (rows.len() - index - 1) as f64 * 0.005,
                    );
                }
            }
        }
    }
    scores.retain(|(backend, _)| backend != "auto");
    scores.sort_by(|a, b| b.1.total_cmp(&a.1));
    let order: Vec<_> = scores.iter().map(|(backend, _)| backend.as_str()).collect();
    let scored: Vec<_> = scores.iter().map(|(backend,score)| json!({"backend_override":backend,"score":(score*10000.0).round_ties_even()/10000.0})).collect();
    let diverges = order != PRIORITY[..order.len()];
    let recommended = json!({"priority_order":order,"scored_backends":scored,
        "rationale":["Release smoke results for 10x4_1m are weighted most heavily.","Reconstruct and reconstruct_data are weighted above encode.","Criterion mul_slice and mul_slice_xor at 1 MiB and 4 MiB break close ties.","The recommendation is benchmark-driven and may differ from the current runtime dispatch order."],
        "current_runtime_priority_x86":PRIORITY,"diverges_from_current_runtime_priority_x86":diverges});
    let mut eligible = PRIORITY;
    eligible.sort();
    let policy = json!({"priority_order":order,"scored_backends":scored,
        "rationale":["Starts from the benchmark-driven ranking.","Filters out backends that are not currently eligible for default runtime dispatch.","The eligible set is aligned to the runtime order implemented in src/galois_8/backend.rs."],
        "eligible_backends_x86":eligible,"current_runtime_priority_x86":PRIORITY,"diverges_from_current_runtime_priority_x86":diverges});
    (recommended, policy)
}

#[derive(Serialize)]
struct MachineReport<'a> {
    #[serde(flatten)]
    machine: Value,
    date_utc: &'a str,
    machine_slug: &'a str,
    criterion_galois_backend: &'a [CriterionRow],
    release_smoke: &'a Smoke,
    release_smoke_override_mismatches: BTreeMap<&'a String, Vec<&'a BTreeMap<String, String>>>,
    rankings_10x4_1m: &'a BTreeMap<&'static str, Vec<SmokeRank>>,
    criterion_rankings: &'a BTreeMap<&'static str, Vec<KernelRank>>,
    recommended_default_priority: Value,
    policy_eligible_default_priority: Value,
    adoption_decision_stub: Value,
    default_switch_checklist: Value,
}

fn write_report(root: &Path, slug: &str, date: &str, output: &Path) -> Result<()> {
    let smoke = collect_smoke(root)?;
    let criterion = collect_criterion(root)?;
    // One ranking/score pass feeds both reports; never re-read artifacts to rank.
    let ranks = smoke_rankings(&smoke)?;
    let kernels = kernel_rankings(&criterion);
    let (recommended, policy) = recommendations(&ranks, &kernels);
    let mismatches: BTreeMap<_, Vec<_>> = smoke
        .iter()
        .map(|(file, rows)| (file, rows.iter().filter(|row| !honored(row)).collect()))
        .collect();
    let count: usize = mismatches.values().map(Vec::len).sum();
    let adoption_decision_stub = json!({"status":"manual-review-required","reason":"same-machine evidence must be reviewed manually before changing the default runtime priority",
            "override_mismatch_count":count,"diverges_from_current_runtime_priority_x86":recommended["diverges_from_current_runtime_priority_x86"],
            "recommended_priority_order":recommended["priority_order"],"suggested_labels":["candidate-only","candidate-default","fallback-only"]});
    let default_switch_checklist = json!({"same_machine_required":true,"repeat_runs_required":true,"kernel_and_workload_evidence_required":true,
            "override_mismatches_must_be_empty":true,"manual_review_required_when_priority_diverges":true});
    let report = MachineReport {
        machine: crate::metadata::machine_info(),
        date_utc: date,
        machine_slug: slug,
        criterion_galois_backend: &criterion,
        release_smoke: &smoke,
        release_smoke_override_mismatches: mismatches,
        rankings_10x4_1m: &ranks,
        criterion_rankings: &kernels,
        recommended_default_priority: recommended,
        policy_eligible_default_priority: policy,
        adoption_decision_stub,
        default_switch_checklist,
    };
    // Serialize borrowed rows directly; do not build a second full JSON tree.
    artifact::write_json(output, &report)
}

fn archive(root: &Path) -> Result<Vec<Value>> {
    let mut reports = Vec::new();
    for entry in artifact::entries(&root.join("benchmarks/x86_64-simd"))? {
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "non-UTF8 archive name")?;
        if !name.ends_with(".json") || name.ends_with(".run-meta.json") {
            continue;
        }
        let report: Value = artifact::read_json(&entry.path())?;
        // Raw benchmark arrays share this directory with machine reports.
        if report.is_array() {
            continue;
        }
        if !report.is_object() {
            return Err(format!("{name}: expected a machine-report object").into());
        }
        let mut row = json!({"file":name,"recommended_default_priority":report["recommended_default_priority"]["priority_order"].as_array().cloned().unwrap_or_default(),
            "recommendation_rationale":report["recommended_default_priority"]["rationale"].as_array().cloned().unwrap_or_default(),
            "adoption_decision_status":report["adoption_decision_stub"]["status"].as_str().unwrap_or("legacy-archive-no-stub"),
            "override_mismatch_count":report["adoption_decision_stub"]["override_mismatch_count"].as_u64().unwrap_or(0)});
        for op in OPS {
            row[op] = report["rankings_10x4_1m"][op][0]["backend_override"]
                .as_str()
                .unwrap_or("n/a")
                .into();
        }
        reports.push(row);
    }
    Ok(reports)
}

pub(crate) fn run(args: &Args) -> Result<()> {
    if args.0.contains_key("--summary") {
        println!("{}", serde_json::to_string_pretty(&archive(&args.root())?)?);
    } else {
        let output = Path::new(args.required("--machine-json")?);
        write_report(
            &args.root(),
            args.required("--machine-slug")?,
            args.required("--date")?,
            output,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xor_rows_do_not_enter_mul_slice_ranking_or_scores_twice() {
        let rows = [CriterionRow {
            benchmark: "galois_mul_slice_xor_scalar_override_scalar".into(),
            length: Some("len_1048576".into()),
            mean_ns: 10.,
            lower_ns: 9.,
            upper_ns: 11.,
        }];
        let kernels = kernel_rankings(&rows);
        assert!(kernels["galois_mul_slice"].is_empty());
        assert_eq!(kernels["galois_mul_slice_xor"].len(), 1);
        let smoke = OPS.into_iter().map(|op| (op, Vec::new())).collect();
        let (recommendation, _) = recommendations(&smoke, &kernels);
        assert_eq!(recommendation["scored_backends"][0]["score"], 0.5);
    }
}
