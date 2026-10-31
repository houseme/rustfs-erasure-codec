//! Pure renderers; file/process ownership belongs to the calling controller.
use super::{catalog, dynamic, sampler, source, templates};
use crate::Result;
use std::{collections::BTreeMap, path::Path};

type Files = BTreeMap<String, String>;

fn verify_engine(text: &str) -> Result<String> {
    let prefix = "rustfs_erasure_codec::__rse_verify_scratch::";
    let mut edit = source::Edits::new(text);
    edit.before(
        "            ITERATIONS.with(|n| n.set(count));",
        &format!("            {prefix}observation(index);\n"),
    )?;
    edit.before("    let (observations, outcome) =\n        measure_fixed(key, count, &mut operation, &ns_per_iter, placement);", &format!("    {prefix}start(page_size);\n"))?;
    let marker =
        "    if let Err(error) = outcome {\n        std::panic::resume_unwind(error);\n    }";
    edit.replace(marker, &format!("    let diagnostic_ok = {prefix}export();\n{marker}\n    assert!(diagnostic_ok, \"verify scratch observation failed; raw rows retained\");"), 1)?;
    edit.finish()
}

fn views(files: &mut Files, tool: &Path) -> Result<()> {
    let text = &files["stream.rs"];
    let kernel = source::block(text, "fn execute_release_kernel")?;
    let mut changed = kernel.to_owned();
    for &(pattern, operation) in catalog::STREAM_KERNELS {
        let original = sampler::original_loop(kernel, pattern)?;
        let mut edit = source::Edits::new(original);
        edit.after(
            "for _ in 0..iterations {",
            "\n                super::stream_views::call();",
        )?;
        if operation != "reconstruct_stream" {
            edit.replace("let mut readers: Vec<&[u8]> = original.iter().map(Vec::as_slice).collect();", "let mut readers: Vec<_> = original.iter().map(|v| super::stream_views::Reader(v.as_slice())).collect();\n                super::stream_views::readers(&readers);", 1)?;
            if operation == "encode_stream" {
                edit.replace(
                    "reserved_stream_outputs(case.parity_shards, case.shard_size)",
                    "super::stream_views::outputs(case.parity_shards, case.shard_size)",
                    1,
                )?;
                edit.before(
                    "                rs.encode_stream(",
                    "                super::stream_views::writers(&writers);\n",
                )?;
                edit.after(
                    "                .unwrap();",
                    "\n                super::stream_views::after_encode();",
                )?;
            }
        } else {
            edit.before(
                "                cursors[0] = Cursor::new(Vec::new());",
                "                super::stream_views::cursors(&cursors, 6);\n",
            )?;
            edit.after(
                "                assert_eq!(cursors[0].get_ref(), &original[0]);",
                "\n                super::stream_views::cursors(&cursors, 7);",
            )?;
            edit.before(
                "                rs.reconstruct_stream(",
                "                super::stream_views::cursors(&cursors, 8);\n",
            )?;
        }
        let drops = match operation {
            "encode_stream" => "super::stream_views::drop_outputs(writers); drop(readers);",
            "verify_stream" => "drop(readers);",
            _ => "drop(cursors);",
        };
        edit.before(
            "\n            }",
            &format!("\n                {drops}\n                super::stream_views::end_call();"),
        )?;
        changed = source::replace_once(
            &changed,
            original,
            &edit.finish()?,
            "stream observation loop changed",
        )?;
    }
    let text = source::replace_once(text, kernel, &changed, "stream observation kernel changed")?;
    let marker = "        let elapsed = execute_release_kernel(self, iterations);";
    let text = source::replace_once(&text, marker, &format!("        super::stream_views::begin(&self.source, iterations, self.fixture.page_size());\n{marker}\n        super::stream_views::finish();"), "stream preparation changed")?
        .replace("\"Vec<&[u8]> -> Vec<Vec<u8>>\"", "\"Vec<stream_views::Reader<'_>> -> Vec<stream_views::Writer>\"")
        .replace("\"Vec<&[u8]>\"", "\"Vec<stream_views::Reader<'_>>\"");
    files.insert("stream.rs".into(), text);
    files.insert("stream_views.rs".into(), templates::native("views", tool)?);
    let marker = "fn release_controlled_profile() {";
    let mut main = source::replace_once(
        &files["root.rs"],
        marker,
        &(marker.to_owned() + " stream_views::enable();"),
        "controlled worker entry changed",
    )?;
    main += "\n#[path=\"rse_release_sampling/stream_views.rs\"] mod stream_views;\n";
    files.insert("root.rs".into(), main);
    Ok(())
}

fn shard_slot(files: &mut Files, tool: &Path) -> Result<()> {
    let text = &files["small_file.rs"];
    let kernel = source::block(text, "fn execute_release_kernel")?;
    let original = sampler::original_loop(kernel, "SmallFileOp::ReconstructShardSlot")?;
    for marker in [
        "let mut shards = shards_to_slots(original);",
        "mark_missing_slots(&mut shards, &[0, case.data_shards]);",
        "rs.reconstruct(&mut shards).unwrap();",
    ] {
        if original.matches(marker).count() != 1 {
            return Err("ShardSlot workload changed; review the diagnostic adapter".into());
        }
    }
    let mut edit = source::Edits::new(original);
    edit.before(
        "                let mut shards = shards_to_slots(original);",
        "                let clone_start = Instant::now();\n",
    )?;
    let marker = "                rs.reconstruct(&mut shards).unwrap();";
    edit.replace(marker, &format!("                let clone_ns = clone_start.elapsed().as_nanos();\n                trace.before(&shards);\n                let reconstruct_start = Instant::now();\n{marker}\n                let reconstruct_ns = reconstruct_start.elapsed().as_nanos();\n                trace.after(&shards, original);\n                let drop_start = Instant::now();\n                drop(shards);\n                trace.phases(clone_ns, reconstruct_ns, drop_start.elapsed().as_nanos());"), 1)?;
    let changed = format!(
        "            let mut trace = super::slot_trace::Trace::for_process(original, iterations);\n{}\n            trace.finish();",
        edit.finish()?
    );
    let changed = source::replace_once(kernel, original, &changed, "ShardSlot loop changed")?;
    let text = source::replace_once(text, kernel, &changed, "ShardSlot kernel changed")?;
    files.insert("small_file.rs".into(), text);
    files.insert(
        "slot_trace.rs".into(),
        source::read(tool.join("scripts/diagnostics/shard_slot.rs"))?,
    );
    files
        .get_mut("root.rs")
        .ok_or("missing sampler entry")?
        .push_str("\n#[path=\"rse_release_sampling/slot_trace.rs\"] mod slot_trace;\n");
    Ok(())
}

fn cpu_placement(files: &mut Files, root: &Path, tool: &Path, allocations: bool) -> Result<()> {
    let engine = source::replace_once(
        &files["engine.rs"],
        "fn read_bounded",
        "pub(super) fn read_bounded",
        "bounded observer changed; review the diagnostic adapter",
    )?;
    files.insert("engine.rs".into(), engine);
    let diagnostic = source::read(tool.join("scripts/diagnostics/cpu_placement.rs"))?;
    files
        .get_mut("small_file.rs")
        .ok_or("missing small-file sampler")?
        .push_str(&format!("\n{diagnostic}"));
    let main = files.get_mut("root.rs").ok_or("missing sampler entry")?;
    *main += "\n#[test]\n#[ignore]\nfn release_cpu_placement() { small_file::cpu_placement_diagnostic(); }\n";
    if allocations {
        let counter = source::read(root.join("scripts/diagnostics/telemetry_allocations.rs"))?;
        let counter = counter
            .lines()
            .filter(|line| !line.starts_with("//!") && !line.starts_with("#![allow"))
            .collect::<Vec<_>>()
            .join("\n");
        let original = source::read(root.join("src/tests/mod.rs"))?;
        let original = source::block(&original, "fn shard_index_pattern")?.replacen(
            "fn shard_index_pattern",
            "fn original_pattern",
            1,
        );
        *main += &format!("\n{counter}\n{original}");
        *main += "\n#[test]\n#[cfg(target_os = \"linux\")]\nfn placement_fault_reads_keep_the_rust_heap_untouched() {\n    let (_, counts) = count_heap(|| { for _ in 0..16 { std::hint::black_box(small_file::placement_faults()); } });\n    assert_eq!(counts, Counts::default());\n}\n";
    }
    Ok(())
}

fn native_service_clock(files: &mut Files, tool: &Path) -> Result<()> {
    let mut process = source::replace_once(
        &files["process.rs"],
        "pub(super) fn snapshot() -> String {",
        "fn snapshot_process() -> String {",
        "diagnostic process entry changed",
    )?;
    process.push_str("\n#[path = \"native_service_clock.rs\"] mod native_service_clock;\npub(super) fn snapshot() -> String { native_service_clock::snapshot(snapshot_process) }\npub(super) fn native_clock_probe() { native_service_clock::probe(); }\n");
    files.insert("process.rs".into(), process);
    files.insert(
        "native_service_clock.rs".into(),
        source::read(tool.join("scripts/diagnostics/native_service_clock.rs"))?,
    );
    files.get_mut("root.rs").ok_or("missing sampler root")?.push_str(
        "\n#[test]\n#[ignore]\nfn native_service_clock_probe() { process::native_clock_probe(); }\n",
    );
    Ok(())
}

pub fn render(root: &Path, tool: &Path, variant: &str, allocations: bool) -> Result<Files> {
    if allocations && variant != "cpu-placement" {
        return Err("allocation probe is only supported for CPU placement".into());
    }
    if variant == "dynamic" {
        return dynamic::render(root, tool);
    }
    let mut files = sampler::render(root, tool)?;
    match variant {
        "native-service-clock" => native_service_clock(&mut files, tool)?,
        "views" => views(&mut files, tool)?,
        "verify" => {
            files.insert("engine.rs".into(), verify_engine(&files["engine.rs"])?);
        }
        "stream" => {
            views(&mut files, tool)?;
            files.insert(
                "engine.rs".into(),
                verify_engine(&files["engine.rs"])?
                    .replace("__rse_verify_scratch", "__rse_stream_core")
                    .replace(
                        "verify scratch observation failed",
                        "stream core observation failed",
                    ),
            );
        }
        "shard-slot" => shard_slot(&mut files, tool)?,
        "cpu-placement" => cpu_placement(&mut files, root, tool, allocations)?,
        _ => return Err(format!("unknown diagnostic overlay: {variant}").into()),
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn digest(files: Files, root: &Path) -> String {
        use std::fmt::Write;
        let mut hash = Sha256::new();
        for (name, text) in files {
            hash.update(name.as_bytes());
            hash.update(b"\0");
            hash.update(
                text.replace(&root.display().to_string(), "<SOURCE>")
                    .as_bytes(),
            );
        }
        let mut result = String::with_capacity(64);
        for byte in hash.finalize() {
            write!(result, "{byte:02x}").unwrap();
        }
        result
    }

    #[test]
    fn native_clock_changes_only_the_independent_process_observer() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let original = sampler::render(root, root).unwrap();
        let observed = render(root, root, "native-service-clock", false).unwrap();
        for (name, value) in &original {
            if name != "process.rs" && name != "root.rs" {
                assert_eq!(&observed[name], value, "{name}");
            }
        }
        assert!(observed["root.rs"].starts_with(&original["root.rs"]));
        assert!(observed.contains_key("native_service_clock.rs"));
    }

    #[test]
    fn native_clock_rejects_a_changed_process_entry() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let mut files = sampler::render(root, root).unwrap();
        files.insert("process.rs".into(), "fn changed() {}".into());
        assert!(native_service_clock(&mut files, root).is_err());
    }

    fn dynamic_source_files(root: &Path) -> Files {
        [
            "src/lib.rs",
            "src/matrix.rs",
            "src/core/reconstruct.rs",
            "src/galois_8/policy.rs",
        ]
        .into_iter()
        .chain(super::super::leopard::FILES)
        .map(|name| (name.into(), source::read(root.join(name)).unwrap()))
        .collect()
    }

    #[test]
    fn dynamic_observation_preserves_adaptive_verification_bodies() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let original = dynamic_source_files(root);
        let observed = dynamic::patch(&original, root).unwrap();
        let name = "src/galois_8/policy.rs";
        let body = |text: &str, entry: &str| {
            let mut selected = String::new();
            source::method(text, entry, |original, _| {
                selected = original.to_owned();
                Ok(())
            })
            .unwrap();
            selected
        };
        let inverse = "    pub fn invert(";
        let mut observed_inverse = body(&observed["src/matrix.rs"], inverse);
        for stage in ["invert_ready", "invert_coded"] {
            let marker = format!("        work.__rse_buffer_view(\"{stage}\");\n");
            assert_eq!(observed_inverse.matches(&marker).count(), 1);
            observed_inverse = observed_inverse.replace(&marker, "");
        }
        assert_eq!(observed_inverse, body(&original["src/matrix.rs"], inverse));
        let builder = "    fn build_data_decode_matrix(";
        let observed_builder = body(&observed["src/core/reconstruct.rs"], builder);
        let marker = "        selected.__rse_buffer_view(\"decode_submatrix\");\n";
        assert_eq!(observed_builder.matches(marker).count(), 1);
        assert_eq!(
            observed_builder.replace(marker, ""),
            body(&original["src/core/reconstruct.rs"], builder),
            "only the declared submatrix observation may change the inverse builder"
        );
        for entry in [
            "    pub fn verify_opt<T>",
            "    pub fn verify_with_buffer_opt<T, U>",
            "    pub fn verify_with_workspace_opt(",
        ] {
            assert_eq!(
                body(&observed[name], entry),
                body(&original[name], entry),
                "{entry} must remain production source"
            );
        }
    }

    #[test]
    fn generators_bind_current_source_and_retain_reviewed_diagnostic_bytes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        for (variant, allocations, expected) in [
            (
                "views",
                false,
                "65313d780a1a601111a127fbe6a204a1c464bbc3c6fc90996c50d097843d6009",
            ),
            (
                "verify",
                false,
                "9676ae8acc91001c66e964bf6bffba020d4123392202f20989c151811aa7a540",
            ),
            (
                "stream",
                false,
                "f1b6dfbb7291a6d69bb6fb6ea1675f17c846469ed4c2a664659af753b8355273",
            ),
            (
                "dynamic",
                false,
                "d3a3d136c406622069d7f0e5afc481e320c599a47dcf2a43b346ca5132de472f",
            ),
            (
                "shard-slot",
                false,
                "e931717efceffaa7e9dda0ba147366e710a3a1826b2f5abe8b0434a2ec951e02",
            ),
            (
                "cpu-placement",
                false,
                "557ef5d47a4ec39c31ad3ba0b2248b0dbbbacfc4d30485192781f14110c80f4f",
            ),
            (
                "cpu-placement",
                true,
                "1e4ecb537f17e1a9a08292d728f6b135cf4bcf7c5d0878aefe858bd56c5e7fda",
            ),
        ] {
            assert_eq!(
                digest(render(root, root, variant, allocations).unwrap(), root),
                expected,
                "{variant}"
            );
        }
        // Unlike the frozen sampler snapshots above, this binds current production
        // bodies, including reduced systematic equations, the direct single
        // equation and dense fallback. The observer must preserve every branch.
        let original = dynamic_source_files(root);
        assert_eq!(
            digest(dynamic::patch(&original, root).unwrap(), root),
            "ef8c8ec6e49ae2f01692e61cf584aa96a3d8c25dd08f73342b51bce39c28fcc7"
        );
    }
}
