fn main() {
    let cases = rse_wasm_gf16_compat_probe::validate();
    assert_eq!(cases, 1708);
    println!(
        "Cross-version erasure patterns: {cases}; x86_avx2_target={}",
        cfg!(all(target_arch = "x86_64", target_feature = "avx2"))
    );
}
