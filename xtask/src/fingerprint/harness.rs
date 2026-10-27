//! Original harness identity, independent of the repository tool implementation.
use super::{Digest, Path, Result, Sha256, artifact, update_file};
use crate::diagnostics::source;
const FILES: [&str; 5] = [
    "tests/benchmark_smoke.rs",
    "tests/benchmark_small_files.rs",
    "tests/benchmark_stream_paths.rs",
    "tests/common/mod.rs",
    "benches/common/mod.rs",
];
const DECLARATIONS: [&str; 11] = [
    "struct ReconstructionHotspotBenchResult",
    "fn benchmark_test_iterations",
    "macro_rules! make_random_shards",
    "pub fn fill_random",
    "fn option_shards_with_missing",
    "fn shard_index_pattern",
    "fn assert_required_reconstruction_matches",
    "fn bench_reconstruct_data_hotspot",
    "fn bench_reconstruct_some_hotspot",
    "fn write_reconstruction_hotspot_bench_results",
    "fn benchmark_reconstruction_hotspots",
];
pub(super) fn digest(root: &Path) -> Result<String> {
    let mut hash = Sha256::new();
    let mut buffer = Vec::new();
    for name in FILES {
        hash.update(name.as_bytes());
        hash.update([0]);
        update_file(&root.join(name), &mut hash, &mut buffer)?;
        hash.update([0]);
    }
    let text = source::read(root.join("src/tests/mod.rs"))?;
    for name in DECLARATIONS {
        hash.update(source::block(&text, name)?.as_bytes());
        hash.update([0]);
    }
    Ok(artifact::snapshot::hex(hash.finalize().as_slice()))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::snapshot::tests::Directory;
    use std::fs;
    #[test]
    fn original_harness_bytes_and_first_block_selection_are_fresh_and_newline_compatible() {
        let directory = Directory::new();
        for (index, name) in FILES.into_iter().enumerate() {
            let path = directory.0.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, format!("raw{index}\r\n")).unwrap();
        }
        let text = DECLARATIONS
            .iter()
            .map(|name| format!("{name} {{\n    // 雪\n}}\n"))
            .collect::<String>();
        let path = directory.0.join("src/tests/mod.rs");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        for newline in ["\n", "\r\n", "\r"] {
            fs::write(&path, text.replace('\n', newline)).unwrap();
            assert_eq!(
                digest(&directory.0).unwrap(),
                "4a690790ac010ca1a5aae7c65d70e3cf74e71770afcfea7d98ae5dabb63771ae"
            );
        }
        fs::write(
            &path,
            format!(
                "unselected prefix\n{text}{} {{\n    // duplicate ignored\n}}\n",
                DECLARATIONS[0]
            ),
        )
        .unwrap();
        assert_eq!(
            digest(&directory.0).unwrap(),
            "4a690790ac010ca1a5aae7c65d70e3cf74e71770afcfea7d98ae5dabb63771ae"
        );
        fs::write(&path, text.replace("// 雪", "// changed")).unwrap();
        assert_ne!(
            digest(&directory.0).unwrap(),
            "4a690790ac010ca1a5aae7c65d70e3cf74e71770afcfea7d98ae5dabb63771ae"
        );
        fs::write(&path, &text).unwrap();
        fs::write(directory.0.join(FILES[0]), "raw0\n").unwrap();
        assert_ne!(
            digest(&directory.0).unwrap(),
            "4a690790ac010ca1a5aae7c65d70e3cf74e71770afcfea7d98ae5dabb63771ae"
        );
        fs::write(&path, "missing source blocks").unwrap();
        assert!(digest(&directory.0).is_err());
    }
}
