#[cfg(test)]
use rustfs_erasure_codec::galois_8::active_backend_name;

#[cfg(test)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ExpectedBackend<'a> {
    Auto,
    Name(&'a str),
    Unknown,
}

#[cfg(test)]
fn expected_backend_name(override_value: &str) -> ExpectedBackend<'_> {
    match override_value {
        "auto" => ExpectedBackend::Auto,
        "scalar" | "scalar-rust" => ExpectedBackend::Name("scalar-rust"),
        "rust-neon" => ExpectedBackend::Name("rust-neon"),
        "rust-ssse3" => ExpectedBackend::Name("rust-ssse3"),
        "rust-avx2" => ExpectedBackend::Name("rust-avx2"),
        "rust-avx512" => ExpectedBackend::Name("rust-avx512"),
        "rust-gfni-avx2" => ExpectedBackend::Name("rust-gfni-avx2"),
        "rust-gfni-avx512" => ExpectedBackend::Name("rust-gfni-avx512"),
        "rust-vsx" => ExpectedBackend::Name("rust-vsx"),
        _ => ExpectedBackend::Unknown,
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub fn override_honored() -> bool {
    let override_value =
        std::env::var("RSE_BACKEND_OVERRIDE").unwrap_or_else(|_| "auto".to_string());
    match expected_backend_name(override_value.trim()) {
        ExpectedBackend::Auto => true,
        ExpectedBackend::Name(expected) => active_backend_name() == expected,
        ExpectedBackend::Unknown => false,
    }
}

#[cfg(test)]
pub fn assert_backend_override_honored_if_strict() {
    if std::env::var_os("RSE_STRICT_BACKEND_OVERRIDE").is_none() {
        return;
    }

    let override_value =
        std::env::var("RSE_BACKEND_OVERRIDE").unwrap_or_else(|_| "auto".to_string());
    match expected_backend_name(override_value.trim()) {
        ExpectedBackend::Auto => {}
        ExpectedBackend::Name(expected) => {
            let actual = active_backend_name();
            assert_eq!(
                expected, actual,
                "requested backend override '{}' was not honored; actual backend was '{}'",
                override_value, actual
            );
        }
        ExpectedBackend::Unknown => {
            panic!(
                "requested backend override '{}' is not a recognised backend",
                override_value
            );
        }
    }
}
