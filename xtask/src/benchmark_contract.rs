//! Shared native model for benchmark and reconstruction regression gates.
use crate::{Result, artifact};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::LazyLock,
};

#[derive(Deserialize)]
pub(crate) struct Contract {
    schema: String,
    pub operations: Vec<String>,
    pub case_key: Vec<String>,
    pub optional_case_key: Vec<String>,
    pub provenance_fields: Vec<String>,
    pub provenance_aliases: BTreeMap<String, Vec<String>>,
    pub strict_provenance_fields: Vec<String>,
    pub stream_provenance_fields: Vec<String>,
    pub thread_fields: Vec<String>,
    pub unknown_provenance: BTreeSet<String>,
    pub default_thresholds: BTreeMap<String, f64>,
    pub hotspot_case_fields: Vec<String>,
}
pub(crate) static CONTRACT: LazyLock<Contract> = LazyLock::new(|| {
    let contract: Contract = serde_json::from_str(include_str!("../contracts/benchmark.json"))
        .expect("checked-in benchmark contract");
    assert_eq!(contract.schema, "rse-benchmark-contract-v1");
    assert_eq!(contract.case_key.len(), 3);
    assert_eq!(contract.optional_case_key.len(), 1);
    assert!(
        contract
            .operations
            .iter()
            .all(|op| contract.default_thresholds.contains_key(op))
    );
    contract
});

pub(crate) fn positive_metric(value: &Value, name: &str) -> Result<f64> {
    let number = match value {
        Value::Number(number) => number.as_f64(),
        Value::String(raw) => raw.trim().parse().ok(),
        _ => None,
    }
    .filter(|v| v.is_finite() && *v > 0.0);
    number.ok_or_else(|| format!("{name} must be a finite positive number").into())
}

fn integer(value: &Value, name: &str, zero: bool) -> Result<String> {
    let raw = match value {
        Value::String(raw) => raw.clone(),
        Value::Number(raw) => raw.to_string(),
        _ => String::new(),
    };
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!(
            "{name} must be a {} integer",
            if zero { "nonnegative" } else { "positive" }
        )
        .into());
    }
    let normalized = raw.trim_start_matches('0');
    if normalized.is_empty() {
        if zero {
            return Ok("0".into());
        }
        return Err(format!("{name} must be a positive integer").into());
    }
    Ok(normalized.to_owned())
}
pub(crate) fn positive_integer(value: &Value, name: &str) -> Result<String> {
    integer(value, name, false)
}

fn normalize_provenance(value: &Value, field: &str) -> Result<Option<String>> {
    if value.is_null() {
        return Ok(None);
    }
    if ["benchmark_metrics_enabled", "override_honored"].contains(&field) {
        let raw = match value {
            Value::Bool(v) => v.to_string(),
            Value::String(s) => s.trim().to_ascii_lowercase(),
            _ => String::new(),
        };
        if !["true", "false"].contains(&raw.as_str()) {
            return Err(format!("{field} must be true or false").into());
        }
        return Ok(Some(raw));
    }
    if CONTRACT.thread_fields.iter().any(|v| v == field) {
        return Ok(Some(integer(value, field, field == "max_parallel_jobs")?));
    }
    if field == "features" {
        let raw = value
            .as_str()
            .ok_or("features must be a pipe-separated string")?;
        return Ok(Some(
            raw.split('|')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join("|"),
        ));
    }
    let mut text = if field == "schema_version" {
        match value {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => if *b { "True" } else { "False" }.into(),
            _ => return Err("schema_version must be a scalar".into()),
        }
    } else {
        value
            .as_str()
            .ok_or_else(|| format!("{field} must be a string"))?
            .to_owned()
    };
    text = text.trim().to_owned();
    if text.is_empty() && field != "rustflags" {
        return Err(format!("{field} must not be empty when present").into());
    }
    if ["compiler", "cpu_model"].contains(&field) {
        text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if field == "compiler" {
            text = text.strip_prefix("rustc ").unwrap_or(&text).to_owned();
        }
    }
    if field != "rustflags"
        && CONTRACT
            .unknown_provenance
            .contains(&text.to_ascii_lowercase())
    {
        Ok(None)
    } else {
        Ok(Some(text))
    }
}

pub(crate) fn provenance(
    record: &serde_json::Map<String, Value>,
) -> Result<BTreeMap<String, Option<String>>> {
    CONTRACT
        .provenance_fields
        .iter()
        .map(|field| Ok((field.clone(), provenance_value(record, field)?)))
        .collect()
}

pub(crate) fn provenance_value(
    record: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<Option<String>> {
    let aliases = CONTRACT.provenance_aliases.get(field);
    let names = aliases
        .into_iter()
        .flatten()
        .map(String::as_str)
        .chain(aliases.is_none().then_some(field));
    let mut values = Vec::new();
    for alias in names {
        if let Some(value) = record.get(alias)
            && let Some(value) = normalize_provenance(value, field)?
        {
            values.push((alias, value));
        }
    }
    if let Some((_, first)) = values.first()
        && values.iter().any(|(_, value)| value != first)
    {
        return Err(format!(
            "conflicting aliases for {field}: {}",
            values
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
                .join(", ")
        )
        .into());
    }
    Ok(values.into_iter().next().map(|(_, value)| value))
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Key(pub [String; 5]);
impl Key {
    pub fn label(&self) -> String {
        self.0.join(":").trim_end_matches(':').to_owned()
    }
}
pub(crate) struct Record {
    pub key: Key,
    pub metrics: BTreeMap<String, f64>,
    pub provenance: BTreeMap<String, Option<String>>,
}

pub(crate) fn load_rows(path: &Path) -> Result<Vec<Value>> {
    let rows = match path.extension().and_then(|s| s.to_str()) {
        Some("json") => {
            let value: Value = artifact::read_json(path)?;
            let Value::Array(rows) = value else {
                return Err("expected a nonempty list of benchmark rows".into());
            };
            rows
        }
        Some("csv") => artifact::csv_rows(path)?
            .into_iter()
            .map(|row| {
                Value::Object(
                    row.into_iter()
                        .map(|(key, value)| (key, value.into()))
                        .collect(),
                )
            })
            .collect(),
        _ => return Err(format!("unsupported file type: {}", path.display()).into()),
    };
    if rows.is_empty() {
        return Err("expected a nonempty list of benchmark rows".into());
    }
    Ok(rows)
}

fn normalize(row: Value, strict: bool) -> Result<Record> {
    normalize_ref(&row, strict)
}

fn normalize_ref(row: &Value, strict: bool) -> Result<Record> {
    let row = row
        .as_object()
        .ok_or("each benchmark row must be an object")?;
    let operation = row
        .get("operation")
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty())
        .ok_or("operation must be a nonempty string")?
        .trim()
        .to_owned();
    let mut key = [
        operation,
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    ];
    for (index, field) in CONTRACT
        .case_key
        .iter()
        .chain(&CONTRACT.optional_case_key)
        .enumerate()
    {
        let value = row.get(field);
        if index >= 3 && value.is_none_or(|v| v == "") {
            continue;
        }
        key[index + 1] = positive_integer(value.unwrap_or(&Value::Null), field)?;
    }
    let mut metrics = BTreeMap::new();
    for field in ["throughput_mb_s", "ns_per_iter", "ns_per_block"] {
        if field == "ns_per_block" && !row.contains_key(field) {
            continue;
        }
        metrics.insert(
            field.to_owned(),
            positive_metric(row.get(field).unwrap_or(&Value::Null), field)?,
        );
    }
    let provenance = provenance(row)?;
    if strict {
        let mut required: Vec<_> = CONTRACT.strict_provenance_fields.iter().collect();
        if key[0].ends_with("_stream")
            || !key[4].is_empty()
            || provenance["artifact_kind"].as_deref() == Some("stream-path-results")
        {
            required.extend(&CONTRACT.stream_provenance_fields);
        }
        let missing: Vec<_> = required
            .into_iter()
            .filter(|name| {
                provenance[*name].as_ref().is_none_or(|v| {
                    v.is_empty()
                        || CONTRACT
                            .unknown_provenance
                            .contains(&v.to_ascii_lowercase())
                            && !(name.as_str() == "features" && v.eq_ignore_ascii_case("none"))
                })
            })
            .map(String::as_str)
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "strict provenance missing or unknown fields: {}",
                missing.join(", ")
            )
            .into());
        }
    }
    Ok(Record {
        key: Key(key),
        metrics,
        provenance,
    })
}

pub(crate) fn index(rows: Vec<Value>, source: &str, strict: bool) -> Result<BTreeMap<Key, Record>> {
    let mut indexed = BTreeMap::new();
    for (number, row) in rows.into_iter().enumerate() {
        let record = normalize(row, strict)
            .map_err(|error| format!("{source}: row {}: {error}", number + 1))?;
        let key = record.key.clone();
        if indexed.insert(key.clone(), record).is_some() {
            return Err(format!("{source}: duplicate case {}", key.label()).into());
        }
    }
    Ok(indexed)
}

pub(crate) struct Hotspot {
    pub case_fields: BTreeMap<String, Value>,
    pub metrics: BTreeMap<String, f64>,
    pub provenance: BTreeMap<String, Option<String>>,
}
pub(crate) fn hotspots(rows: Vec<Value>, metric: &str) -> Result<BTreeMap<String, Hotspot>> {
    let mut indexed = BTreeMap::new();
    for row in rows {
        let (scenario, record) = hotspot(&row, metric)?;
        if indexed.insert(scenario.clone(), record).is_some() {
            return Err(format!("duplicate hotspot scenario: {scenario}").into());
        }
    }
    Ok(indexed)
}

fn hotspot(row: &Value, metric: &str) -> Result<(String, Hotspot)> {
    let row = row
        .as_object()
        .ok_or("each hotspot result must be an object")?;
    let scenario = row
        .get("scenario")
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty())
        .ok_or("scenario must be a nonempty string")?
        .to_owned();
    let provenance = provenance(row)?;
    let mut metrics = BTreeMap::new();
    for field in ["candidate_mb_s", "baseline_mb_s", "speedup"] {
        if field == metric || row.contains_key(field) {
            metrics.insert(
                field.into(),
                positive_metric(row.get(field).unwrap_or(&Value::Null), field)?,
            );
        }
    }
    if !metrics.contains_key(metric) {
        metrics.insert(
            metric.into(),
            positive_metric(row.get(metric).unwrap_or(&Value::Null), metric)?,
        );
    }
    let mut case_fields = BTreeMap::new();
    for field in &CONTRACT.hotspot_case_fields {
        let value = if let Some(value) = row.get(field) {
            if [
                "data_shards",
                "parity_shards",
                "shard_size",
                "useful_shards",
            ]
            .contains(&field.as_str())
            {
                positive_integer(value, field)?.into()
            } else {
                value.clone()
            }
        } else {
            Value::Null
        };
        case_fields.insert(field.clone(), value);
    }
    Ok((
        scenario,
        Hotspot {
            case_fields,
            metrics,
            provenance,
        },
    ))
}

/// Preserve row order, extra fields and the legacy capture's normalized values.
pub(crate) fn library_rows(rows: Value, source: &str, strict: bool) -> Result<Value> {
    let Value::Array(rows) = rows else {
        return Err(format!("{source}: expected a nonempty list of benchmark rows").into());
    };
    if rows.is_empty() {
        return Err(format!("{source}: expected a nonempty list of benchmark rows").into());
    }
    let mut keys = BTreeSet::new();
    let mut output = Vec::with_capacity(rows.len());
    for (index, mut row) in rows.into_iter().enumerate() {
        let record = normalize_ref(&row, strict)
            .map_err(|error| format!("{source}: row {}: {error}", index + 1))?;
        if !keys.insert(record.key.clone()) {
            return Err(format!("{source}: duplicate case {}", record.key.label()).into());
        }
        let row_map = row
            .as_object_mut()
            .ok_or("each benchmark row must be an object")?;
        row_map.insert("operation".into(), record.key.0[0].clone().into());
        for (field, value) in CONTRACT
            .case_key
            .iter()
            .chain(&CONTRACT.optional_case_key)
            .zip(&record.key.0[1..])
        {
            row_map.insert(field.clone(), value.clone().into());
        }
        for (field, value) in record.metrics {
            row_map.insert(field, value.into());
        }
        for (field, value) in record.provenance {
            row_map.insert(field, value.map_or(Value::Null, Value::String));
        }
        output.push(row);
    }
    Ok(output.into())
}

pub(crate) fn library_hotspots(rows: Value, metric: &str) -> Result<Value> {
    let Value::Array(rows) = rows else {
        return Err("expected a nonempty list of hotspot results".into());
    };
    if rows.is_empty() {
        return Err("expected a nonempty list of hotspot results".into());
    }
    let mut keys = BTreeSet::new();
    let mut output = Vec::with_capacity(rows.len());
    for mut row in rows {
        let (scenario, normalized) = hotspot(&row, metric)?;
        if !keys.insert(scenario.clone()) {
            return Err(format!("duplicate hotspot scenario: {scenario}").into());
        }
        let row_map = row
            .as_object_mut()
            .ok_or("each hotspot result must be an object")?;
        for (field, value) in normalized.metrics {
            row_map.insert(field, value.into());
        }
        for (field, value) in normalized.case_fields {
            if row_map.contains_key(&field) {
                row_map.insert(field, value);
            }
        }
        output.push(row);
    }
    Ok(output.into())
}

/// Per-field failures allow a caller to query one field independently; native
/// gates still reject any malformed field when validating a complete row.
pub(crate) fn provenance_outcomes(row: Value) -> Result<Value> {
    let row = row
        .as_object()
        .ok_or("benchmark provenance must be an object")?;
    let mut values = BTreeMap::new();
    let mut errors = BTreeMap::new();
    for field in &CONTRACT.provenance_fields {
        match provenance_value(row, field) {
            Ok(value) => {
                values.insert(field, value);
            }
            Err(error) => {
                errors.insert(field, error.to_string());
            }
        }
    }
    Ok(serde_json::json!({"values":values,"errors":errors}))
}

fn compare_row(profile: &str, left: &Value, right: &Value) -> Result<()> {
    let left = left
        .as_object()
        .ok_or("benchmark provenance must be an object")?;
    let right = right
        .as_object()
        .ok_or("benchmark provenance must be an object")?;
    for field in &CONTRACT.provenance_fields {
        if provenance_value(left, field)? != provenance_value(right, field)? {
            return Err(format!("{profile}: incompatible {field}").into());
        }
    }
    if profile == "hotspot" {
        for field in &CONTRACT.hotspot_case_fields {
            if left.get(field).unwrap_or(&Value::Null) != right.get(field).unwrap_or(&Value::Null) {
                return Err(format!("hotspot: incompatible {field}").into());
            }
        }
    }
    Ok(())
}

pub(crate) fn compatible(
    profile: &str,
    left: &BTreeMap<String, Value>,
    right: &BTreeMap<String, Value>,
) -> Result<()> {
    if left.keys().ne(right.keys()) {
        return Err("complete evidence case sets must match".into());
    }
    for (case, row) in left {
        compare_row(profile, row, &right[case])?;
    }
    Ok(())
}

pub(crate) fn observations(
    profile: &str,
    records: Vec<Value>,
    reference: Option<BTreeMap<String, (Value, Value)>>,
    collect: bool,
) -> Result<Value> {
    let supplied = reference.is_some() && !collect;
    let mut contracts = reference.unwrap_or_default();
    let mut first = Vec::new();
    for (index, record) in records.iter().enumerate() {
        let case = record
            .get("case")
            .and_then(Value::as_str)
            .ok_or("observation case must be a string")?;
        let row = record
            .get("rows")
            .and_then(Value::as_array)
            .and_then(|rows| rows.first())
            .ok_or("observation row is required")?;
        let placement = record
            .get("placement")
            .ok_or("observation placement is required")?;
        if let Some((expected_row, expected_placement)) = contracts.get(case) {
            let scalar = |key, default| {
                record
                    .get(key)
                    .map(|value| {
                        value
                            .as_str()
                            .map_or_else(|| value.to_string(), str::to_owned)
                    })
                    .unwrap_or_else(|| String::from(default))
            };
            let label = format!(
                "{case} {}[{}] batch={}",
                scalar("kind", "observation"),
                scalar("index", "?"),
                scalar("calibration_batch", "0")
            );
            compare_row(profile, expected_row, row).map_err(|error| format!("{label}: {error}"))?;
            if placement != expected_placement {
                return Err(format!(
                    "{profile}/{label}: observation placement differs from calibration/worker"
                )
                .into());
            }
        } else {
            if supplied {
                return Err(format!("{profile}: observation case differs from calibration").into());
            }
            contracts.insert(case.to_owned(), (row.clone(), placement.clone()));
            first.push((case.to_owned(), index));
        }
    }
    Ok(serde_json::to_value(first)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn row() -> Value {
        json!({"operation":"encode","data_shards":4,"parity_shards":2,"shard_size":1024,"throughput_mb_s":100,"ns_per_iter":1000})
    }
    #[test]
    fn capture_rows_preserve_order_extra_fields_and_normalized_aliases() {
        let mut first = row();
        first["backend_name"] = "scalar".into();
        first["data_shards"] = "004".into();
        first["extra"] = json!({"keep":[true,null,"text"]});
        let mut second = first.clone();
        second["shard_size"] = 2048.into();
        let input = json!([first, second]);
        let output = library_rows(input.clone(), "test", false).unwrap();
        assert_eq!(output[0]["backend"], "scalar");
        assert_eq!(output[0]["backend_name"], "scalar");
        assert_eq!(output[0]["data_shards"], "4");
        assert_eq!(output[1]["shard_size"], "2048");
        assert_eq!(output[0]["extra"], input[0]["extra"]);
        assert!(input[0].get("backend").is_none());
        assert!(library_rows(json!([row(), row()]), "test", false).is_err());
    }
    #[test]
    fn observation_batches_collect_first_references_and_reject_unknown_calibration_cases() {
        let row = serde_json::json!({"case":"a","kind":"sample","index":3,"rows":[{"backend":"scalar"}],"placement":{"cpu":2}});
        assert_eq!(
            observations("smoke", vec![row.clone(), row.clone()], None, true).unwrap(),
            serde_json::json!([["a", 0]])
        );
        let reference = BTreeMap::from([(
            "a".into(),
            (row["rows"][0].clone(), row["placement"].clone()),
        )]);
        let mut new_case = row.clone();
        new_case["case"] = "b".into();
        assert!(
            observations(
                "smoke",
                vec![new_case.clone()],
                Some(reference.clone()),
                false
            )
            .is_err()
        );
        assert_eq!(
            observations("smoke", vec![new_case], Some(reference.clone()), true).unwrap(),
            serde_json::json!([["b", 0]])
        );
        let mut changed = row.clone();
        changed["rows"][0]["backend_id"] = "changed".into();
        assert!(
            observations("smoke", vec![changed], Some(reference.clone()), false)
                .unwrap_err()
                .to_string()
                .contains("a sample[3] batch=0: smoke: incompatible backend_id")
        );
        let mut changed = row;
        changed["placement"]["cpu"] = 3.into();
        assert!(
            observations("smoke", vec![changed], Some(reference), false)
                .unwrap_err()
                .to_string()
                .contains("observation placement")
        );
    }
    #[test]
    fn requested_provenance_fields_are_independent_of_other_invalid_fields() {
        let result = provenance_outcomes(
            json!({"backend_name":"scalar","compiler":[],"cpu":"one","cpu_model":"two"}),
        )
        .unwrap();
        assert_eq!(result["values"]["backend"], "scalar");
        assert!(
            result["errors"]["compiler"]
                .as_str()
                .unwrap()
                .contains("compiler")
        );
        assert!(
            result["errors"]["cpu_model"]
                .as_str()
                .unwrap()
                .contains("conflicting aliases")
        );
    }
    #[test]
    fn capture_hotspots_keep_raw_provenance_and_only_normalize_existing_case_fields() {
        let original = json!([{"scenario":"case","candidate_mb_s":"10.5","baseline_mb_s":5,"speedup":2.1,"data_shards":4,"schema_version":1,"cpu":"  Example  CPU  "}]);
        let output = library_hotspots(original.clone(), "candidate_mb_s").unwrap();
        assert_eq!(output[0]["schema_version"], 1);
        assert_eq!(output[0]["cpu"], original[0]["cpu"]);
        assert_eq!(output[0]["data_shards"], "4");
        assert_eq!(output[0]["candidate_mb_s"], 10.5);
        assert!(output[0].get("missing_pattern").is_none());
    }
    fn strict() -> Value {
        let mut r = row();
        for (k,v) in json!({"target_triple":"x86_64-unknown-linux-gnu","features":"std|simd-accel","benchmark_metrics_enabled":false,"backend":"rust-avx2","cpu_model":"Example CPU 16C","compiler":"rustc 1.98.1 (012345abc 2026-09-10)","build_profile":"release","rayon_threads":4,"available_parallelism":16,"max_parallel_jobs":2}).as_object().unwrap() {r[k]=v.clone();}
        r
    }
    #[test]
    fn strict_aliases_and_thread_budgets_normalize_without_weakening_conflicts() {
        let canonical = normalize(strict(), true).unwrap();
        for (name, aliases) in &CONTRACT.provenance_aliases {
            for alias in aliases {
                let mut alternate = strict();
                let value = alternate.as_object_mut().unwrap().remove(name).unwrap();
                alternate[alias] = value;
                let row = normalize(alternate, true).unwrap();
                assert_eq!(row.provenance, canonical.provenance, "{alias}");
            }
        }
        let mut alternate = strict();
        alternate["features"] = " simd-accel |std|std ".into();
        alternate["rayon_threads"] = "004".into();
        alternate["max_parallel_jobs"] = "02".into();
        assert_eq!(
            normalize(alternate, true).unwrap().provenance,
            canonical.provenance
        );
        let mut conflict = strict();
        conflict["cpu"] = "another CPU".into();
        assert!(normalize(conflict, true).is_err());
    }
    #[test]
    fn malformed_fields_and_unmatched_extra_rows_are_all_rejected() {
        for field in CONTRACT
            .case_key
            .iter()
            .chain(&CONTRACT.optional_case_key)
            .chain(&CONTRACT.thread_fields)
        {
            for bad in [
                json!(true),
                json!(-1),
                json!(1.5),
                json!("nan"),
                json!(" 4 "),
                Value::Null,
            ] {
                // Explicit null is missing optional provenance in legacy rows.
                if bad.is_null() && CONTRACT.thread_fields.contains(field) {
                    continue;
                }
                let mut r = row();
                r[field] = bad;
                assert!(normalize(r, false).is_err(), "{field}");
            }
        }
        for field in ["throughput_mb_s", "ns_per_iter", "ns_per_block"] {
            for bad in [json!(true), json!(0), json!(-1), json!("inf"), Value::Null] {
                let mut r = row();
                r[field] = bad;
                assert!(normalize(r, false).is_err());
            }
        }
        let mut extra = strict();
        extra["shard_size"] = 2048.into();
        extra.as_object_mut().unwrap().remove("cpu_model");
        assert!(index(vec![strict(), extra], "current", true).is_err());
        assert!(index(vec![row(), row()], "fixture", false).is_err());
    }
    #[test]
    fn strict_unknowns_and_stream_provenance_cannot_be_waived() {
        for field in &CONTRACT.strict_provenance_fields {
            let mut missing = strict();
            missing.as_object_mut().unwrap().remove(field);
            assert!(normalize(missing, true).is_err(), "{field}");
        }
        for features in ["none", "None", "std|simd-accel"] {
            let mut r = strict();
            r["features"] = features.into();
            assert!(normalize(r, true).is_ok());
        }
        for features in ["", " | ", "unknown"] {
            let mut r = strict();
            r["features"] = features.into();
            assert!(normalize(r, true).is_err());
        }
        let mut r = strict();
        r["operation"] = "encode_stream".into();
        r["stream_block_size"] = 4096.into();
        assert!(normalize(r.clone(), true).is_err());
        r["stream_io_backend"] = "memory".into();
        r["stream_io_mode"] = "serial".into();
        assert!(normalize(r, true).is_ok());
    }
}
