fn main() {
    let backend = codec_new::galois_8::active_backend_name();
    if let Ok(expected) = std::env::var("RSE_EXPECT_BACKEND") {
        assert_eq!(backend, expected);
    }
    println!("Selected backend: {backend}");
    let cases = rse_std_gf16_compat_probe::validate();
    assert_eq!(cases, 1708);
    println!("std cross-version erasure patterns: {cases}");
}
