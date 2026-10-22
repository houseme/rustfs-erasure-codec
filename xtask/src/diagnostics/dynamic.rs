//! Dynamic-storage observations over disposable copies of either codec revision.
use super::{catalog, leopard, sampler, source, templates};
use crate::Result;
use std::{collections::BTreeMap, path::Path};

pub const MODULE: &str = "__rse_dynamic_buffers";
const API: &str = "rustfs_erasure_codec::__rse_dynamic_buffers::";
const LIB: &str = "crate::__rse_dynamic_buffers::";

pub fn matrix(text: &str) -> Result<String> {
    let heap = if text.contains("data: SmallVec<") {
        "self.data.spilled()"
    } else {
        "true"
    };
    let marker = "impl<F: Field> Matrix<F> {";
    let helper = format!(
        "\n    pub(crate) fn __rse_buffer_view(&self, stage: &'static str) {{\n        {LIB}span(\"matrix\",stage,0,self.data.as_ptr(),self.data.len(),Some(self.data.capacity()),if {heap} {{\"heap\"}} else {{\"inline\"}},true);\n    }}\n"
    );
    let text = source::replace_once(
        text,
        marker,
        &(marker.to_owned() + &helper),
        "matrix implementation changed",
    )?;
    let text = source::method(&text, "    pub fn invert(", |original, edit| {
        if original
            .matches("        let mut work = self.augment(&Self::identity(row_count))?;")
            .count()
            != 1
        {
            return Err("matrix scratch initializer changed".into());
        }
        let marker = "        work.gaussian_elim()?;";
        edit.replace(marker, &format!("        work.__rse_buffer_view(\"invert_ready\");\n{marker}\n        work.__rse_buffer_view(\"invert_coded\");"), 1)
    })?;
    source::method(&text, "    pub fn new(rows:", |_, edit| {
        let heap = if text.contains("data: SmallVec<") {
            "data.spilled()"
        } else {
            "true"
        };
        edit.before("        Matrix {", &format!("        {LIB}span(\"matrix\",\"constructed\",0,data.as_ptr(),data.len(),Some(data.capacity()),if {heap} {{\"heap\"}} else {{\"inline\"}},true);\n"))
    })
}

pub fn region(text: &str, declaration: &str) -> Result<String> {
    source::method(text, declaration, |original, edit| {
        let end = original.find('{').ok_or("missing decode region body")? + 1;
        edit.after(
            &original[..end],
            &format!("\n        let _rse_region = {LIB}Region::new(\"decode\");"),
        )
    })
}

pub fn decode(text: &str) -> Result<String> {
    let mut text = text.to_owned();
    for declaration in [
        "    pub fn reconstruct<",
        "    pub fn reconstruct_data<",
        "    pub fn reconstruct_some<",
    ] {
        text = region(&text, declaration)?;
    }
    let declaration = "    pub(crate) fn get_data_decode_matrix(";
    text = source::method(&text, declaration, |_, edit| {
        edit.after(
            "            if let Some(entry) = cache.get(invalid_indices) {",
            "\n                entry.__rse_buffer_view(\"decode_cached\");",
        )?;
        edit.before(
            "        let data_decode_matrix = Arc::new(",
            "        sub_matrix.__rse_buffer_view(\"decode_submatrix\");\n",
        )?;
        edit.before(
            "        Ok(data_decode_matrix)",
            "        data_decode_matrix.__rse_buffer_view(\"decode_ready\");\n",
        )
    })?;
    text = region(&text, declaration)?;
    if text.contains("    fn reconstruct_leopard_impl<") {
        text = source::method(
            &text,
            "    fn reconstruct_leopard_impl<",
            |original, edit| {
                let (buffers, prepared, decoded, after) =
                    if original.contains("let mut recovered: SmallVec<[Vec<u8>; 32]>") {
                        (
                            "recovered",
                            "        let mut outputs: SmallVec<[&mut [u8]; 32]> =",
                            "        drop(input_data);",
                            true,
                        )
                    } else {
                        (
                            "output_bufs",
                            "        let mut outputs: Vec<&mut [u8]> = output_bufs",
                            "        for i in 0..total {\n            if present[i] {",
                            false,
                        )
                    };
                edit.before(
                    prepared,
                    &format!(
                        "        {LIB}buffers(\"leopard_decoded\",\"prepared\",&{buffers});\n"
                    ),
                )?;
                if after {
                    edit.after(
                        decoded,
                        &format!(
                            "\n        {LIB}buffers(\"leopard_decoded\",\"decoded\",&{buffers});"
                        ),
                    )
                } else {
                    edit.before(
                        decoded,
                        &format!(
                            "        {LIB}buffers(\"leopard_decoded\",\"decoded\",&{buffers});\n"
                        ),
                    )
                }
            },
        )?;
    }
    Ok(text)
}

pub fn patch(original: &BTreeMap<String, String>, tool: &Path) -> Result<BTreeMap<String, String>> {
    let get = |name: &str| {
        original
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| format!("missing dynamic source: {name}"))
    };
    let mut result = leopard::patch(original)?;
    result.extend([
        (
            "src/lib.rs".into(),
            format!(
                "{}\n#[doc(hidden)]\npub mod {MODULE};\n",
                get("src/lib.rs")?
            ),
        ),
        ("src/matrix.rs".into(), matrix(get("src/matrix.rs")?)?),
        (
            "src/core/reconstruct.rs".into(),
            decode(get("src/core/reconstruct.rs")?)?,
        ),
        (
            "src/galois_8/policy.rs".into(),
            region(
                get("src/galois_8/policy.rs")?,
                "    pub fn reconstruct_opt(",
            )?,
        ),
        (
            format!("src/{MODULE}.rs"),
            templates::native("dynamic", tool)?,
        ),
    ]);
    Ok(result)
}

fn caller(text: &str, profile: &str) -> Result<String> {
    let declaration = if profile == "smoke" {
        "fn execute_smoke_kernel"
    } else {
        "fn execute_release_kernel"
    };
    let kernel = source::block(text, declaration)?;
    let changed = if profile == "smoke" {
        let mut edit = source::Edits::new(kernel);
        edit.after(
            "    for _ in 0..iterations {",
            &format!("\n        let _rse_epoch = {API}Epoch::begin();"),
        )?;
        let marker = "                    make_full_shards(seed, case.data_shards, case.parity_shards, case.shard_size);";
        edit.replace(
            marker,
            &format!("{marker}\n                {API}vectors(\"source\",\"created\",&shards);"),
            6,
        )?;
        edit.before("                let old_refs =", &format!("                {API}vectors(\"old_data\",\"cloned\",&old_data);\n                {API}vectors(\"updated\",\"changed\",&updated);\n                {API}vectors(\"parity\",\"before\",&parity);\n"))?;
        edit.after(
            "                rs.update(&old_refs, &changes, &mut parity_refs).unwrap();",
            &format!("\n                {API}vectors(\"parity\",\"updated\",&parity);"),
        )?;
        for method in ["reconstruct", "reconstruct_data"] {
            let marker = format!("                rs.{method}(&mut shards).unwrap();");
            edit.replace(&marker, &format!("                {API}optional(\"dynamic\",\"missing\",&shards);\n{marker}\n                {API}optional(\"dynamic\",\"recovered\",&shards);"), 1)?;
        }
        let marker = "                rs.encode(&mut shards).unwrap();";
        edit.replace(
            marker,
            &format!("{marker}\n                {API}vectors(\"source\",\"encoded\",&shards);"),
            5,
        )?;
        let marker = "                let mut shards: Vec<Option<Vec<u8>>> = shards.into_iter().map(Some).collect();";
        edit.replace(
            marker,
            &format!("{marker}\n                {API}optional(\"dynamic\",\"moved\",&shards);"),
            2,
        )?;
        edit.finish()?
    } else {
        let mut changed = kernel.to_owned();
        for &(pattern, operation) in if profile == "small_file" {
            catalog::SMALL_KERNELS
        } else {
            catalog::STREAM_KERNELS
        } {
            let original = sampler::original_loop(kernel, pattern)?;
            let mut edit = source::Edits::new(original);
            let scratch = if profile == "small_file" {
                format!("\n                {API}vectors(\"scratch\",\"retained\",scratch);")
            } else {
                String::new()
            };
            edit.after("            for _ in 0..iterations {", &format!("\n                let _rse_epoch = {API}Epoch::begin();\n                {API}vectors(\"source\",\"retained\",original);{scratch}"))?;
            if profile == "small_file" && operation.starts_with("reconstruct") {
                let slot = operation == "reconstruct_shard_slot";
                let helper = if slot { "slots" } else { "optional" };
                let marker = if slot {
                    "                let mut shards = shards_to_slots(original);"
                } else {
                    "                let mut shards: Vec<Option<Vec<u8>>> = original.iter().cloned().map(Some).collect();"
                };
                edit.after(
                    marker,
                    &format!("\n                {API}{helper}(\"dynamic\",\"cloned\",&shards);"),
                )?;
                let method = if slot {
                    "reconstruct"
                } else if operation == "reconstruct_some_data_only" {
                    "reconstruct_some"
                } else {
                    operation
                };
                let args = if operation == "reconstruct_some_data_only" {
                    "&mut shards, required"
                } else {
                    "&mut shards"
                };
                let marker = format!("                rs.{method}({args}).unwrap();");
                edit.replace(&marker, &format!("                {API}{helper}(\"dynamic\",\"missing\",&shards);\n{marker}\n                {API}{helper}(\"dynamic\",\"recovered\",&shards);"), 1)?;
            }
            changed = source::replace_once(
                &changed,
                original,
                &edit.finish()?,
                "dynamic operation loop changed",
            )?;
        }
        changed
    };
    source::replace_once(text, kernel, &changed, "dynamic caller kernel changed")
}

fn hotspot(text: &str) -> Result<String> {
    let mut text = source::top(text, "fn option_shards_with_missing", |edit| {
        edit.after(
            "    let mut working = shards_to_option_shards(shards);",
            &format!("\n    {API}optional(\"clone\",\"cloned\",&working);"),
        )
    })?;
    for name in ["data", "some"] {
        text = source::top(
            &text,
            &format!("fn bench_reconstruct_{name}_hotspot"),
            |edit| {
                edit.after(
                    "    for _ in 0..iterations {",
                    &format!("\n        let _rse_epoch = {API}Epoch::begin();"),
                )?;
                edit.after("        let mut shards = make_random_shards!(rng, shard_size, data_shards + parity_shards);", &format!("\n        {API}vectors(\"source\",\"created\",&shards);"))?;
                edit.after(
                    "        r.encode(&mut shards).unwrap();",
                    &format!("\n        {API}vectors(\"source\",\"encoded\",&shards);"),
                )?;
                for role in ["baseline", "candidate"] {
                    edit.after(&format!("        let mut {role} = option_shards_with_missing(&shards, missing_indices);"), &format!("\n        {API}optional(\"{role}\",\"missing\",&{role});"))?;
                    let method = match (role, name) {
                        ("baseline", "data") => "reconstruct",
                        ("candidate", "some") => "reconstruct_some",
                        _ => "reconstruct_data",
                    };
                    let args = if method == "reconstruct_some" {
                        format!("&mut {role}, required")
                    } else {
                        format!("&mut {role}")
                    };
                    edit.after(
                        &format!("        r.{method}({args}).unwrap();"),
                        &format!("\n        {API}optional(\"{role}\",\"recovered\",&{role});"),
                    )?;
                }
                Ok(())
            },
        )?;
    }
    Ok(text)
}

pub fn render(root: &Path, tool: &Path) -> Result<BTreeMap<String, String>> {
    let mut files = sampler::render(root, tool)?;
    for profile in ["smoke", "small_file", "stream"] {
        let name = format!("{profile}.rs");
        files.insert(name.clone(), caller(&files[&name], profile)?);
    }
    files.insert("hotspot.rs".into(), hotspot(&files["hotspot.rs"])?);
    let mut text = files["engine.rs"].clone();
    for (marker, observation) in [
        (
            "    let constructor_budget = std::thread::available_parallelism().unwrap().get();",
            format!("    let _rse_root = {API}Root::start(page_size);\n"),
        ),
        (
            "            ITERATIONS.with(|n| n.set(count));",
            format!("            {API}observation(index);\n"),
        ),
    ] {
        text = source::replace_once(
            &text,
            marker,
            &(observation + marker),
            "original sampler dynamic boundary changed",
        )?;
    }
    for marker in [
        "            println!(\n                \"\\nRSE_TIMING ",
        "        println!(\n            \"\\nRSE_SAMPLE ",
    ] {
        let indent = marker
            .split_once("println!")
            .ok_or("missing numeric export")?
            .0;
        text = source::replace_once(
            &text,
            marker,
            &format!(
                "{indent}let rse_diagnostic_workload = RseDiagnosticWorkload(&workload);\n{marker}"
            ),
            "original numeric export boundary changed",
        )?;
    }
    let marker = "\\\"workload\\\":{workload}";
    if text.matches(marker).count() != 2 {
        return Err("original workload formatter changed".into());
    }
    text = text.replace(marker, "\\\"workload\\\":{rse_diagnostic_workload}");
    let helpers = "\nstruct RseDiagnosticWorkload<'a>(&'a super::workload::OptionalWorkload);\nimpl std::fmt::Display for RseDiagnosticWorkload<'_> {fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {write!(f, \"{{\\\"observer\\\":\\\"dynamic-source-copy-v1\\\",\\\"contract\\\":{}}}\", self.0)}}\n\npub(super) fn rse_dynamic_placement(budget:usize)->impl Fn() {let placement=Placement::prepare(budget); move || {if let Some(p)=&placement {p.check(); assert!(p.is_current());}}}\n";
    let marker = "\n#[cfg(test)]\nmod tests {";
    text = source::replace_once(
        &text,
        marker,
        &(helpers.to_owned() + marker),
        "sampler test module boundary changed",
    )?;
    files.insert("engine.rs".into(), text);
    let main = files.get_mut("root.rs").ok_or("missing sampler entry")?;
    *main += &format!(
        "\n#[global_allocator]\nstatic RSE_DYNAMIC_ALLOCATOR: {API}Allocator = {API}Allocator;\n"
    );
    let diagnostics = tool.join("scripts/diagnostics");
    let semantics = source::read(diagnostics.join("dynamic_buffer_semantics.rs"))?
        .replace("__RSE_PAGE_SIZE__", "observer::native_page_size()")
        .replace("__RSE_CURRENT_SOURCE__", "current_source()");
    *main += &format!(
        "\n{}\n{semantics}",
        source::read(diagnostics.join("dynamic_decoder_supplement.rs"))?
    );
    Ok(files)
}
