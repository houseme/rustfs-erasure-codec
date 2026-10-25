use super::source;
use crate::Result;
pub const MODULE: &str = "__rse_stream_core";

pub(super) const RECONSTRUCT_BLOCK_CALLS: [&str; 2] = [
    "            plan.reconstruct_block(&mut bufs, actual_len, &data_rows, &parity_rows);",
    "            plan.reconstruct_block(\n                &mut bufs,\n                actual_len,\n                &data_rows,\n                &parity_rows,\n                &mut data_outputs,\n            );",
];

fn guard(api: &str, variant: &str) -> String {
    format!("        let mut rse_probe = crate::{MODULE}::Guard::new(\"{api}\", \"{variant}\");\n")
}

fn spans(phase: &str, groups: &[(usize, &str, &str, &str)], indent: usize) -> String {
    let mut lines = Vec::new();
    let pad = " ".repeat(indent);
    for &(role, name, kind, visible) in groups {
        lines.push(format!(
            "{pad}for (rse_i, rse_b) in {name}.iter().enumerate() {{"
        ));
        if kind == "option" {
            lines.push(format!("{pad}    if let Some(rse_b) = rse_b {{"));
            lines.push(format!("{pad}        rse_probe.span(\"{phase}\", {role}, rse_i, rse_b.as_ptr(), rse_b.len(), rse_b.capacity(), rse_b.len(), true);"));
            lines.push(format!("{pad}    }} else {{"));
            lines.push(format!("{pad}        rse_probe.span::<u8>(\"{phase}\", {role}, rse_i, core::ptr::null(), 0, 0, 0, false);"));
            lines.push(format!("{pad}    }}"));
        } else {
            let (buffer, visible) = if kind == "read" {
                ("rse_b.bytes", "rse_b.block_len")
            } else {
                ("rse_b", visible)
            };
            lines.push(format!("{pad}    rse_probe.span(\"{phase}\", {role}, rse_i, {buffer}.as_ptr(), {buffer}.len(), {buffer}.capacity(), {visible}, true);"));
        }
        lines.push(format!("{pad}}}"));
    }
    lines.join("\n") + "\n"
}

pub fn patch(text: &str) -> Result<String> {
    let current = text.contains("struct StreamReadBuffer {");
    let text = source::method(text, "    pub fn encode_stream(", |_, edit| {
        let first = "        let mut data_bufs:";
        edit.replace(
            first,
            &(guard(
                "encode_stream",
                if current {
                    "retained-prefix"
                } else {
                    "vec-views"
                },
            ) + first),
            1,
        )?;
        let mut groups = vec![
            (0, "data_bufs", if current { "read" } else { "vec" }, "0"),
            (1, "parity_bufs", "vec", "rse_b.len()"),
        ];
        edit.replace(
            "        loop {",
            &(spans("reserved", &groups, 8)
                + "        loop {\n            rse_probe.next_block();"),
            1,
        )?;
        groups[0].3 = "actual_len";
        for (stage, marker) in [
            ("read", "            if all_eof {"),
            (
                "ready",
                if current {
                    "            self.encode_sep_opt("
                } else {
                    "            // Encode (parallel codec)."
                },
            ),
            (
                "coded",
                "            // Write parity using the selected stream I/O mode.",
            ),
        ] {
            edit.replace(marker, &(spans(stage, &groups, 12) + marker), 1)?;
        }
        edit.replace(
            "        Ok(())",
            "        rse_probe.complete(None);\n        Ok(())",
            1,
        )
    })?;
    let text = source::method(&text, "    pub fn verify_stream(", |_, edit| {
        let first = "        let mut bufs:";
        edit.replace(
            first,
            &(guard(
                "verify_stream",
                if current {
                    "retained-scratch"
                } else {
                    "per-block-verify-par"
                },
            ) + first),
            1,
        )?;
        let mut groups = vec![(0, "bufs", if current { "read" } else { "vec" }, "0")];
        if current {
            groups.push((1, "verify_bufs", "vec", "rse_b.len()"));
        }
        edit.replace(
            "        loop {",
            &(spans("reserved", &groups, 8)
                + "        loop {\n            rse_probe.next_block();"),
            1,
        )?;
        groups[0].3 = "actual_len";
        for (stage, marker) in [
            ("read", "            if all_eof {"),
            ("ready", "            let valid = self"),
            ("coded", "            if !valid {"),
        ] {
            edit.replace(marker, &(spans(stage, &groups, 12) + marker), 1)?;
        }
        edit.replace(
            "                return Ok(false);",
            "                rse_probe.complete(Some(false));\n                return Ok(false);",
            1,
        )?;
        edit.replace(
            "        Ok(true)",
            "        rse_probe.complete(Some(true));\n        Ok(true)",
            1,
        )
    })?;
    source::method(
        &text,
        if current {
            "    pub fn reconstruct_stream_from<"
        } else {
            "    pub fn reconstruct_stream("
        },
        |method, edit| {
            let first = "        let mut bufs:";
            edit.replace(
                first,
                &(guard(
                    "reconstruct_stream",
                    if current {
                        "planned-retained"
                    } else {
                        "option-transfer"
                    },
                ) + first),
                1,
            )?;
            let mut groups = vec![(0, "bufs", "vec", "rse_b.len()")];
            if !current {
                groups.push((1, "reconstruct_bufs", "option", ""));
            }
            if current {
                let marker = "        let (all_eof, mut actual_len) =";
                edit.replace(
                    marker,
                    &(spans("reserved", &groups, 8) + "        rse_probe.next_block();\n" + marker),
                    1,
                )?;
                edit.replace("        if all_eof {\n            return Ok(());\n        }",&(spans("read",&groups,8)+"        if all_eof {\n            rse_probe.complete(None);\n            return Ok(());\n        }"),1)?;
                let mut calls = RECONSTRUCT_BLOCK_CALLS
                    .iter()
                    .copied()
                    .filter(|call| method.contains(call));
                let marker = calls.next().ok_or("stream reconstruction call changed")?;
                if calls.next().is_some() {
                    return Err("ambiguous stream reconstruction calls".into());
                }
                edit.replace(
                    marker,
                    &(spans("ready", &groups, 12) + marker + "\n" + &spans("coded", &groups, 12)),
                    1,
                )?;
                let marker = "            let (all_eof, next_len) =";
                edit.replace(
                    marker,
                    &("            rse_probe.next_block();\n".to_owned() + marker),
                    1,
                )?;
                let marker = "            if all_eof {";
                edit.replace(marker, &(spans("read", &groups, 12) + marker), 1)?;
            } else {
                edit.replace(
                    "        loop {",
                    &(spans("reserved", &groups, 8)
                        + "        loop {\n            rse_probe.next_block();"),
                    1,
                )?;
                for (stage, marker) in [
                    ("read", "            if all_eof {"),
                    (
                        "ready",
                        "            self.reconstruct(&mut reconstruct_bufs)",
                    ),
                    (
                        "coded",
                        "            // Write recovered data into missing shards' cursors.",
                    ),
                ] {
                    edit.replace(marker, &(spans(stage, &groups, 12) + marker), 1)?;
                }
                let marker = "            for idx in 0..total {\n                if let Some(buf) = reconstruct_bufs[idx].take() {\n                    bufs[idx] = buf;\n                }\n            }";
                edit.replace(
                    marker,
                    &(marker.replace(
                        "bufs[idx] = buf;",
                        "bufs[idx] = buf;\n                    rse_probe.replaced(0, idx);",
                    ) + "\n"
                        + &spans("recycled", &groups, 12)),
                    1,
                )?;
            }
            edit.replace(
                "\n        Ok(())\n",
                "\n        rse_probe.complete(None);\n        Ok(())\n",
                1,
            )
        },
    )
}

pub fn verify_par(text: &str) -> Result<String> {
    source::method(text, "    pub fn verify_par<T>", |_, edit| {
        let marker = "        let mut scratch: SmallVec<[F::Elem; VERIFY_INLINE_SCRATCH_ELEMS]> =";
        edit.replace(
            marker,
            &(guard("verify_par", "smallvec-per-block") + marker),
            1,
        )?;
        let marker = "        scratch.resize(scratch_len, F::zero());";
        edit.replace(marker,&("        rse_probe.span(\"reserved\", 0, 0, scratch.as_ptr(), scratch.len(), scratch.capacity(), 0, true);\n".to_owned()+marker+"\n        rse_probe.span(\"initialized\", 0, 0, scratch.as_ptr(), scratch.len(), scratch.capacity(), scratch.len(), true);\n        let rse_scratch_range = (scratch.as_ptr(), scratch.len(), scratch.capacity());"),1)?;
        edit.replace("        self.verify_with_buffer_par(slices, &mut buffer_views)","        let rse_result = self.verify_with_buffer_par(slices, &mut buffer_views);\n        rse_probe.span(\"coded\", 0, 0, rse_scratch_range.0, rse_scratch_range.1, rse_scratch_range.2, rse_scratch_range.1, true);\n        if let Ok(valid) = rse_result { rse_probe.complete(Some(valid)); }\n        rse_result",1)
    })
}

pub fn verify(text: &str) -> Result<String> {
    source::method(
        text,
        "    pub fn verify<T: AsRef<[F::Elem]>>(&self, slices: &[T]) -> Result<bool, Error> {",
        |body, edit| {
            let marker =
                "        let mut scratch: SmallVec<[F::Elem; VERIFY_INLINE_SCRATCH_ELEMS]> =";
            edit.replace(marker,&("        let mut rse_probe = crate::__rse_verify_scratch::Guard::new(scratch_len);\n".to_owned()+marker),1)?;
            let marker = "        scratch.resize(scratch_len, F::zero());";
            edit.replace(marker,&("        rse_probe.reserved(scratch.as_ptr(), scratch.capacity(), !scratch.spilled());\n".to_owned()+marker+"\n        rse_probe.initialized(scratch.as_ptr(), scratch.len(), scratch.capacity());"),1)?;
            let (returned, computation) = if body
                .contains("        self.check_classic_parity(data, to_check, &mut buffer_views)")
            {
                (
                    "        self.check_classic_parity(data, to_check, &mut buffer_views)",
                    "self.check_classic_parity(data, to_check, &mut buffer_views)?",
                )
            } else {
                (
                    "        Ok(self.check_some_slices_with_buffer_raw(&parity_rows, data, to_check, &mut buffer_views))",
                    "self.check_some_slices_with_buffer_raw(&parity_rows, data, to_check, &mut buffer_views)",
                )
            };
            edit.replace(
                returned,
                &format!("        let valid = {computation};\n        rse_probe.verified(valid);\n        Ok(valid)"),
                1,
            )
        },
    )
}
