/// CPU cache size detection for cache-aware parallel job sizing.
///
/// Detects L2 cache size per core on Linux (via sysfs) and macOS (via sysctl).
/// Falls back to a conservative default of 256 KiB when detection fails.
/// Default L2 cache estimate when detection fails (256 KiB per core).
pub(crate) const DEFAULT_L2_CACHE_BYTES: usize = 256 * 1024;

/// Detect the L2 cache size in bytes.
///
/// Returns `Some(bytes)` on success, `None` if detection fails or is unsupported.
/// The caller should fall back to [`DEFAULT_L2_CACHE_BYTES`].
pub(crate) fn detect_l2_cache_bytes() -> Option<usize> {
    // Hardware discovery can read sysfs or start sysctl. Cache only this value;
    // each policy still applies fresh environment overrides afterwards.
    static L2_CACHE_BYTES: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    *L2_CACHE_BYTES.get_or_init(detect_l2_cache_bytes_uncached)
}

fn detect_l2_cache_bytes_uncached() -> Option<usize> {
    #[cfg(target_os = "linux")]
    {
        detect_l2_cache_linux()
    }
    #[cfg(target_os = "macos")]
    {
        detect_l2_cache_macos()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

/// Linux: read `/sys/devices/system/cpu/cpu0/cache/index2/size`.
#[cfg(target_os = "linux")]
fn detect_l2_cache_linux() -> Option<usize> {
    let content = std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cache/index2/size").ok()?;
    parse_cache_size(&content)
}

/// macOS: query `sysctl -n hw.l2cachesize`.
#[cfg(target_os = "macos")]
fn detect_l2_cache_macos() -> Option<usize> {
    let output = std::process::Command::new("sysctl")
        .arg("-n")
        .arg("hw.l2cachesize")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8(output.stdout).ok()?;
    let val = s.trim().parse::<usize>().ok()?;
    if val > 0 { Some(val) } else { None }
}

/// Parse a cache size string like "32K", "256K", "1M", or a plain number.
fn parse_cache_size(s: &str) -> Option<usize> {
    let s = s.trim();
    if let Some(k) = s.strip_suffix('K') {
        k.parse::<usize>().ok().and_then(|v| v.checked_mul(1024))
    } else if let Some(m) = s.strip_suffix('M') {
        m.parse::<usize>()
            .ok()
            .and_then(|v| v.checked_mul(1024 * 1024))
    } else {
        s.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cache_size_kb() {
        assert_eq!(parse_cache_size("32K"), Some(32 * 1024));
        assert_eq!(parse_cache_size("256K\n"), Some(256 * 1024));
        assert_eq!(parse_cache_size("1024K"), Some(1024 * 1024));
    }

    #[test]
    fn test_parse_cache_size_mb() {
        assert_eq!(parse_cache_size("1M"), Some(1024 * 1024));
        assert_eq!(parse_cache_size("8M"), Some(8 * 1024 * 1024));
    }

    #[test]
    fn test_parse_cache_size_raw() {
        assert_eq!(parse_cache_size("262144"), Some(262144));
        assert_eq!(parse_cache_size("0"), Some(0));
    }

    #[test]
    fn test_parse_cache_size_invalid() {
        assert_eq!(parse_cache_size("abc"), None);
        assert_eq!(parse_cache_size(""), None);
    }

    #[test]
    fn test_detect_returns_something() {
        // On any platform, detection should either return a value or None.
        // We just verify it doesn't panic.
        let _ = detect_l2_cache_bytes();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_detect_linux_returns_value() {
        // On Linux CI, sysfs should be available
        if let Some(bytes) = detect_l2_cache_linux() {
            assert!(bytes >= 1024, "L2 cache too small: {bytes}");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_detect_macos_returns_value() {
        // On macOS, sysctl should be available
        if let Some(bytes) = detect_l2_cache_macos() {
            assert!(bytes >= 1024, "L2 cache too small: {bytes}");
        }
    }
}
