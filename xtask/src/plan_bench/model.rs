use crate::{Result, artifact};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

pub(super) const SCHEMA: &str = "plan-bench-functional-v1";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Manifest {
    pub schema: String,
    pub source_commit: String,
    pub source_tree: String,
    pub source_sha256: String,
    pub compiler: String,
    pub features: Vec<String>,
    pub binary_sha256: String,
    pub archive_sha256: String,
    pub catalog_sha256: String,
    pub catalog: Vec<String>,
}
impl Manifest {
    pub fn validate(&self) -> Result<()> {
        if self.schema != SCHEMA
            || self.compiler.trim().is_empty()
            || self.features != ["std", "simd-accel"]
        {
            return Err("unsupported frozen functional benchmark identity".into());
        }
        valid_revision(&self.source_commit)?;
        valid_revision(&self.source_tree)?;
        for hash in [
            &self.source_sha256,
            &self.binary_sha256,
            &self.archive_sha256,
            &self.catalog_sha256,
        ] {
            valid_digest(hash)?;
        }
        if self.catalog.is_empty()
            || self.catalog.len() > 4096
            || self.catalog.windows(2).any(|w| w[0] >= w[1])
        {
            return Err("require a nonempty, unique, sorted bounded catalog".into());
        }
        for key in &self.catalog {
            valid_key(key)?;
        }
        let text = self.catalog.join("\n") + "\n";
        if artifact::snapshot::digest(text.as_bytes()) != self.catalog_sha256 {
            return Err("catalog bytes differ from frozen source contract".into());
        }
        Ok(())
    }
}
pub(super) fn valid_digest(text: &str) -> Result<()> {
    if text.len() != 64
        || !text
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    {
        return Err("require a complete lowercase SHA-256".into());
    }
    Ok(())
}
pub(super) fn valid_revision(text: &str) -> Result<()> {
    if ![40, 64].contains(&text.len())
        || !text.bytes().all(|c| c.is_ascii_hexdigit())
        || text.bytes().all(|c| c == b'0')
    {
        return Err("require a resolved nonzero source revision".into());
    }
    Ok(())
}
pub(super) fn valid_stage(stage: &str) -> Result<()> {
    if !["A1", "B1", "B2", "A2"].contains(&stage) {
        return Err("stage must be A1/B1/B2/A2".into());
    }
    Ok(())
}
fn valid_key(key: &str) -> Result<()> {
    if !key.starts_with("reconstruct_plans/")
        || key.len() > 256
        || key.split('/').any(str::is_empty)
        || !key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-/".contains(&c))
    {
        return Err("invalid plan benchmark key".into());
    }
    Ok(())
}
pub(super) fn catalog(text: &str) -> Result<Vec<String>> {
    let rows: Vec<_> = text.lines().map(str::to_owned).collect();
    if rows.is_empty()
        || rows.len() > 4096
        || rows.windows(2).any(|w| w[0] >= w[1])
        || text != rows.join("\n") + "\n"
    {
        return Err("catalog must be canonical sorted LF lines".into());
    }
    for key in &rows {
        valid_key(key)?;
    }
    Ok(rows)
}
pub(super) fn source_hash(root: &Path) -> Result<String> {
    fn visit(root: &Path, path: &Path, members: &mut Vec<(String, String)>) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                return Err("symlinked build source is unsupported".into());
            }
            if kind.is_dir() {
                visit(root, &entry.path(), members)?;
            } else if kind.is_file() {
                let name = entry
                    .path()
                    .strip_prefix(root)?
                    .to_str()
                    .ok_or("invalid source name")?
                    .replace('\\', "/");
                members.push((name, crate::fingerprint::file(&entry.path())?));
            } else {
                return Err("nonregular build source".into());
            }
        }
        Ok(())
    }
    let mut members = Vec::new();
    visit(root, root, &mut members)?;
    members.sort();
    Ok(artifact::snapshot::digest(&serde_json::to_vec(&members)?))
}
pub(super) fn build_executable(log: &str, source: &Path, target: &Path) -> Result<PathBuf> {
    let source = fs::canonicalize(source)?;
    let target = fs::canonicalize(target)?;
    let mut matches = Vec::new();
    let mut finished = false;
    for line in log.lines().filter(|l| !l.is_empty()) {
        let artifact::StrictJson(value) = serde_json::from_str(line)?;
        match value["reason"].as_str() {
            Some("compiler-artifact")
                if value["target"]["name"] == "reconstruct_plans"
                    && value["target"]["kind"] == serde_json::json!(["bench"]) =>
            {
                let declared_manifest = fs::canonicalize(
                    value["manifest_path"]
                        .as_str()
                        .ok_or("missing build manifest")?,
                )?;
                let declared_source = fs::canonicalize(
                    value["target"]["src_path"]
                        .as_str()
                        .ok_or("missing build source")?,
                )?;
                if declared_manifest != source.join("Cargo.toml")
                    || declared_source != source.join("benches/reconstruct_plans.rs")
                {
                    return Err("build artifact belongs to another source".into());
                }
                let path = fs::canonicalize(
                    value["executable"]
                        .as_str()
                        .ok_or("missing benchmark executable")?,
                )?;
                if !path.starts_with(&target) {
                    return Err("build artifact escaped the owned target directory".into());
                }
                matches.push(path);
            }
            Some("build-finished") => {
                if finished || value["success"] != true {
                    return Err("missing or conflicting build completion".into());
                }
                finished = true;
            }
            _ => {}
        }
    }
    if !finished || matches.len() != 1 {
        return Err("require one completed benchmark build artifact".into());
    }
    Ok(matches.remove(0))
}
pub(super) fn validate_list(text: &str, expected: &[String]) -> Result<()> {
    let keys: Vec<_> = text
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            l.strip_suffix(": benchmark")
                .ok_or("unexpected frozen catalog output")
        })
        .collect::<std::result::Result<_, _>>()?;
    let unique: BTreeSet<_> = keys.iter().copied().collect();
    if unique.len() != keys.len()
        || unique.into_iter().collect::<Vec<_>>()
            != expected.iter().map(String::as_str).collect::<Vec<_>>()
    {
        return Err("binary catalog differs from the frozen source contract".into());
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Outcome {
    pub exit_code: Option<i32>,
    pub reason: String,
    pub reaped: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub schema: String,
    pub stage: String,
    pub mode: String,
    pub manifest_sha256: String,
    pub stdout_sha256: String,
    pub stderr_sha256: String,
    pub outcome: Outcome,
    pub originals_unchanged: bool,
}
#[derive(Serialize)]
pub(crate) struct Check {
    pub accepted: bool,
    pub observed: usize,
    pub errors: Vec<String>,
    pub performance_accepted: bool,
}
pub(super) fn check_output(text: &str, expected: &[String]) -> Check {
    let expected: BTreeSet<_> = expected.iter().map(String::as_str).collect();
    let mut observed = BTreeSet::new();
    let mut pending = None;
    let mut errors = Vec::new();
    for line in text.lines().filter(|l| !l.is_empty()) {
        if let Some(key) = line.strip_prefix("Testing ") {
            if pending.take().is_some() {
                errors.push("case missing a Success outcome".into());
            }
            if !expected.contains(key) {
                errors.push(format!("unexpected case: {key}"));
            }
            pending = Some(key);
        } else if line == "Success" {
            match pending.take() {
                Some(key) => {
                    if !observed.insert(key) {
                        errors.push(format!("duplicate case: {key}"));
                    }
                }
                None => errors.push("orphan Success record".into()),
            }
        } else {
            errors.push("unexpected functional output line".into());
        }
    }
    if pending.is_some() {
        errors.push("truncated case outcome".into());
    }
    if observed != expected {
        errors.push(format!(
            "incomplete catalog: {} of {} successful cases",
            observed.len(),
            expected.len()
        ));
    }
    Check {
        accepted: errors.is_empty(),
        observed: observed.len(),
        errors,
        performance_accepted: false,
    }
}
