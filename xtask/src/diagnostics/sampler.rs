use super::{catalog::*, source};
use crate::Result;
use regex::Regex;
use std::{collections::BTreeMap, path::Path, sync::LazyLock};

static SMALL_LOOP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?ms)^            for _ in 0\.\.iterations \{.*?^            }")
        .expect("fixed loop regex")
});
static HOTSPOT_LOOP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?ms)^    for _ in 0\.\.iterations \{.*?^    }").expect("fixed hotspot regex")
});
static ARM_END: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^        }").expect("fixed arm regex"));
static LOOP_START: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^            for _ in 0\.\.iterations \{").expect("fixed loop start regex")
});
static LOOP_END: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^            }$").expect("fixed loop end regex"));

pub fn original_loop<'a>(text: &'a str, pattern: &str) -> Result<&'a str> {
    let marker = format!("        {pattern} => {{");
    if text.matches(&marker).count() != 1 {
        return Err("benchmark operation arm changed".into());
    }
    let arm = &text[text.find(&marker).ok_or("missing benchmark arm")?..];
    let end = ARM_END
        .find(arm)
        .ok_or("benchmark arm closing boundary changed")?
        .start();
    let arm = &arm[..end];
    if arm.matches("for _ in 0..iterations {").count() != 1 {
        return Err("benchmark operation loop changed".into());
    }
    SMALL_LOOP
        .find(arm)
        .map(|m| m.as_str())
        .ok_or_else(|| "benchmark operation loop changed".into())
}

pub fn exclude_setup(text: &str, name: &str, count: usize) -> Result<String> {
    let original = source::block(text, name)?;
    let starts: Vec<_> = LOOP_START.find_iter(original).collect();
    if starts.len() != count {
        return Err("benchmark loop layout changed; review the timing adapter".into());
    }
    let mut changed = original.to_owned();
    for start in starts.iter().rev() {
        let close = LOOP_END
            .find(&original[start.end()..])
            .ok_or("benchmark loop closing scope changed")?;
        changed.insert_str(
            start.end() + close.end(),
            "\n            measured += start.elapsed();",
        );
        changed.insert_str(start.start(), "            let start = Instant::now();\n");
    }
    changed = changed
        .replacen(
            "    let start = Instant::now();",
            "    let mut measured = std::time::Duration::ZERO;",
            1,
        )
        .replacen(
            "    let elapsed = start.elapsed();",
            "    let elapsed = measured;",
            1,
        );
    let restored = changed
        .replace("            let start = Instant::now();\n", "")
        .replace("\n            measured += start.elapsed();", "")
        .replacen(
            "    let mut measured = std::time::Duration::ZERO;",
            "    let start = Instant::now();",
            1,
        )
        .replacen(
            "    let elapsed = measured;",
            "    let elapsed = start.elapsed();",
            1,
        );
    if restored != original {
        return Err("timing adapter changed a non-timing statement".into());
    }
    Ok(text.replacen(original, &changed, 1))
}
pub fn reserve_outputs(text: &str) -> Result<String> {
    source::replace_once(
        text,
        "let mut writers = vec![Vec::with_capacity(case.shard_size); case.parity_shards];",
        "let mut writers = reserved_stream_outputs(case.parity_shards, case.shard_size);",
        "stream output construction changed; review the shared fixture adapter",
    )
}

pub fn prepared_kernel(text: &str, profile: &str) -> Result<String> {
    let small = profile == "small_file";
    let mut arms = Vec::new();
    let mut descriptors = Vec::new();
    for &(pattern, op) in if small { SMALL_KERNELS } else { STREAM_KERNELS } {
        let mut code = original_loop(text, pattern)?.to_owned();
        let replacements: &[(&str, &str)] = match op {
            "encode" => &[("rs.encode(&mut shards)", "rs.encode(&mut *original)")],
            "verify" => &[("rs.verify(&shards)", "rs.verify(&*original)")],
            "verify_with_buffer" => &[(
                "rs.verify_with_buffer(&shards, &mut buffer)",
                "rs.verify_with_buffer(&*original, &mut *scratch)",
            )],
            "reconstruct_shard_slot" => {
                &[("shards_to_slots(&original)", "shards_to_slots(original)")]
            }
            "reconstruct_some_data_only" => &[("&required)", "required)")],
            "encode_stream" => &[("data.iter()", "original.iter()")],
            "verify_stream" => &[("shards.iter()", "original.iter()")],
            "reconstruct_stream" => &[
                ("shards.iter()", "original.iter()"),
                ("&shards[0]", "&original[0]"),
            ],
            _ => &[],
        };
        for &(before, after) in replacements {
            code = source::replace_once(&code, before, after, "benchmark operand layout changed")?;
        }
        if ["verify", "verify_with_buffer"].contains(&op) {
            let old = format!(
                "rs.{op}(&*original{}).unwrap();",
                if op == "verify" {
                    ""
                } else {
                    ", &mut *scratch"
                }
            );
            let new = format!(
                "assert!(std::hint::black_box({}));",
                old.trim_end_matches(';')
            );
            code =
                source::replace_once(&code, &old, &new, "benchmark verification oracle changed")?;
        }
        arms.push(format!("        {pattern} => {{\n{code}\n        }}"));
        let contract = fixture(op)?;
        let (entry, policy, missing, required) = (
            contract.entry,
            contract.policy,
            contract.missing.expression(),
            contract.required.expression(),
        );
        descriptors.push(format!(
            "        {pattern} => (\"{entry}\", \"{policy}\", {missing}, {required}),"
        ));
    }
    let (signature, bindings, contract_signature) = if small {
        arms.push("        _ => panic!(\"unsupported small-file release operation\"),".into());
        descriptors
            .push("        _ => panic!(\"unsupported small-file release operation\"),".into());
        (
            "prepared: &mut PreparedSmallFile, iterations: usize",
            "    let case = prepared.case; let operation = prepared.operation; let rs = &prepared.codec; let original = &mut prepared.source; let scratch = &mut prepared.scratch; let required = &prepared.required;\n",
            "case: BenchCase, operation: SmallFileOp",
        )
    } else {
        for arm in &mut arms {
            *arm = arm.replace("&options", "options");
        }
        (
            "prepared: &mut PreparedStream, iterations: usize",
            "    let case = prepared.stream_case.case; let operation = prepared.operation; let rs = &prepared.codec; let original = &mut prepared.source; let options = &prepared.options;\n",
            "case: BenchCase, operation: StreamOperation",
        )
    };
    Ok(format!(
        "\nfn execute_release_kernel({signature}) -> std::time::Duration {{\n{bindings}    let start = Instant::now();\n    match operation {{\n{}\n    }}\n    start.elapsed()\n}}\nfn release_contract({contract_signature}) -> (&'static str, &'static str, u64, u64) {{\n    match operation {{\n{}\n    }}\n}}\n",
        arms.join("\n"),
        descriptors.join("\n")
    ))
}

pub fn smoke_kernel(text: &str) -> Result<String> {
    let original = source::block(text, "fn run_operation")?;
    let signature =
        "fn run_operation(case: BenchCase, operation: Operation, iterations: usize) -> SmokeResult";
    let native = "fn execute_smoke_kernel(case: BenchCase, operation: Operation, iterations: usize, rs: &ReedSolomon) -> SmokeResult";
    let ctor = "    let rs = ReedSolomon::new(case.data_shards, case.parity_shards).unwrap();\n";
    if original.matches(signature).count() != 1 || original.matches(ctor).count() != 1 {
        return Err("smoke preparation changed".into());
    }
    let kernel = original
        .replacen(signature, native, 1)
        .replacen(ctor, "", 1)
        .replacen(
            "rs.verify(&shards).unwrap();",
            "assert!(std::hint::black_box(rs.verify(&shards).unwrap()));",
            1,
        );
    let restored = kernel
        .replacen(native, signature, 1)
        .replacen("    let bytes =", &format!("{ctor}    let bytes ="), 1)
        .replacen(
            "assert!(std::hint::black_box(rs.verify(&shards).unwrap()));",
            "rs.verify(&shards).unwrap();",
            1,
        );
    if restored != original {
        return Err("smoke adapter changed original timed statements".into());
    }
    let wrapper = format!(
        "{signature} {{\n{ctor}    execute_smoke_kernel(case, operation, iterations, &rs)\n}}"
    );
    let specs = [
        (
            "Encode",
            "Vec<Vec<u8>>",
            "iteration-generated-source-parity-in-place",
            "0",
            "0",
        ),
        (
            "Update",
            "Vec<&Vec<u8>>+Vec<Option<&Vec<u8>>>->Vec<&mut Vec<u8>>",
            "iteration-generated-source-cloned-old-updated-and-parity",
            "0",
            "0",
        ),
        (
            "Verify",
            "Vec<Vec<u8>>",
            "iteration-generated-source-internal-scratch-untracked",
            "0",
            "0",
        ),
        (
            "Reconstruct",
            "Vec<Option<Vec<u8>>>",
            "iteration-generated-source-moved-options-drop-missing-new-output-untracked",
            "1 | (1 << case.data_shards)",
            "(1 << (case.data_shards+case.parity_shards))-1",
        ),
        (
            "ReconstructData",
            "Vec<Option<Vec<u8>>>",
            "iteration-generated-source-moved-options-drop-missing-new-output-untracked",
            "3",
            "(1 << case.data_shards)-1",
        ),
    ];
    let arms = specs
        .iter()
        .map(|(variant, entry, policy, missing, required)| {
            format!("Operation::{variant} => (\"{entry}\",\"{policy}\",{missing},{required}),")
        })
        .collect::<String>();
    Ok(format!(
        "{}\n{kernel}\nfn smoke_contract(case: BenchCase, operation: Operation) -> (&'static str,&'static str,u64,u64) {{ match operation {{{arms}_ => panic!(\"unsupported smoke release case\"), }} }}",
        text.replacen(original, &wrapper, 1)
    ))
}

pub fn hotspot_kernel(text: &str) -> Result<String> {
    let original = source::block(text, "macro_rules! make_random_shards")?;
    let updated = original
        .replace(
            "($per_shard:expr, $size:expr)",
            "($rng:expr, $per_shard:expr, $size:expr)",
        )
        .replace("fill_random(s);", "fill_random(&mut *$rng, s);");
    if updated == original {
        return Err("hotspot random allocation constructor changed".into());
    }
    let mut text = text.replacen(original, &updated, 1);
    let fill = source::block(&text, "pub fn fill_random")?;
    text = text.replacen(
        fill,
        "fn fill_random(rng: &mut super::workload::InputRng, arr: &mut [u8]) { rng.fill(arr); }",
        1,
    );
    for name in ["data", "some"] {
        let declaration = format!("fn bench_reconstruct_{name}_hotspot");
        let original = source::block(&text, &declaration)?;
        let before = HOTSPOT_LOOP
            .find(original)
            .ok_or("missing hotspot loop")?
            .as_str();
        let mut body = original[original
            .find("    let r = ")
            .ok_or("missing hotspot constructor")?..]
            .to_owned();
        body=body.replacen("    let r = ReedSolomon::new(data_shards, parity_shards).unwrap();",&format!("    let case = prepared.case; let data_shards=case.data; let parity_shards=case.parity; let shard_size=prepared.shard_size; let scenario=case.name; let missing_indices=case.missing; {}let r=&prepared.codec; let rng=&mut prepared.rng;",if name=="some" {"let required_indices=case.required.unwrap_or(&[]); "} else {""}),1).replacen("    let iterations = benchmark_test_iterations();\n","",1);
        if name == "data" {
            let start = body
                .find("    let missing_data_indices:")
                .ok_or("missing hotspot data mask")?;
            let end = body[start..]
                .find("    let useful_shards")
                .ok_or("missing hotspot useful count")?
                + start;
            body.replace_range(
                start..end,
                "    let missing_data_indices=&prepared.missing_data;\n",
            );
            body = body
                .replace(
                    "for &index in &missing_data_indices",
                    "for &index in missing_data_indices",
                )
                .replace(
                    "shard_index_pattern(data_shards, &missing_data_indices)",
                    "shard_index_pattern(data_shards, missing_data_indices)",
                );
        } else {
            let start = body
                .find("    let mut required =")
                .ok_or("missing hotspot required mask")?;
            let end = body[start..]
                .find("    let mut baseline_total")
                .ok_or("missing hotspot counters")?
                + start;
            body.replace_range(start..end, "    let required=&prepared.required;\n\n");
            body = body.replace("&required)", "required)");
        }
        body = source::replace_once(
            &body,
            "make_random_shards!(shard_size, data_shards + parity_shards)",
            "make_random_shards!(rng, shard_size, data_shards + parity_shards)",
            "hotspot per-iteration preparation changed",
        )?;
        let after = HOTSPOT_LOOP
            .find(&body)
            .ok_or("missing adapted hotspot loop")?
            .as_str();
        let restored = after
            .replace("make_random_shards!(rng, ", "make_random_shards!(")
            .replace(
                "for &index in missing_data_indices",
                "for &index in &missing_data_indices",
            )
            .replace(
                "reconstruct_some(&mut candidate, required)",
                "reconstruct_some(&mut candidate, &required)",
            );
        if restored != before {
            return Err("hotspot adapter changed original timed iteration statements".into());
        }
        text=text.replacen(original,&format!("{declaration}(prepared: &mut PreparedHotspot, iterations: usize) -> ReconstructionHotspotBenchResult {{\n{body}"),1);
    }
    Ok(text)
}

pub fn hotspot_cases() -> String {
    fn indices(values: &[usize]) -> String {
        format!(
            "&[{}]",
            values
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
    HOTSPOT_CASES
        .iter()
        .map(|case| {
            format!(
                "HotspotCase {{data:{},parity:{},name:\"{}\",missing:{},required:{}}}",
                case.data,
                case.parity,
                case.name,
                indices(case.missing),
                case.required
                    .map_or_else(|| "None".into(), |v| format!("Some({})", indices(v)))
            )
        })
        .collect::<Vec<_>>()
        .join(",\n")
}

pub fn render(root: &Path, tool: &Path) -> Result<BTreeMap<String, String>> {
    let templates = tool.join("scripts/release-sampling");
    let mut rendered = BTreeMap::new();
    for name in [
        "engine.rs",
        "process.rs",
        "layout.rs",
        "pattern.rs",
        "workload.rs",
        "service_window.rs",
    ] {
        rendered.insert(name.into(), source::read(templates.join(name))?);
    }
    for (profile, filename, iterations) in [
        ("smoke", "benchmark_smoke.rs", "fn smoke_iterations"),
        (
            "small_file",
            "benchmark_small_files.rs",
            "fn small_file_iterations",
        ),
        (
            "stream",
            "benchmark_stream_paths.rs",
            "fn stream_iterations",
        ),
    ] {
        let mut text = source::read(root.join("tests").join(filename))?;
        text = source::replace_once(
            &text,
            "#[path = \"../benches/common/mod.rs\"]\nmod bench_common;\nmod common;",
            "use super::{bench_common, common};",
            "benchmark common-module layout changed",
        )?;
        text = text.replacen(
            source::block(&text, iterations)?,
            &format!("{iterations}() -> usize {{ super::sampling::iterations() }}"),
            1,
        );
        if profile != "smoke" {
            text = exclude_setup(
                &text,
                if profile == "small_file" {
                    "fn run_operation"
                } else {
                    "fn run_stream_operation"
                },
                if profile == "small_file" { 11 } else { 3 },
            )?;
        }
        if profile == "stream" {
            text = reserve_outputs(&text)?;
        }
        if profile == "smoke" {
            text = smoke_kernel(&text)?;
        }
        let mut generated = format!(
            "{text}\n{}",
            source::read(templates.join(format!("{profile}.rs")))?
        );
        if profile != "smoke" {
            generated += &prepared_kernel(&text, profile)?;
        }
        rendered.insert(format!("{profile}.rs"), generated);
    }
    let original = source::read(root.join("src/tests/mod.rs"))?;
    let mut text = HOTSPOT_BLOCKS
        .iter()
        .map(|name| source::block(&original, name))
        .collect::<Result<Vec<_>>>()?
        .join("\n");
    if text.matches(": String,").count() != 2 {
        return Err("hotspot result metadata changed; review the bounded adapter".into());
    }
    text = text.replace(": String,", ": super::pattern::ShardPattern,");
    text=text.replacen(source::block(&text,"fn shard_index_pattern")?,"fn shard_index_pattern(data_shards: usize, indices: &[usize]) -> super::pattern::ShardPattern { super::pattern::ShardPattern::new(data_shards, indices) }",1);
    text = text.replacen(
        source::block(&text, "fn benchmark_test_iterations")?,
        "fn benchmark_test_iterations() -> usize { super::sampling::iterations() }",
        1,
    );
    text = hotspot_kernel(&text)?;
    let prefix = "use std::{fs, path::PathBuf, time::Instant};\nuse rustfs_erasure_codec::galois_8::ReedSolomon;\nfn benchmark_metrics_enabled() -> bool { cfg!(feature=\"benchmark-metrics\") }\nconst BENCHMARK_ARTIFACT_SCHEMA_VERSION: u32 = 1;\n";
    rendered.insert(
        "hotspot.rs".into(),
        format!(
            "{prefix}{text}\n{}",
            source::read(templates.join("hotspot.rs"))?
                .replace("__HOTSPOT_CASES__", &hotspot_cases())
        ),
    );
    let mut main = format!(
        "#![allow(dead_code)]\n#[path=\"{}/benches/common/mod.rs\"] mod bench_common;\n#[path=\"{}/tests/common/mod.rs\"] mod common;\n",
        root.display(),
        root.display()
    );
    main += &[
        "engine",
        "process",
        "layout",
        "pattern",
        "workload",
        "smoke",
        "small_file",
        "stream",
        "hotspot",
    ]
    .iter()
    .map(|name| format!("#[path=\"rse_release_sampling/{name}.rs\"] mod {name};"))
    .collect::<Vec<_>>()
    .join("\n");
    main = main.replace("mod engine;", "mod sampling;");
    main += "\n#[test]\n#[ignore]\nfn release_controlled_profile() { match std::env::var(\"RSE_SAMPLER_PROFILE\").unwrap().as_str() {";
    main += &["smoke", "small_file", "stream", "hotspot"]
        .iter()
        .map(|name| format!("\"{name}\" => {name}::release_profile()"))
        .collect::<Vec<_>>()
        .join(",");
    main += ", _ => panic!(\"unknown release profile\") } }\n";
    rendered.insert("root.rs".into(), main);
    Ok(rendered)
}
