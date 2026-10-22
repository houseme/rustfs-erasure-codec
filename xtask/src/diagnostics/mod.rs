//! Repository-only source generation. No Python is executed by native tools.
pub(crate) mod catalog;
pub(crate) mod dynamic;
pub(crate) mod ledger;
pub mod leopard;
pub(crate) mod overlays;
mod owned;
pub mod sampler;
pub mod source;
pub mod stream;
mod stream_core;
pub mod templates;
mod verify_scratch;
pub(crate) mod views;

use crate::{Args, Result};

pub(crate) fn check_telemetry(args: &Args) -> Result<()> {
    use std::{
        process::Command,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };
    let root = std::fs::canonicalize(args.root())?;
    let tool = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("missing tool workspace")?;
    let text = templates::telemetry(&root, tool)?;
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancelled);
    ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed))?;
    let mut probe = owned::File::create(
        &root.join("tests/rse_sampler_telemetry_allocations.rs"),
        text,
    )?;
    let mut command = Command::new("cargo");
    command
        .args([
            "test",
            "--locked",
            "--release",
            "--features",
            "std simd-accel",
            "--test",
            "rse_sampler_telemetry_allocations",
            "--",
            "--nocapture",
        ])
        .current_dir(&root)
        .env("RUSTUP_TOOLCHAIN", "stable");
    let outcome = owned::run(command, &cancelled);
    if !outcome.reaped {
        probe.preserve();
        return outcome.result;
    }
    probe.finish()?;
    outcome.result
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn formal_sampler_matches_the_previous_generators_bytes() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap();
        let files = sampler::render(root, root).unwrap();
        let expected = [
            (
                "engine.rs",
                "17c0f1e515cf52fc06125e895359980ead2c28582690d380d805c623c5bbb0bc",
            ),
            (
                "hotspot.rs",
                "07f942ccd0e20160289aa8043921b8fbc6f3fc64167e930bc369326dae8b2718",
            ),
            (
                "layout.rs",
                "01cd488d6394ce0194face373764b92ec497c3c091427c7533c60e3d9ea9ab81",
            ),
            (
                "pattern.rs",
                "e491cb504e47d2ef363e4f19d80c0c184155c9b0cd675e946c46ffd1dcbe70ec",
            ),
            (
                "process.rs",
                "87ab2c0e6409cfd2f601fbb573048ec369dc4b7c3d26dbd1e8421639dc698a66",
            ),
            (
                "root.rs",
                "0b702bbeb4c452fb665c76c1dc005731ab1a2e7573362632803ffc20d254c67d",
            ),
            (
                "service_window.rs",
                "99859216f08517e952fce6e1ccad2d68c34b8310f87bdef01749a48e2ccc092b",
            ),
            (
                "small_file.rs",
                "d22c0676a8257f6570d92d8ec4cb57c3779a13745ece417ea9a67a2b86b756d6",
            ),
            (
                "smoke.rs",
                "b3e3f95bab37b22c108084e56d258878e508dc55ac21f801adc1145d62e04c9b",
            ),
            (
                "stream.rs",
                "0dfef5187ceb740b80142d9b386bf03b37c2b79ab45666234ed8f7fed4ea9e43",
            ),
            (
                "workload.rs",
                "1967c8dabd401ff4468940bd75dc5073a60b35ada7702451717dfb5491dfba2e",
            ),
        ];
        assert_eq!(files.len(), expected.len());
        for (name, hash) in expected {
            let text = files[name].replace(&root.display().to_string(), "<SOURCE>");
            use std::fmt::Write;
            let mut actual = String::with_capacity(64);
            for byte in Sha256::digest(text.as_bytes()) {
                write!(actual, "{byte:02x}").unwrap();
            }
            assert_eq!(actual, hash, "{name}");
            assert!(
                !text.contains("#[global_allocator]"),
                "formal sampler must not contain a counter"
            );
        }
        let probe = templates::telemetry(root, root).unwrap();
        assert!(probe.contains("#[global_allocator]"));
        assert!(probe.contains("mod sampling;"));
    }
}
