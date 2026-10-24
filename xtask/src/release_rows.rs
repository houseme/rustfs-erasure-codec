//! Source-bound release row validation over the shared normalization model.
use crate::{Result, benchmark_contract as model, sampling_contract};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub profile: String,
    pub rows: Value,
    pub revision: String,
    pub iteration_counts: Option<BTreeMap<String, u64>>,
    pub default_iterations: Option<u64>,
    #[serde(default = "complete")]
    pub complete: bool,
}
fn complete() -> bool {
    true
}
pub(crate) fn metric(profile: &str) -> Result<&'static str> {
    match profile {
        "smoke" => Ok("throughput_mb_s"),
        "small_file" => Ok("ns_per_iter"),
        "stream" => Ok("ns_per_block"),
        "hotspot" => Ok("candidate_mb_s"),
        _ => Err("unknown release profile".into()),
    }
}
/// Persisted artifact names also determine the original summary filenames.
pub(crate) fn artifact_kind(profile: &str) -> Result<&'static str> {
    match profile {
        "smoke" => Ok("smoke-results"),
        "small_file" => Ok("small-file-results"),
        "stream" => Ok("stream-path-results"),
        "hotspot" => Ok("reconstruction-hotspot-results"),
        _ => Err("unknown release profile".into()),
    }
}
impl Request {
    fn raw<K, V>(&self, row: &Value, case: &str, provenance: &BTreeMap<K, Option<V>>) -> Result<()>
    where
        K: std::borrow::Borrow<str> + Ord,
        V: std::ops::Deref<Target = str>,
    {
        if !sampling_contract::known_case(&self.profile, case) {
            return Err("incomplete or unexpected release case catalog".into());
        }
        if row["schema_version"].as_u64() != Some(1) || row["benchmark_metrics_enabled"] != false {
            return Err("release artifacts require schema 1 and disabled instrumentation".into());
        }
        let kind = artifact_kind(&self.profile)?;
        if row["artifact_kind"] != kind {
            return Err("wrong benchmark artifact kind".into());
        }
        if self.profile == "hotspot" {
            if model::CONTRACT
                .hotspot_case_fields
                .iter()
                .any(|key| row.get(key).is_none())
            {
                return Err("missing hotspot workload fields".into());
            }
        } else {
            let revision = row["git_revision"]
                .as_str()
                .filter(|r| {
                    (7..=40).contains(&r.len())
                        && r.bytes()
                            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                })
                .ok_or("benchmark revision does not match the measured source")?;
            if !self.revision.starts_with(revision) {
                return Err("benchmark revision does not match the measured source".into());
            }
            if [
                "target_triple",
                "backend",
                "backend_id",
                "backend_kind",
                "backend_override",
                "override_honored",
                "features",
            ]
            .iter()
            .any(|key| provenance.get(*key).is_none_or(Option::is_none))
            {
                return Err("missing native benchmark provenance".into());
            }
            let profile = if self.profile == "smoke" {
                "extended"
            } else {
                "fast"
            };
            if self.profile != "smoke" && row["profile"] != profile
                || row.get("profile").is_some_and(|p| p != profile)
            {
                return Err("unexpected benchmark profile".into());
            }
            let count = if let Some(counts) = &self.iteration_counts {
                *counts.get(case).ok_or("missing captured iteration count")?
            } else {
                self.default_iterations
                    .ok_or("missing default iteration count")?
            };
            if row["iterations"].as_u64() != Some(count) {
                return Err("benchmark iterations differ from captured settings".into());
            }
            if provenance.get("features").and_then(Option::as_deref) != Some("simd-accel|std") {
                return Err("unexpected features or benchmark profile".into());
            }
        }
        if !row[metric(&self.profile)?]
            .as_f64()
            .is_some_and(|n| n.is_finite() && n > 0.0)
        {
            return Err("release metrics must be finite and positive".into());
        }
        Ok(())
    }
    /// Validate a borrowed worker row without constructing normalized JSON output.
    pub(crate) fn validate_row(&self, row: &Value, expected: &str) -> Result<()> {
        if self.profile == "hotspot" {
            let (case, normalized) = model::hotspot_ref(row, metric(&self.profile)?)?;
            self.raw(row, &case, &normalized.provenance)?;
            if case != expected {
                return Err("raw diagnostic workload mismatch".into());
            }
        } else {
            let normalized = model::normalize_ref(row, false)?;
            let case = normalized.key.label();
            self.raw(row, &case, &normalized.provenance)?;
            if case != expected {
                return Err("raw diagnostic workload mismatch".into());
            }
        }
        Ok(())
    }
    pub(crate) fn records(self) -> Result<Value> {
        Ok(Value::Array(
            self.indexed()?.into_iter().map(|(_, row)| row).collect(),
        ))
    }
    /// Keep validated rows owned until an external JSON response is required.
    pub(crate) fn indexed(mut self) -> Result<Vec<(String, Value)>> {
        metric(&self.profile)?;
        let rows = std::mem::take(&mut self.rows);
        let mut keys = Vec::new();
        // Checked callbacks see original types before normalization changes fields.
        let normalized = if self.profile == "hotspot" {
            model::library_hotspots_checked(
                rows,
                metric(&self.profile)?,
                |row, case, normalized| self.raw(row, case, &normalized.provenance),
            )?
        } else {
            model::library_rows_checked(rows, "release evidence", false, |row, normalized| {
                self.raw(row, &normalized.key.label(), &normalized.provenance)
            })?
        };
        let Value::Array(rows) = normalized else {
            return Err("invalid normalized rows".into());
        };
        for row in rows {
            let case = if self.profile == "hotspot" {
                row["scenario"]
                    .as_str()
                    .ok_or("missing scenario")?
                    .to_owned()
            } else {
                [
                    "operation",
                    "data_shards",
                    "parity_shards",
                    "shard_size",
                    "stream_block_size",
                ]
                .iter()
                .map(|key| row[*key].as_str().unwrap_or(""))
                .collect::<Vec<_>>()
                .join(":")
                .trim_end_matches(':')
                .to_owned()
            };
            keys.push((case, row));
        }
        if self.complete
            && keys.len()
                != sampling_contract::case_catalog()
                    .get(self.profile.as_str())
                    .ok_or("unknown release profile")?
                    .len()
        {
            return Err("incomplete or unexpected release case catalog".into());
        }
        Ok(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn row() -> Value {
        json!({"operation":"encode","data_shards":4,"parity_shards":2,"shard_size":1024,"throughput_mb_s":100.0,"ns_per_iter":1000.0,
        "schema_version":1,"benchmark_metrics_enabled":false,"artifact_kind":"small-file-results","git_revision":"bbbbbbb","iterations":2,"profile":"fast",
        "target_triple":"fixture-target","backend":"scalar","backend_id":"scalar","backend_kind":"scalar","backend_override":"auto","override_honored":true,"features":"simd-accel|std"})
    }
    fn request(rows: Vec<Value>) -> Request {
        Request {
            profile: "small_file".into(),
            rows: rows.into(),
            revision: "b".repeat(40),
            iteration_counts: None,
            default_iterations: Some(2),
            complete: false,
        }
    }
    #[test]
    fn typed_views_keep_original_order_and_json_contracts_without_copying_payloads() {
        for profile in sampling_contract::PROFILES {
            let samples = sampling_contract::calibration_profile::tests::records(profile);
            let rows = samples
                .iter()
                .step_by(12)
                .map(|r| r["rows"][0].clone())
                .collect::<Vec<_>>();
            let make = || Request {
                profile: profile.into(),
                rows: Value::Array(rows.clone()),
                revision: "a".repeat(40),
                iteration_counts: None,
                default_iterations: Some(1),
                complete: true,
            };
            let indexed = make().indexed().unwrap();
            assert_eq!(
                indexed.len(),
                sampling_contract::case_catalog()[profile].len()
            );
            assert_eq!(
                make().records().unwrap(),
                Value::Array(indexed.into_iter().map(|(_, row)| row).collect())
            );
        }
        let mut raw = row();
        raw["retained_payload"] = json!("original payload 雪".repeat(1024));
        let pointer = raw["retained_payload"].as_str().unwrap().as_ptr();
        let result = request(vec![raw]).records().unwrap();
        assert_eq!(
            result[0]["retained_payload"].as_str().unwrap().as_ptr(),
            pointer
        );
    }
    #[test]
    fn original_types_are_checked_before_normalization() {
        request(vec![row()]).indexed().unwrap();
        for (key, value) in [
            ("schema_version", json!("1")),
            ("schema_version", json!(true)),
            ("benchmark_metrics_enabled", json!("false")),
            ("iterations", json!(true)),
            ("ns_per_iter", json!("1000")),
            ("git_revision", json!("ccccccc")),
            ("features", json!("std")),
        ] {
            let mut bad = row();
            bad[key] = value;
            assert!(request(vec![bad]).indexed().is_err(), "{key}");
        }
    }
    #[test]
    fn row_order_extra_fields_and_selected_iteration_counts_are_preserved() {
        let mut first = row();
        first["operation"] = json!("verify");
        first["extra"] = json!({"retained":[true,null,1.25]});
        let second = row();
        let result = request(vec![first.clone(), second]).indexed().unwrap();
        assert_eq!(result[0].0, "verify:4:2:1024");
        assert_eq!(result[1].0, "encode:4:2:1024");
        assert_eq!(result[0].1["extra"], first["extra"]);
        let mut selected = request(vec![row()]);
        selected.iteration_counts = Some(BTreeMap::from([("encode:4:2:1024".into(), 3)]));
        assert!(selected.indexed().is_err());
    }
    #[test]
    fn partial_rows_cannot_certify_the_complete_catalog_or_hide_duplicates() {
        let mut complete = request(vec![row()]);
        complete.complete = true;
        assert!(complete.indexed().is_err());
        assert!(request(vec![row(), row()]).indexed().is_err());
        let mut foreign = row();
        foreign["shard_size"] = json!(12345);
        assert!(request(vec![foreign]).indexed().is_err());
    }
}
