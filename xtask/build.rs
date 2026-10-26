mod source_stamp;
fn main() {
    println!(
        "cargo:rustc-env=RSE_NATIVE_TARGET={}",
        std::env::var("TARGET").expect("Cargo sets target identity")
    );
    let manifest = std::path::PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets the manifest root"),
    );
    let root = manifest.parent().expect("workspace parent");
    for path in [
        "Cargo.toml",
        "Cargo.lock",
        "xtask/Cargo.toml",
        "xtask/build.rs",
        "xtask/source_stamp.rs",
        "xtask/src",
        "xtask/contracts",
        "scripts/release-sampling",
        "scripts/diagnostics",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(path).display());
    }
    println!(
        "cargo:rustc-env=RSE_NATIVE_SOURCE_SHA256={}",
        source_stamp::digest(root).expect("read native controller sources and assets")
    );
}
