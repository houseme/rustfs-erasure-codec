//! Admission is exact compiler/host identity, not merely a matching version line.
use super::*;
fn host(identity: &str) -> Result<&str> {
    if !identity
        .lines()
        .next()
        .is_some_and(|v| v.starts_with("rustc "))
    {
        return Err("complete verbose compiler identity required".into());
    }
    let mut hosts = identity
        .lines()
        .filter_map(|line| line.strip_prefix("host: "));
    let value = hosts.next().ok_or("missing frozen compiler host")?;
    if hosts.next().is_some()
        || value.is_empty()
        || !value
            .bytes()
            .all(|v| v.is_ascii_alphanumeric() || b"_-".contains(&v))
    {
        return Err("ambiguous or invalid compiler host".into());
    }
    Ok(value)
}
pub(crate) fn require_compiler(frozen: &str, live: &str, controller_target: &str) -> Result<()> {
    let frozen_host = host(frozen)?;
    let live_host = host(live)?;
    if !live_host.split('-').any(|v| v == "linux") || live_host != controller_target {
        return Err("recorded collection requires the native Linux compiler host and controller target to match".into());
    }
    if frozen_host != live_host || frozen != live {
        return Err(
            "frozen benchmark compiler identity differs from the live native compiler".into(),
        );
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn identity(host: &str) -> String {
        format!(
            "rustc 1.99.1 (fixture)\nbinary: rustc\ncommit-hash: {}\ncommit-date: 2026-10-13\nhost: {host}\nrelease: 1.99.1\nLLVM version: 23.1.1",
            "a".repeat(40)
        )
    }
    #[test]
    fn exact_native_identity_passes_and_cross_host_binaries_are_rejected() {
        let linux = identity("x86_64-unknown-linux-gnu");
        require_compiler(&linux, &linux, "x86_64-unknown-linux-gnu").unwrap();
        let darwin = identity("aarch64-apple-darwin");
        assert!(require_compiler(&darwin, &linux, "x86_64-unknown-linux-gnu").is_err());
        assert!(require_compiler(&linux, &linux, "aarch64-unknown-linux-gnu").is_err());
        assert!(require_compiler(&darwin, &darwin, "aarch64-apple-darwin").is_err());
    }
    #[test]
    fn matching_version_line_cannot_hide_different_commit_or_llvm_identity() {
        let linux = identity("x86_64-unknown-linux-gnu");
        for changed in [
            linux.replace(&"a".repeat(40), &"b".repeat(40)),
            linux.replace("23.1.1", "23.1.2"),
        ] {
            assert_eq!(changed.lines().next(), linux.lines().next());
            assert!(require_compiler(&changed, &linux, "x86_64-unknown-linux-gnu").is_err());
        }
    }
    #[test]
    fn missing_duplicate_and_malformed_host_records_stay_refusals() {
        for value in [
            "rustc 1.99.1".to_owned(),
            format!(
                "{}\nhost: x86_64-unknown-linux-gnu",
                identity("x86_64-unknown-linux-gnu")
            ),
            identity("x86_64 unknown linux"),
        ] {
            assert!(require_compiler(&value, &value, "x86_64-unknown-linux-gnu").is_err());
        }
    }
}
