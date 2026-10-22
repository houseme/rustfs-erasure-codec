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
    let mut text = text.to_owned();
    if text.contains("struct DecodeBuffers {") {
        text = source::top(&text, "fn with_decode_buffers<", |edit| {
            let views = |stage| {
                format!(
                    "    buffers.work.__rse_decode_view(\"{stage}\",0);\n{}{}",
                    vector("buffers.scratch", "leopard_scratch", stage, 4),
                    vector("buffers.out_scratch", "leopard_out_scratch", stage, 4)
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
    let regex = regex::Regex::new(
        r"(?m)^( +)if out_scratch\.len\(\) < size \{\n +out_scratch\.resize\(size, 0\);\n +\}",
    )?;
    let positions: Vec<_> = regex
        .captures_iter(&text)
        .map(|c| (c.get(0).expect("whole match").range(), c[1].len()))
        .collect();
    if positions.len() != 2 {
        return Err("GF16 output scratch initialization changed".into());
    }
    for (range, indent) in positions.iter().rev() {
        text.insert_str(
            range.end,
            &format!(
                "\n{}",
                vector("out_scratch", "leopard_out_scratch", "chunk_ready", *indent)
                    .trim_end_matches('\n')
            ),
        );
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
