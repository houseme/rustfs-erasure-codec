use crate::{Args, Result, abba};
use serde::Serialize;
use std::{fs, io::Write, path::Path, time::UNIX_EPOCH};

#[derive(Serialize)]
struct Record<'a> {
    stage: &'a str,
    commit: &'a str,
    backend_override: &'a str,
    criterion_group: String,
    operation: &'a str,
    length_bytes: u64,
    mean_ns: f64,
    mean_lower_ns: f64,
    mean_upper_ns: f64,
}

fn directories(path: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            entries.push(entry.path());
        }
    }
    entries.sort();
    Ok(entries)
}

pub(crate) fn extract(args: &Args) -> Result<()> {
    let csv_path = Path::new(args.required("--csv")?);
    let jsonl_path = Path::new(args.required("--jsonl")?);
    if csv_path == jsonl_path {
        return Err("CSV and JSONL outputs must be different files".into());
    }
    let stage = args.required("--stage")?;
    if !abba::STAGES.contains(&stage) {
        return Err("expected A1/B1/B2/A2 stage".into());
    }
    let commit = args.required("--commit")?;
    let backend = args.required("--backend-override")?;
    let updated_after: f64 = args.required("--updated-after-epoch")?.parse()?;
    if !updated_after.is_finite() || updated_after < 0.0 {
        return Err("updated-after-epoch must be finite and nonnegative".into());
    }
    let mut records = Vec::new();
    for group in directories(Path::new(args.get("--criterion-root", "target/criterion")))? {
        let name = group
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or("non-UTF8 Criterion group")?;
        let Some((operation, _)) = abba::operation(name) else {
            continue;
        };
        for length in directories(&group)? {
            let Some(length_bytes) = length
                .file_name()
                .and_then(|v| v.to_str())
                .and_then(|v| v.strip_prefix("len_"))
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|v| *v > 0)
            else {
                continue;
            };
            let path = length.join("new/estimates.json");
            if !path.is_file() {
                continue;
            }
            if path
                .metadata()?
                .modified()?
                .duration_since(UNIX_EPOCH)?
                .as_secs_f64()
                < updated_after
            {
                continue;
            }
            let [mean, lower, upper] = crate::artifact::criterion_mean(&path)?;
            records.push(Record {
                stage,
                commit,
                backend_override: backend,
                criterion_group: name.to_owned(),
                operation,
                length_bytes,
                mean_ns: mean,
                mean_lower_ns: lower,
                mean_upper_ns: upper,
            });
        }
    }
    if records.is_empty() {
        return Err("no galois_backend Criterion estimates found".into());
    }
    // Validate the complete batch before appending to either artifact.
    let mut csv = csv::WriterBuilder::new()
        .has_headers(args.0.contains_key("--write-header"))
        .from_writer(Vec::new());
    let mut jsonl = Vec::new();
    for record in &records {
        csv.serialize(record)?;
        serde_json::to_writer(&mut jsonl, record)?;
        jsonl.push(b'\n');
    }
    let csv = csv.into_inner()?;
    for (path, bytes) in [(csv_path, csv), (jsonl_path, jsonl)] {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?
            .write_all(&bytes)?;
    }
    println!(
        "extracted {} galois_backend estimates for {stage}",
        records.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xor_operation_is_not_mistaken_for_mul_slice() {
        assert_eq!(
            abba::operation("galois_mul_slice_xor_neon_override_auto"),
            Some(("mul_slice_xor", "neon_override_auto"))
        );
        assert_eq!(
            abba::operation("galois_mul_slice_neon_override_auto"),
            Some(("mul_slice", "neon_override_auto"))
        );
        assert_eq!(abba::operation("unrelated"), None);
    }
}
