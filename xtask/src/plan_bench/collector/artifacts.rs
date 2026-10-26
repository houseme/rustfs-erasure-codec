use super::*;
/// Polling checks preserve every entry/type/byte constraint without retaining
/// relative names. Directory paths are needed only for the traversal stack.
#[derive(Default)]
pub(super) struct Scan {
    pub entries: usize,
    pub files: usize,
    pub bytes: u64,
}
#[derive(Clone, Copy)]
struct Limits {
    member: u64,
    total: u64,
    depth: usize,
}
const LIMITS: Limits = Limits {
    member: artifact::snapshot::MEMBER_LIMIT,
    total: 1024 * 1024 * 1024,
    depth: 32,
};
fn scan(
    root: &Path,
    max_entries: usize,
    limits: Limits,
    mut file: impl FnMut(&str),
) -> Result<Scan> {
    let root_meta = fs::symlink_metadata(root)?;
    if root_meta.file_type().is_symlink() || !root_meta.is_dir() {
        return Err("regular worker artifact root required".into());
    }
    let mut pending = vec![(root.to_owned(), 0)];
    let mut result = Scan::default();
    while let Some((dir, depth)) = pending.pop() {
        if depth > limits.depth {
            return Err("worker artifact directory nesting exceeded".into());
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            result.entries += 1;
            if result.entries > max_entries {
                return Err("worker artifact entry budget exceeded".into());
            }
            let path = entry.path();
            let meta = fs::symlink_metadata(&path)?;
            if meta.is_dir() {
                pending.push((path, depth + 1));
            } else if meta.is_file() {
                result.files += 1;
                result.bytes = result
                    .bytes
                    .checked_add(meta.len())
                    .ok_or("worker artifact size overflow")?;
                if meta.len() > limits.member || result.bytes > limits.total {
                    return Err("worker artifact byte budget exceeded".into());
                }
                // Validate all names, including ignored sidecars; narrowing the
                // retained catalog must not narrow original acceptance rules.
                file(
                    path.strip_prefix(root)?
                        .to_str()
                        .ok_or("non-UTF8 artifact name")?,
                );
            } else {
                return Err(
                    "regular worker artifacts required; symlink or special member refused".into(),
                );
            }
        }
    }
    Ok(result)
}
pub(super) fn check(root: &Path, max_entries: usize) -> Result<Scan> {
    scan(root, max_entries, LIMITS, |_| {})
}
pub(super) fn metadata(root: &Path, max_entries: usize) -> Result<Vec<String>> {
    let mut files = Vec::new();
    scan(root, max_entries, LIMITS, |name| {
        let normalized = if name.contains('\\') {
            std::borrow::Cow::Owned(name.replace('\\', "/"))
        } else {
            std::borrow::Cow::Borrowed(name)
        };
        if normalized == "new/benchmark.json" || normalized.ends_with("/new/benchmark.json") {
            files.push(normalized.into_owned());
        }
    })?;
    files.sort();
    Ok(files)
}
pub(super) struct Produced {
    pub cases: Vec<Value>,
    pub copies: Vec<ArtifactCopy>,
    pub valid: bool,
    pub means: Vec<Option<f64>>,
    pub error: Option<String>,
}
pub(super) fn collect(
    root: &Path,
    stage: &Path,
    output: &Path,
    manifest: &Manifest,
    plan: &Plan,
) -> Result<Produced> {
    let raw = stage.join("raw");
    let files = metadata(&raw, manifest.catalog.len() * 64 + 128)?;
    let mut snapshot = Snapshot::new(&raw)?;
    let mut seen = BTreeSet::new();
    let mut result:Vec<_>=(0..manifest.catalog.len()).map(|i|json!({"case_index":i,"benchmark_sha256":null,"sample_sha256":null,"estimates_sha256":null})).collect();
    let mut errors = Vec::new();
    let mut copies = Vec::new();
    let mut means = vec![None; manifest.catalog.len()];
    for name in &files {
        let bytes = snapshot.original(name)?;
        let artifact::StrictJson(value) = serde_json::from_slice(&bytes)?;
        let key = value["full_id"]
            .as_str()
            .ok_or("missing raw benchmark identity")?;
        let index = manifest
            .catalog
            .binary_search_by(|v| v.as_str().cmp(key))
            .map_err(|_| "unknown raw benchmark identity")?;
        if !seen.insert(index) {
            return Err("duplicate raw benchmark identity".into());
        }
        let directory = output.join(format!("{index:04}"));
        fs::create_dir_all(&directory)?;
        write(&directory.join("benchmark.json"), &bytes)?;
        result[index]["benchmark_sha256"] = digest(&bytes).into();
        copies.push(ArtifactCopy {
            canonical: directory
                .join("benchmark.json")
                .strip_prefix(root)?
                .to_str()
                .ok_or("invalid canonical path")?
                .into(),
            original: raw
                .join(name)
                .strip_prefix(root)?
                .to_str()
                .ok_or("invalid raw path")?
                .into(),
            sha256: digest(&bytes),
        });
        let prefix = name
            .strip_suffix("benchmark.json")
            .ok_or("invalid metadata member")?;
        let mut values = Vec::new();
        for (member, field) in [
            ("sample.json", "sample_sha256"),
            ("estimates.json", "estimates_sha256"),
        ] {
            match snapshot.original(&format!("{prefix}{member}")) {
                Ok(bytes) => {
                    write(&directory.join(member), &bytes)?;
                    result[index][field] = digest(&bytes).into();
                    copies.push(ArtifactCopy {
                        canonical: directory
                            .join(member)
                            .strip_prefix(root)?
                            .to_str()
                            .ok_or("invalid canonical path")?
                            .into(),
                        original: raw
                            .join(format!("{prefix}{member}"))
                            .strip_prefix(root)?
                            .to_str()
                            .ok_or("invalid raw path")?
                            .into(),
                        sha256: digest(&bytes),
                    });
                    let artifact::StrictJson(value) = serde_json::from_slice(&bytes)?;
                    values.push(value);
                }
                Err(e) => errors.push(e.to_string()),
            }
        }
        if values.len() == 2 {
            if values[0]["times"].as_array().map(Vec::len) != Some(plan.minimum_samples) {
                errors.push("sample count differs from the predeclared plan".into());
            }
            let estimates = values.pop().ok_or("missing estimate")?;
            let sample = values.pop().ok_or("missing sample")?;
            match samples::validate_raw(
                sample,
                estimates,
                plan.minimum_samples,
                plan.confidence_level,
                plan.max_ci_width,
            ) {
                Ok(mean) => means[index] = Some(mean),
                Err(e) => errors.push(format!("{key}: {e}")),
            }
        }
    }
    snapshot.finish()?;
    Ok(Produced {
        cases: result,
        copies,
        valid: seen.len() == manifest.catalog.len() && errors.is_empty(),
        means,
        error: if seen.len() != manifest.catalog.len() {
            Some("incomplete original case catalog".into())
        } else {
            errors.into_iter().next()
        },
    })
}

#[cfg(test)]
mod tests;
