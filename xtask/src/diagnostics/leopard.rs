use super::source;
use crate::Result;
use std::collections::BTreeMap;
const LIB: &str = "crate::__rse_dynamic_buffers::";
pub const FILES: [&str; 4] = [
    "src/core/leopard_gf8/work.rs",
    "src/core/leopard_gf8/decode.rs",
    "src/core/leopard_gf16/work.rs",
    "src/core/leopard_gf16/decode.rs",
];
fn work(text: &str, gf16: bool) -> Result<String> {
    let (typename, length) = if gf16 {
        ("FlatWork16", "len_elems")
    } else {
        ("FlatWork", "len")
    };
    let marker = format!("impl {typename} {{");
    let helper = format!(
        "\n    pub(crate) fn __rse_decode_view(&self, stage: &'static str, active: usize) {{\n        {LIB}work(\"leopard_work\",stage,self.ptr,self.{length}, [self.lanes,active,self.lane_len]);\n    }}\n"
    );
    source::replace_once(
        text,
        &marker,
        &(marker.clone() + &helper),
        "Leopard work implementation changed",
    )
}
fn vector(name: &str, role: &str, stage: &str, indent: usize) -> String {
    format!(
        "{}{LIB}span(\"{role}\",\"{stage}\",0,{name}.as_ptr(),{name}.len(),Some({name}.capacity()),\"heap\",true);\n",
        " ".repeat(indent)
    )
}
fn locator(stage: &str) -> String {
    format!(
        "    {LIB}span(\"leopard_locator\",\"{stage}\",0,err_locs.as_ptr(),err_locs.len(),Some(err_locs.len()),\"inline\",true);\n"
    )
}
fn gf8(text: &str) -> Result<String> {
    let declaration = if text.contains("struct DecodeBuffers {") {
        "fn reconstruct_with_resources("
    } else {
        "pub(crate) fn reconstruct_with_tables("
    };
    source::top(text, declaration, |edit| {
        let marker = "    let mut offset = 0usize;";
        edit.replace(
            marker,
            &(locator("leased")
                + "    work.__rse_decode_view(\"leased\",0);\n"
                + &vector("scratch", "leopard_scratch", "leased", 4)
                + marker),
            1,
        )?;
        let marker = "        let size = end - offset;";
        edit.replace(
            marker,
            &(marker.to_owned()
                + "\n        work.__rse_decode_view(\"chunk_ready\",size);\n"
                + &vector("scratch", "leopard_scratch", "chunk_ready", 8)),
            1,
        )?;
        let marker = "        offset = end;";
        edit.replace(
            marker,
            &("        work.__rse_decode_view(\"chunk_coded\",size);\n".to_owned()
                + &vector("scratch", "leopard_scratch", "chunk_coded", 8)
                + marker),
            1,
        )?;
        let marker = "\n    Ok(())\n}";
        edit.replace(
            marker,
            &("\n".to_owned()
                + &locator("returned")
                + "    work.__rse_decode_view(\"returned\",0);\n"
                + &vector("scratch", "leopard_scratch", "returned", 4)
                + marker),
            1,
        )
    })
}
fn gf16(text: &str) -> Result<String> {
    let separate = regex::Regex::new(
        r"(?m)^( +)if out_scratch\.len\(\) < size \{\n +out_scratch\.resize\(size, 0\);\n +\}",
    )?;
    let shared = regex::Regex::new(r"(?m)^( +)let out_scratch = &mut \*scratch;")?;
    let fused = regex::Regex::new(concat!(
        r"(?m)^( +)for (?:&\(shard_idx, work_idx, inv_err\) in &missing|",
        r"\(k, &\(_, work_idx, inv_err\)\) in missing\.iter\(\)\.enumerate\(\)) \{\n",
        r" +super::fused::work_mul_to_user\(\n",
        r" +&work\.lane\(work_idx\)\[\.\.size\],\n",
        r" +&mut (?:outputs\[shard_idx\]\[offset \* 2\.\.end \* 2\]|",
        r"group_out\[k\]\[\(offset - start\) \* 2\.\.\(end - start\) \* 2\]),\n",
        r" +(?:&mut scratch\[\.\.size\]|conversion_scratch\(scratch, size\)),\n",
        r" +inv_err,\n +tables,\n +\);",
    ))?;
    let (separate_output, fused_output) = match (
        separate.captures_iter(text).count(),
        shared.captures_iter(text).count(),
        fused.captures_iter(text).count(),
    ) {
        (2, 0, 0) => (true, false),
        (0, 2, 0) => (false, false),
        (0, 0, 2)
            if text.matches("super::fused::work_mul_to_user(").count() == 2
                && text
                    .matches("for &(shard_idx, work_idx, inv_err) in &missing {")
                    .count()
                    == 1
                && text
                    .matches("for (k, &(_, work_idx, inv_err)) in missing.iter().enumerate() {")
                    .count()
                    == 1 =>
        {
            (false, true)
        }
        _ => return Err("GF16 output scratch boundary changed or is ambiguous".into()),
    };
    if text.contains("struct DecodeBuffers {")
        && separate_output != text.contains("    out_scratch: Vec<u16>,")
    {
        return Err("GF16 output scratch owner changed".into());
    }
    let mut text = text.to_owned();
    if text.contains("struct DecodeBuffers {") {
        text = source::top(&text, "fn with_decode_buffers<", |edit| {
            let views = |stage| {
                format!(
                    "    buffers.work.__rse_decode_view(\"{stage}\",0);\n{}{}",
                    vector("buffers.scratch", "leopard_scratch", stage, 4),
                    if separate_output {
                        vector("buffers.out_scratch", "leopard_out_scratch", stage, 4)
                    } else {
                        String::new()
                    }
                )
            };
            let marker = "    let result = operation(&mut buffers);";
            edit.replace(
                marker,
                &(views("leased") + marker + "\n" + &views("returned")),
                1,
            )
        })?;
    }
    text = source::top(&text, "fn compute_reconstruct_chunk(", |edit| {
        let marker = "    // Step 1:";
        edit.replace(
            marker,
            &(locator("chunk_ready")
                + "    work.__rse_decode_view(\"chunk_ready\",size);\n"
                + &vector("scratch", "leopard_scratch", "chunk_ready", 4)
                + marker),
            1,
        )?;
        let marker = "\n    Ok(())\n}";
        edit.replace(
            marker,
            &("\n".to_owned()
                + &locator("chunk_coded")
                + "    work.__rse_decode_view(\"chunk_coded\",size);\n"
                + &vector("scratch", "leopard_scratch", "chunk_coded", 4)
                + marker),
            1,
        )
    })?;
    let expected = if separate_output {
        &separate
    } else if fused_output {
        &fused
    } else {
        &shared
    };
    let positions: Vec<_> = expected
        .captures_iter(&text)
        .map(|c| (c.get(0).expect("whole match").range(), c[1].len()))
        .collect();
    if positions.len() != 2 {
        return Err("GF16 output scratch boundary changed or is ambiguous".into());
    }
    for (range, indent) in positions.iter().rev() {
        if fused_output {
            text.insert_str(
                range.start,
                &vector("scratch", "leopard_out_scratch", "chunk_ready", *indent),
            );
        } else {
            text.insert_str(
                range.end,
                &format!(
                    "\n{}",
                    vector("out_scratch", "leopard_out_scratch", "chunk_ready", *indent)
                        .trim_end_matches('\n')
                ),
            );
        }
    }
    Ok(text)
}
pub fn patch(original: &BTreeMap<String, String>) -> Result<BTreeMap<String, String>> {
    let get = |name| {
        original
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| format!("missing Leopard source: {name}"))
    };
    Ok(BTreeMap::from([
        (FILES[0].into(), work(get(FILES[0])?, false)?),
        (FILES[1].into(), gf8(get(FILES[1])?)?),
        (FILES[2].into(), work(get(FILES[2])?, true)?),
        (FILES[3].into(), gf16(get(FILES[3])?)?),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gf16_observer_uses_the_shared_reader_for_lf_crlf_and_cr_checkouts() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap();
        let canonical = source::read(root.join(FILES[3])).unwrap();
        let expected = gf16(&canonical).unwrap();
        let directory = crate::artifact::snapshot::tests::Directory::new();
        let input = directory.0.join("decode.rs");
        for newline in ["\n", "\r\n", "\r"] {
            let bytes = canonical.replace('\n', newline);
            std::fs::write(&input, bytes).unwrap();
            let loaded = source::read(&input).unwrap();
            assert_eq!(loaded, canonical);
            assert_eq!(gf16(&loaded).unwrap(), expected);
        }
    }

    #[test]
    fn gf16_observer_accepts_fused_shared_and_separate_scratch_and_rejects_ambiguity() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap();
        let current = source::read(root.join(FILES[3])).unwrap();
        let alias = "let out_scratch = &mut *scratch;";
        let observed = gf16(&current).unwrap();
        assert!(!observed.contains("buffers.out_scratch"));
        assert_eq!(observed.matches("span(\"leopard_out_scratch\"").count(), 2);
        // Current serial code uses the no-std-aware helper while the parallel
        // std-only path still uses a direct slice. Older snapshots use direct
        // slices in both paths; both shapes must remain observable.
        let previous_fused = current.replacen(
            "conversion_scratch(scratch, size),\n                    inv_err,",
            "&mut scratch[..size],\n                    inv_err,",
            1,
        );
        assert_ne!(previous_fused, current);
        assert_eq!(
            gf16(&previous_fused)
                .unwrap()
                .matches("span(\"leopard_out_scratch\"")
                .count(),
            2
        );
        assert_eq!(
            observed
                .matches(
                    "span(\"leopard_out_scratch\",\"chunk_ready\",0,scratch.as_ptr(),scratch.len()"
                )
                .count(),
            2
        );

        let loops = regex::Regex::new(concat!(
            r"(?m)^( +)(for (?:&\(shard_idx, work_idx, inv_err\) in &missing|",
            r"\(k, &\(_, work_idx, inv_err\)\) in missing\.iter\(\)\.enumerate\(\)) \{)",
        ))
        .unwrap();
        assert_eq!(loops.captures_iter(&current).count(), 2);
        let legacy = current.replace(
            "super::fused::work_mul_to_user(",
            "legacy_work_mul_to_user(",
        );
        let legacy = loops
            .replace_all(&legacy, |capture: &regex::Captures<'_>| {
                format!("{}{alias}\n{}{}", &capture[1], &capture[1], &capture[2])
            })
            .to_string();
        let observed = gf16(&legacy).unwrap();
        assert!(!observed.contains("buffers.out_scratch"));
        assert_eq!(observed.matches("span(\"leopard_out_scratch\"").count(), 2);

        let shared = regex::Regex::new(r"(?m)^( +)let out_scratch = &mut \*scratch;").unwrap();
        let separate = shared.replace_all(&legacy, |capture: &regex::Captures<'_>| {
            let indent = &capture[1];
            format!("{indent}if out_scratch.len() < size {{\n{indent}    out_scratch.resize(size, 0);\n{indent}}}")
        }).replace("    scratch: Vec<u16>,", "    scratch: Vec<u16>,\n    out_scratch: Vec<u16>,");
        let observed = gf16(&separate).unwrap();
        assert!(observed.contains("buffers.out_scratch"));
        assert_eq!(observed.matches("span(\"leopard_out_scratch\"").count(), 4);
        for source in [&current, &previous_fused, &legacy, &separate] {
            let observed = gf16(source).unwrap();
            let original: Vec<_> = observed
                .lines()
                .filter(|line| {
                    !line.trim().is_empty()
                        && !line.contains("crate::__rse_dynamic_buffers::")
                        && !line.contains(".__rse_decode_view(")
                })
                .collect();
            assert_eq!(
                original,
                source
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .collect::<Vec<_>>(),
                "observation must not change production code"
            );
        }
        for invalid in [
            current.replacen("conversion_scratch(scratch, size),\n                    inv_err,", "conversion_scratch(other, size),\n                    inv_err,", 1),
            current.replacen("&mut scratch[..size],\n                            inv_err,", "&mut other[..size],\n                            inv_err,", 1),
            current.replacen("for &(shard_idx, work_idx, inv_err) in &missing {", &format!("{alias}\n            for &(shard_idx, work_idx, inv_err) in &missing {{"), 1),
            legacy.replace(alias, "let out_scratch = unknown();"),
            legacy.replacen(alias, &format!("{alias}\n            {alias}"), 1),
            legacy.replacen(alias, &format!("{alias}\n            if out_scratch.len() < size {{\n                out_scratch.resize(size, 0);\n            }}"), 1),
        ] {
            assert!(gf16(&invalid).is_err());
        }
    }
}
