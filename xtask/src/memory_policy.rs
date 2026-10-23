//! Read-only THP/mTHP provenance. Fault counts never imply physical page sizes.
use crate::{Args, Result, artifact};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, io::Read, path::Path};

const GLOBAL: &[&str] = &["always", "madvise", "never"];
const PER_SIZE: &[&str] = &["always", "madvise", "never", "inherit"];
const DEFRAG: &[&str] = &["always", "madvise", "never", "defer", "defer+madvise"];
const SHMEM: &[&str] = &["always", "within_size", "advise", "never", "deny", "force"];
const SHMEM_SIZE: &[&str] = &["always", "inherit", "within_size", "advise", "never"];
const MAX_SIZES: usize = 64;

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Policy {
    kernel_release: String,
    process_thp_disable: u8,
    thp: Thp,
}
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Thp {
    enabled: String,
    defrag: String,
    shmem_enabled: Option<String>,
    sizes_kib: BTreeMap<String, SizePolicy>,
}
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct SizePolicy {
    anonymous: Option<String>,
    shmem: Option<String>,
}

fn keys<'a>(value: &'a Value, expected: &[&str]) -> Result<&'a serde_json::Map<String, Value>> {
    value
        .as_object()
        .filter(|map| {
            map.len() == expected.len() && expected.iter().all(|key| map.contains_key(*key))
        })
        .ok_or_else(|| "complete kernel memory policy provenance is required".into())
}

pub(crate) fn parse(value: Value, base_page_size: Option<u64>) -> Result<Policy> {
    // Option fields distinguish an absent kernel interface (explicit null) from
    // missing evidence. Serde's default Option behavior alone cannot do this.
    let outer = keys(&value, &["kernel_release", "process_thp_disable", "thp"])?;
    let thp = keys(
        &outer["thp"],
        &["enabled", "defrag", "shmem_enabled", "sizes_kib"],
    )?;
    let sizes = thp["sizes_kib"]
        .as_object()
        .ok_or("invalid THP/mTHP provenance")?;
    for modes in sizes.values() {
        keys(modes, &["anonymous", "shmem"])?;
    }
    let result: Policy = serde_json::from_value(value)
        .map_err(|error| format!("invalid kernel memory policy: {error}"))?;
    result.validate(base_page_size)?;
    Ok(result)
}

fn size_kib(text: &str) -> Result<u64> {
    if text.is_empty()
        || text.len() > 13
        || text.starts_with('0')
        || !text.bytes().all(|c| c.is_ascii_digit())
    {
        return Err("invalid per-size mTHP size".into());
    }
    let size: u64 = text.parse()?;
    if !size.is_power_of_two() {
        return Err("invalid per-size mTHP size".into());
    }
    Ok(size)
}
fn choice_value(value: Option<&str>, allowed: &[&str], message: &str) -> Result<()> {
    if value.is_some_and(|value| !allowed.contains(&value)) {
        return Err(message.to_owned().into());
    }
    Ok(())
}
impl Policy {
    fn validate(&self, base_page_size: Option<u64>) -> Result<()> {
        if self.kernel_release.is_empty()
            || self.kernel_release.len() > 256
            || !self.kernel_release.bytes().all(|c| (32..=126).contains(&c))
        {
            return Err("invalid recorded Linux kernel release".into());
        }
        if ![0, 1, 3].contains(&self.process_thp_disable) {
            return Err("invalid process THP-disable mode".into());
        }
        if !GLOBAL.contains(&self.thp.enabled.as_str())
            || !DEFRAG.contains(&self.thp.defrag.as_str())
        {
            return Err("missing supported THP/process policy".into());
        }
        if self.thp.sizes_kib.len() > MAX_SIZES {
            return Err("invalid THP/mTHP provenance".into());
        }
        choice_value(
            self.thp.shmem_enabled.as_deref(),
            SHMEM,
            "invalid global shmem THP policy",
        )?;
        for (size, modes) in &self.thp.sizes_kib {
            let size = size_kib(size)?;
            if base_page_size.is_some_and(|base| size * 1024 <= base) {
                return Err("invalid per-size mTHP size".into());
            }
            if modes.anonymous.is_none() && modes.shmem.is_none() {
                return Err("missing per-size mTHP interfaces".into());
            }
            choice_value(
                modes.anonymous.as_deref(),
                PER_SIZE,
                "invalid per-size mTHP policy",
            )?;
            choice_value(
                modes.shmem.as_deref(),
                SHMEM_SIZE,
                "invalid per-size mTHP policy",
            )?;
            if modes.shmem.as_deref() == Some("inherit") && self.thp.shmem_enabled.is_none() {
                return Err("inherited shmem THP policy needs its global control".into());
            }
        }
        Ok(())
    }
}

fn text(path: &Path) -> Result<String> {
    let mut raw = Vec::new();
    std::fs::File::open(path)?
        .take(4097)
        .read_to_end(&mut raw)?;
    if raw.len() > 4096 {
        return Err("oversized kernel memory policy".into());
    }
    if !raw.is_ascii() {
        return Err("non-ASCII kernel memory policy".into());
    }
    Ok(String::from_utf8(raw)?)
}
fn choice(text: &str, allowed: &[&str]) -> Result<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut selected = None;
    for token in text
        .split(|c: char| c.is_ascii_whitespace() || matches!(c, '\x1c'..='\x1f'))
        .filter(|token| !token.is_empty())
    {
        let chosen = token
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'));
        let name = chosen.unwrap_or(token);
        if !allowed.contains(&name) || !seen.insert(name) {
            return Err("invalid kernel memory policy choices".into());
        }
        if chosen.is_some() && selected.replace(name).is_some() {
            return Err("kernel memory policy needs exactly one selected choice".into());
        }
    }
    selected
        .map(str::to_owned)
        .ok_or_else(|| "kernel memory policy needs exactly one selected choice".into())
}
fn optional(path: &Path, allowed: &[&str]) -> Result<Option<String>> {
    match text(path) {
        Ok(value) => Ok(Some(choice(&value, allowed)?)),
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

fn capture_at(
    root: &Path,
    release: String,
    process_mode: impl FnOnce() -> Result<u8>,
) -> Result<Policy> {
    if !std::fs::metadata(root)
        .map_err(|error| {
            format!("observable THP sysfs policy is required for release evidence: {error}")
        })?
        .is_dir()
    {
        return Err("THP sysfs root is not a directory".into());
    }
    let enabled = choice(&text(&root.join("enabled"))?, GLOBAL)?;
    let defrag = choice(&text(&root.join("defrag"))?, DEFRAG)?;
    let mut sizes_kib = BTreeMap::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err("non-UTF-8 THP sysfs entry".into());
        };
        if let Some(suffix) = name.strip_prefix("hugepages-") {
            let size = suffix
                .strip_suffix("kB")
                .ok_or("unknown mTHP size directory")?;
            size_kib(size)?;
            if sizes_kib.len() == MAX_SIZES {
                return Err("too many mTHP sizes".into());
            }
            sizes_kib.insert(
                size.to_owned(),
                SizePolicy {
                    anonymous: optional(&entry.path().join("enabled"), PER_SIZE)?,
                    shmem: optional(&entry.path().join("shmem_enabled"), SHMEM_SIZE)?,
                },
            );
        }
    }
    let result = Policy {
        kernel_release: release,
        process_thp_disable: process_mode()?,
        thp: Thp {
            enabled,
            defrag,
            shmem_enabled: optional(&root.join("shmem_enabled"), SHMEM)?,
            sizes_kib,
        },
    };
    result.validate(None)?;
    Ok(result)
}

fn process_mode() -> Result<u8> {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: PR_GET_THP_DISABLE is a read-only query with no pointer operands.
        // Explicit c_ulong arguments match the variadic prctl ABI on 32/64-bit hosts.
        let result = unsafe {
            libc::prctl(
                libc::PR_GET_THP_DISABLE,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
            )
        };
        if result < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(result.try_into()?)
    }
    #[cfg(not(target_os = "linux"))]
    Err("Linux kernel memory policy is required for release capture".into())
}
pub(crate) fn capture() -> Result<Policy> {
    if !cfg!(target_os = "linux") {
        return Err("Linux kernel memory policy is required for release capture".into());
    }
    let output = std::process::Command::new("uname").arg("-r").output()?;
    if !output.status.success() {
        return Err("cannot read Linux kernel release".into());
    }
    capture_at(
        Path::new("/sys/kernel/mm/transparent_hugepage"),
        String::from_utf8(output.stdout)?.trim_end().to_owned(),
        process_mode,
    )
}
pub(crate) fn run(args: &Args) -> Result<()> {
    use std::io::Write;
    let page = args
        .0
        .get("--base-page-size")
        .map(|text| text.parse())
        .transpose()?;
    let policy = if let Some(path) = args.0.get("--input") {
        parse(artifact::read_json(Path::new(path))?, page)?
    } else {
        let result = capture()?;
        result.validate(page)?;
        result
    };
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    serde_json::to_writer_pretty(&mut out, &policy)?;
    out.write_all(b"\n")?;
    out.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "rse-memory-policy-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            let this = Self(path);
            this.write("enabled", "always [madvise] never\n");
            this.write("defrag", "always defer [defer+madvise] madvise never\n");
            this.write("hugepages-16kB/enabled", "[always] inherit madvise never");
            this.write("hugepages-2048kB/enabled", "always [inherit] madvise never");
            this
        }
        fn write(&self, name: &str, text: &str) {
            let path = self.0.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        fn capture(&self, mode: u8) -> Result<Policy> {
            capture_at(&self.0, "fixture-linux".into(), || Ok(mode))
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn fixture() -> Value {
        json!({"kernel_release":"fixture-linux","process_thp_disable":0,"thp":{"enabled":"madvise","defrag":"defer+madvise","shmem_enabled":null,"sizes_kib":{"16":{"anonymous":"always","shmem":null},"2048":{"anonymous":"inherit","shmem":null}}}})
    }

    #[test]
    fn capture_is_read_only_and_preserves_all_supported_process_modes() {
        let f = Fixture::new();
        let before = std::fs::read(f.0.join("enabled")).unwrap();
        for mode in [0, 1, 3] {
            let mut expected = fixture();
            expected["process_thp_disable"] = mode.into();
            assert_eq!(
                serde_json::to_value(f.capture(mode).unwrap()).unwrap(),
                expected
            );
        }
        assert_eq!(std::fs::read(f.0.join("enabled")).unwrap(), before);
    }
    #[test]
    fn interface_absence_is_explicit_and_shmem_inheritance_requires_a_parent() {
        let f = Fixture::new();
        for size in ["16", "2048"] {
            std::fs::remove_dir_all(f.0.join(format!("hugepages-{size}kB"))).unwrap();
        }
        assert!(f.capture(0).unwrap().thp.sizes_kib.is_empty());
        f.write(
            "hugepages-8kB/shmem_enabled",
            "always inherit within_size advise [never]",
        );
        assert_eq!(
            f.capture(0).unwrap().thp.sizes_kib["8"],
            SizePolicy {
                anonymous: None,
                shmem: Some("never".into())
            }
        );
        f.write("hugepages-8kB/shmem_enabled", "[inherit]");
        assert!(
            f.capture(0)
                .unwrap_err()
                .to_string()
                .contains("global control")
        );
        f.write("shmem_enabled", "[advise]");
        assert!(f.capture(0).is_ok());
    }
    #[test]
    fn ambiguous_non_ascii_oversized_and_unknown_choices_are_never_disabled_defaults() {
        let f = Fixture::new();
        for value in [
            "always madvise never",
            "[always] [madvise] never",
            "always [unknown] never",
            "always [madvise] never extra",
            "always always [madvise]",
        ] {
            f.write("enabled", value);
            assert!(f.capture(0).is_err(), "{value}");
        }
        f.write("enabled", &"a".repeat(4097));
        assert!(f.capture(0).unwrap_err().to_string().contains("oversized"));
        std::fs::write(f.0.join("enabled"), [0xff]).unwrap();
        assert!(f.capture(0).unwrap_err().to_string().contains("ASCII"));
        assert!(capture_at(&f.0.join("missing"), "fixture".into(), || Ok(0)).is_err());
    }
    #[test]
    fn missing_fields_null_interfaces_and_invalid_scalar_types_are_rejected() {
        let original = fixture();
        for pointer in [
            "/kernel_release",
            "/process_thp_disable",
            "/thp",
            "/thp/enabled",
            "/thp/defrag",
            "/thp/shmem_enabled",
            "/thp/sizes_kib",
            "/thp/sizes_kib/16/anonymous",
            "/thp/sizes_kib/16/shmem",
        ] {
            let mut value = original.clone();
            let (parent, name) = pointer.rsplit_once('/').unwrap();
            value
                .pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(name);
            assert!(parse(value, None).is_err(), "{pointer}");
        }
        for (pointer, bad) in [
            ("/kernel_release", json!("\n")),
            ("/process_thp_disable", json!(false)),
            ("/process_thp_disable", json!(2)),
            ("/thp/enabled", Value::Null),
            ("/thp/defrag", json!([])),
            ("/thp/sizes_kib/16", json!(false)),
        ] {
            let mut value = original.clone();
            *value.pointer_mut(pointer).unwrap() = bad;
            assert!(parse(value, None).is_err(), "{pointer}");
        }
        let mut value = original;
        value["thp"]["unknown"] = "never".into();
        assert!(parse(value, None).is_err());
    }
    #[test]
    fn only_bounded_power_of_two_sizes_above_the_base_page_are_valid() {
        assert!(parse(fixture(), Some(16384)).is_err());
        assert!(parse(fixture(), Some(4096)).is_ok());
        for name in [
            "hugepages-24kB",
            "hugepages-16MB",
            "hugepages-016kB",
            "hugepages-0kB",
            "hugepages-10000000000000kB",
        ] {
            let f = Fixture::new();
            f.write(&format!("{name}/enabled"), "[never]");
            assert!(f.capture(0).is_err(), "{name}");
        }
        let f = Fixture::new();
        for shift in 0..63 {
            f.write(&format!("hugepages-{}kB/enabled", 1u64 << shift), "[never]");
        }
        assert!(f.capture(0).is_err());
    }
    #[test]
    fn failed_or_unsupported_process_queries_and_missing_size_controls_are_errors() {
        let f = Fixture::new();
        assert!(f.capture(2).is_err());
        assert!(
            capture_at(&f.0, "fixture".into(), || Err(std::io::Error::from(
                std::io::ErrorKind::PermissionDenied
            )
            .into()))
            .is_err()
        );
        std::fs::remove_file(f.0.join("hugepages-16kB/enabled")).unwrap();
        assert!(f.capture(0).unwrap_err().to_string().contains("interfaces"));
    }
    #[test]
    fn valid_choice_whitespace_is_preserved_but_duplicate_tokens_are_not() {
        for whitespace in [' ', '\t', '\n', '\x1c', '\x1f'] {
            assert_eq!(
                choice(
                    &format!("always{whitespace}[madvise]{whitespace}never"),
                    GLOBAL
                )
                .unwrap(),
                "madvise"
            );
        }
        assert!(choice("[always] always", GLOBAL).is_err());
    }
    #[cfg(not(target_os = "linux"))]
    #[test]
    fn non_linux_cannot_claim_a_linux_kernel_policy() {
        assert!(capture().unwrap_err().to_string().contains("Linux"));
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn process_query_is_read_only_and_stable() {
        let before = process_mode().unwrap();
        assert_eq!(process_mode().unwrap(), before);
        assert!([0, 1, 3].contains(&before));
    }
}
