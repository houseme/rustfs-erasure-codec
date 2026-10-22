use super::source;
use crate::Result;
use std::path::Path;

pub fn native(kind: &str, tool: &Path) -> Result<String> {
    let diagnostics = tool.join("scripts/diagnostics");
    let engine = source::read(tool.join("scripts/release-sampling/engine.rs"))?;
    let (file, compact) = match kind {
        "views" => ("stream_views.rs", false),
        "verify" => ("verify_scratch.rs", false),
        "stream" => ("stream_core.rs", false),
        "dynamic" => ("dynamic_buffers.rs", true),
        _ => return Err("unknown native diagnostic template".into()),
    };
    let mut text = source::read(diagnostics.join(file))?;
    let page = source::read(diagnostics.join("page_residency.rs"))?;
    let cfg = if compact {
        "\n#[cfg(target_os=\"linux\")]\n"
    } else {
        "\n#[cfg(target_os = \"linux\")]\n"
    };
    let mut helpers = String::new();
    for declaration in ["fn read_bounded", "fn process_faults"] {
        helpers += cfg;
        helpers += source::block(&engine, declaration)?;
        helpers += "\n";
    }
    if kind == "views" {
        let marker = "\n#[cfg(test)]\nmod tests {";
        text = source::replace_once(
            &text,
            marker,
            &(helpers + marker),
            "stream diagnostic test boundary changed",
        )?;
    } else if kind == "dynamic" {
        let core = source::read(diagnostics.join("stream_core.rs"))?;
        text += &format!("\n{}\n{helpers}", source::block(&core, "fn faults()")?);
    } else {
        if kind == "verify" {
            text += "\n";
        }
        text += &helpers;
    }
    Ok(text.replacen(
        "mod page_residency;",
        &format!("mod page_residency {{\n{page}\n}}"),
        1,
    ))
}

pub fn telemetry(root: &Path, tool: &Path) -> Result<String> {
    let diagnostics = root.join("scripts/diagnostics");
    let mut modules = String::new();
    for name in ["engine", "layout", "process", "pattern", "workload"] {
        modules += &format!(
            "#[path=\"{}/scripts/release-sampling/{name}.rs\"] mod {name};\n",
            root.display()
        );
    }
    modules = modules.replace("mod engine;", "mod sampling;");
    for (name, kind) in [
        ("stream_views", "views"),
        ("verify_scratch", "verify"),
        ("stream_core", "stream"),
        ("dynamic_buffers", "dynamic"),
    ] {
        let text = native(kind, tool)?;
        let text = if kind == "dynamic" {
            text.replace("crate::ShardSlot", "rustfs_erasure_codec::ShardSlot")
        } else {
            text
        };
        modules += &format!("mod {name} {{\n{text}\n}}\n");
    }
    let original = source::read(root.join("src/tests/mod.rs"))?;
    let original = source::block(&original, "fn shard_index_pattern")?.replacen(
        "fn shard_index_pattern",
        "fn original_pattern",
        1,
    );
    let mut result = format!(
        "{}\n{modules}\n{original}",
        source::read(diagnostics.join("telemetry_allocations.rs"))?
    );
    for file in [
        "stream_view_allocations.rs",
        "verify_scratch_allocations.rs",
        "stream_core_allocations.rs",
        "dynamic_buffer_allocations.rs",
    ] {
        result += "\n";
        result += &source::read(diagnostics.join(file))?;
    }
    Ok(result)
}
