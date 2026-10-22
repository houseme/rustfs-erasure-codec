//! Offline syscall replay. Addresses and task IDs never enter the report.
use crate::{Args, Result, artifact};
use regex::Regex;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{BufRead, Write},
    path::Path,
    sync::LazyLock,
};

struct Patterns {
    prefix: Regex,
    call: Regex,
    resume: Regex,
    stdout: Regex,
}
static PATTERNS: LazyLock<Patterns> = LazyLock::new(|| Patterns {
    prefix: Regex::new(r"^\s*(\d+)\s+(\d+\.\d+)\s+(.*)$").unwrap(),
    call: Regex::new(r"^(brk|mmap|munmap|madvise|write)\((.*)\)\s+=\s+(.*?)\s+<(\d+\.\d+)>$")
        .unwrap(),
    resume: Regex::new(r"^<\.\.\. (\w+) resumed>(.*)$").unwrap(),
    stdout: Regex::new(r#"^1, ("(?:\\.|[^"\\])*"), (\d+)$"#).unwrap(),
});

fn nanoseconds(value: &str) -> Result<u64> {
    let (whole, fraction) = value.split_once('.').ok_or("decimal timestamp required")?;
    if !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return Err("invalid timestamp fraction".into());
    }
    let fraction = format!("{:0<9}", &fraction[..fraction.len().min(9)]);
    whole
        .parse::<u64>()?
        .checked_mul(1_000_000_000)
        .and_then(|v| v.checked_add(fraction.parse().ok()?))
        .ok_or_else(|| "timestamp overflow".into())
}
fn pointer(value: &str) -> Result<u64> {
    if value == "NULL" {
        Ok(0)
    } else {
        u64::from_str_radix(value.strip_prefix("0x").unwrap_or(value), 16).map_err(Into::into)
    }
}

struct Call {
    tid: u64,
    time: u64,
    kind: String,
    args: String,
    result: String,
    duration_ns: u64,
}
fn decode(mut reader: impl BufRead) -> Result<(Vec<Call>, usize)> {
    let mut calls = Vec::new();
    let mut pending: BTreeMap<u64, (u64, String)> = BTreeMap::new();
    let mut signals = 0;
    let mut line = String::new();
    let mut number = 0;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        number += 1;
        let fields = PATTERNS
            .prefix
            .captures(line.trim_end_matches(['\r', '\n']))
            .ok_or_else(|| format!("unrecognized syscall prefix at line {number}"))?;
        let tid = fields[1].parse()?;
        let mut time = nanoseconds(&fields[2])?;
        let mut body = fields[3].to_owned();
        if body.starts_with("--- ") || body.starts_with("+++ ") {
            signals += 1;
            continue;
        }
        if let Some(resumed) = PATTERNS.resume.captures(&body) {
            let (start, first) = pending.remove(&tid).ok_or("orphan resumed syscall")?;
            if !first.starts_with(&format!("{}(", &resumed[1])) {
                return Err("resumed syscall name mismatch".into());
            }
            time = start;
            body = first + &resumed[2];
        } else if body.contains("<unfinished ...>") {
            if pending
                .insert(tid, (time, body.replace("<unfinished ...>", "")))
                .is_some()
            {
                return Err("overlapping unfinished syscalls in one task".into());
            }
            continue;
        }
        if pending.contains_key(&tid) {
            return Err("new syscall before unfinished syscall resumed".into());
        }
        let call = PATTERNS
            .call
            .captures(&body)
            .ok_or_else(|| format!("unrecognized syscall at line {number}"))?;
        calls.push(Call {
            tid,
            time,
            kind: call[1].into(),
            args: call[2].into(),
            result: call[3].into(),
            duration_ns: nanoseconds(&call[4])?,
        });
    }
    if !pending.is_empty() {
        return Err("incomplete syscall trace".into());
    }
    calls.sort_by_key(|call| call.time);
    Ok((calls, signals))
}

// strace uses C escapes, including octal/hex bytes that JSON string decoders do
// not accept. Reconstruct bytes before UTF-8/JSON parsing of complete journals.
fn unquote(value: &str) -> Result<Vec<u8>> {
    if !value.starts_with('"') || !value.ends_with('"') {
        return Err("quoted stdout required".into());
    }
    let bytes = &value.as_bytes()[1..value.len() - 1];
    let mut output = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        i += 1;
        if byte != b'\\' {
            output.push(byte);
            continue;
        }
        let escaped = *bytes.get(i).ok_or("incomplete stdout escape")?;
        i += 1;
        match escaped {
            b'\\' | b'"' | b'\'' => output.push(escaped),
            b'n' => output.push(b'\n'),
            b'r' => output.push(b'\r'),
            b't' => output.push(b'\t'),
            b'a' => output.push(7),
            b'b' => output.push(8),
            b'f' => output.push(12),
            b'v' => output.push(11),
            b'0'..=b'7' => {
                let mut number = (escaped - b'0') as u16;
                let mut digits = 1;
                while digits < 3 && bytes.get(i).is_some_and(|b| (b'0'..=b'7').contains(b)) {
                    number = number * 8 + (bytes[i] - b'0') as u16;
                    i += 1;
                    digits += 1;
                }
                output.push(
                    number
                        .try_into()
                        .map_err(|_| "stdout byte escape overflow")?,
                );
            }
            b'x' | b'u' | b'U' => {
                let size = match escaped {
                    b'x' => 2,
                    b'u' => 4,
                    _ => 8,
                };
                let hex = std::str::from_utf8(
                    bytes
                        .get(i..i + size)
                        .ok_or("incomplete stdout hex escape")?,
                )?;
                output.push(
                    u32::from_str_radix(hex, 16)?
                        .try_into()
                        .map_err(|_| "stdout escape is not a byte")?,
                );
                i += size;
            }
            _ => {
                output.push(b'\\');
                output.push(escaped);
            }
        }
    }
    Ok(output)
}

struct Marker {
    time: u64,
    tid: u64,
    kind: &'static str,
    row: Value,
}
fn markers(calls: &[Call]) -> Result<Vec<Marker>> {
    let mut buffers: BTreeMap<u64, Vec<u8>> = BTreeMap::new();
    let mut markers = Vec::new();
    for call in calls.iter().filter(|call| call.kind == "write") {
        let Some(fields) = PATTERNS.stdout.captures(&call.args) else {
            continue;
        };
        let result = call
            .result
            .split_whitespace()
            .next()
            .ok_or("missing write outcome")?;
        if result.starts_with('-') {
            continue;
        }
        let count = result.parse::<usize>()?;
        let payload = unquote(&fields[1])?;
        if count > payload.len() {
            return Err("truncated stdout write in trace".into());
        }
        let buffer = buffers.entry(call.tid).or_default();
        buffer.extend_from_slice(&payload[..count]);
        if buffer.len() > 1_048_576 {
            return Err("bounded stdout reconstruction overflow".into());
        }
        let mut consumed = 0;
        while let Some(offset) = buffer[consumed..].iter().position(|b| *b == b'\n') {
            let line = std::str::from_utf8(&buffer[consumed..consumed + offset])?;
            consumed += offset + 1;
            for (kind, prefix) in [("timing", "RSE_TIMING "), ("sample", "RSE_SAMPLE ")] {
                if let Some(json) = line.strip_prefix(prefix) {
                    let artifact::StrictJson(row) = serde_json::from_str(json)?;
                    markers.push(Marker {
                        time: call.time,
                        tid: call.tid,
                        kind,
                        row,
                    });
                }
            }
        }
        buffer.drain(..consumed);
    }
    if markers.len() != 24 || markers.iter().any(|m| m.tid != markers[0].tid) {
        return Err("missing or ambiguous measuring-thread markers".into());
    }
    Ok(markers)
}

struct Mapping {
    start: u64,
    end: u64,
    origin: u64,
    id: u64,
    anonymous: bool,
}
#[derive(Serialize)]
struct Overlap {
    mapping_id: u64,
    offset: u64,
    bytes: u64,
    anonymous: bool,
}
fn overlap(maps: &[Mapping], start: u64, end: u64) -> Vec<Overlap> {
    maps.iter()
        .filter_map(|m| {
            let lower = start.max(m.start);
            let upper = end.min(m.end);
            (lower < upper).then(|| Overlap {
                mapping_id: m.id,
                offset: lower - m.origin,
                bytes: upper - lower,
                anonymous: m.anonymous,
            })
        })
        .collect()
}
fn remove(maps: &mut Vec<Mapping>, start: u64, end: u64) {
    let mut remaining = Vec::with_capacity(maps.len() + 1);
    for m in maps.drain(..) {
        if start >= m.end || end <= m.start {
            remaining.push(m);
            continue;
        }
        if m.start < start {
            remaining.push(Mapping { end: start, ..m });
        }
        if end < m.end {
            remaining.push(Mapping { start: end, ..m });
        }
    }
    *maps = remaining;
}

#[derive(Default, Serialize)]
struct Group {
    syscalls: BTreeMap<String, u64>,
    successful_madvise_bytes: BTreeMap<String, u64>,
    successful_munmap_bytes: u64,
    brk_grow_bytes: u64,
    brk_shrink_bytes: u64,
}
fn add(total: &mut u64, value: u64) -> Result<()> {
    *total = total.checked_add(value).ok_or("ledger counter overflow")?;
    Ok(())
}

fn analyze(reader: impl BufRead, samples: Vec<Value>) -> Result<Value> {
    if samples.len() != 12
        || samples.iter().enumerate().any(|(i, row)| {
            row["index"].as_u64() != Some(i as u64)
                || row["kind"] != if i < 3 { "warmup" } else { "sample" }
        })
    {
        return Err("complete original observation schedule required".into());
    }
    let (calls, signals) = decode(reader)?;
    let markers = markers(&calls)?;
    for (index, row) in samples.iter().enumerate() {
        let (timing, sample) = (&markers[index], &markers[12 + index]);
        let timing_fields = timing
            .row
            .as_object()
            .ok_or("invalid timing journal object")?;
        if timing.kind != "timing"
            || sample.kind != "sample"
            || &sample.row != row
            || timing_fields.iter().any(|(k, v)| row.get(k) != Some(v))
        {
            return Err("syscall/stdout/numeric journal mismatch".into());
        }
    }
    let tid = markers[0].tid;
    let mut maps = Vec::new();
    let mut next_map = 0;
    let mut brk: Option<u64> = None;
    let mut events = Vec::new();
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();
    for call in calls.iter().filter(|c| c.tid == tid && c.kind != "write") {
        let interval = if call.time < markers[0].time {
            "startup_and_warmup_0".into()
        } else if let Some(i) =
            (1..12).find(|i| call.time >= markers[i - 1].time && call.time < markers[*i].time)
        {
            format!("observation_{i}")
        } else if call.time <= markers.last().unwrap().time {
            "deferred_export".into()
        } else {
            "after_last_sample".into()
        };
        let args: Vec<_> = call.args.split(", ").collect();
        let result = call
            .result
            .split_whitespace()
            .next()
            .ok_or("missing syscall outcome")?;
        let mut event = json!({"kind":call.kind,"interval":interval,"duration_ns":call.duration_ns,"success":!result.starts_with("-1")});
        let arg = |index| {
            args.get(index)
                .copied()
                .ok_or("missing VM syscall argument")
        };
        match call.kind.as_str() {
            "brk" => {
                let query = arg(0)? == "NULL";
                event["query"] = query.into();
                event["delta_bytes"] = Value::Null;
                if event["success"] == true {
                    let address = pointer(result)?;
                    if let Some(old) = brk {
                        let delta: i64 = (address as i128 - old as i128)
                            .try_into()
                            .map_err(|_| "brk delta overflow")?;
                        event["delta_bytes"] = delta.into();
                    }
                    brk = Some(address);
                    let satisfied = query || pointer(arg(0)?)? == address;
                    event["request_satisfied"] = satisfied.into();
                    event["success"] = satisfied.into();
                } else {
                    event["request_satisfied"] = false.into();
                }
            }
            "mmap" => {
                let size = arg(1)?.parse::<u64>()?;
                let flags = arg(3)?;
                let fixed = flags.split('|').any(|v| v == "MAP_FIXED");
                let anonymous = flags.contains("MAP_ANONYMOUS");
                event["bytes"] = size.into();
                event["protection_at_mmap"] = arg(2)?.into();
                event["flags"] = flags.into();
                event["anonymous"] = anonymous.into();
                event["fixed"] = fixed.into();
                if event["success"] == true {
                    let address = pointer(result)?;
                    let end = address.checked_add(size).ok_or("mapping range overflow")?;
                    let replaced = overlap(&maps, address, end);
                    if !replaced.is_empty() && !fixed {
                        return Err("unexpected overlapping mmap".into());
                    }
                    event["mapping_id"] = next_map.into();
                    event["replaced"] = serde_json::to_value(replaced)?;
                    remove(&mut maps, address, end);
                    maps.push(Mapping {
                        start: address,
                        end,
                        origin: address,
                        id: next_map,
                        anonymous,
                    });
                    next_map += 1;
                }
            }
            "munmap" | "madvise" => {
                let start = pointer(arg(0)?)?;
                let size = arg(1)?.parse::<u64>()?;
                let end = start.checked_add(size).ok_or("mapping range overflow")?;
                event["bytes"] = size.into();
                event["matches"] = serde_json::to_value(overlap(&maps, start, end))?;
                if call.kind == "madvise" {
                    event["advice"] = arg(2)?.into();
                } else if event["success"] == true {
                    remove(&mut maps, start, end);
                }
            }
            _ => return Err("unsupported VM syscall".into()),
        }
        let group = groups.entry(interval).or_default();
        add(group.syscalls.entry(call.kind.clone()).or_default(), 1)?;
        if event["success"] == true {
            match call.kind.as_str() {
                "madvise" => add(
                    group
                        .successful_madvise_bytes
                        .entry(arg(2)?.into())
                        .or_default(),
                    event["bytes"].as_u64().unwrap(),
                )?,
                "munmap" => add(
                    &mut group.successful_munmap_bytes,
                    event["bytes"].as_u64().unwrap(),
                )?,
                "brk" => {
                    if let Some(delta) = event["delta_bytes"].as_i64() {
                        if delta >= 0 {
                            add(&mut group.brk_grow_bytes, delta as u64)?;
                        } else {
                            add(&mut group.brk_shrink_bytes, delta.unsigned_abs())?;
                        }
                    }
                }
                _ => {}
            }
        }
        events.push(event);
    }
    let task_calls = calls.iter().filter(|c| c.tid == tid).count();
    Ok(
        json!({"schema":"stream-allocator-ledger-v1","purpose":"observed VM operations and mapping lifetimes; no physical residency, exact timed-interval or throughput attribution",
        "scope":"task writing all original sampler markers; other traced tasks explicitly counted",
        "interval_scope":"between previous timing write and current timing write; includes boundary telemetry/journal costs; samples export only after all observations, first interval includes startup",
        "observations":12,"all_decoded_syscalls":calls.len(),"measuring_task_all_syscalls":task_calls,"measuring_task_vm_syscalls":events.len(),
        "coverage":{"mprotect_observed":false,"physical_residency_observed":false,"exact_api_call_boundaries":false},"other_task_syscalls":calls.len()-task_calls,
        "signal_or_exit_lines":signals,"groups":groups,"events":events,"release_approval":false,"candidate_attribution":null}),
    )
}

pub(crate) fn run(args: &Args) -> Result<()> {
    let path = Path::new(args.required("--output")?);
    if path.exists() {
        return Err("refuse to overwrite an existing ledger".into());
    }
    let report = analyze(
        std::io::BufReader::new(std::fs::File::open(args.required("--trace")?)?),
        artifact::read_json(Path::new(args.required("--samples")?))?,
    )?;
    let mut writer = std::io::BufWriter::new(
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?,
    );
    serde_json::to_writer_pretty(&mut writer, &report)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resumed_calls_and_decimal_timestamps_are_exact() {
        let input = "7 1.000000 mmap(NULL, 4096, PROT_NONE, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0 <unfinished ...>\n8 1.000001 --- SIGCHLD {si_signo=SIGCHLD} ---\n7 1.000002 <... mmap resumed>) = 0x1000 <0.000002>\n";
        let (calls, signals) = decode(input.as_bytes()).unwrap();
        assert_eq!((calls.len(), signals), (1, 1));
        assert_eq!((calls[0].time, calls[0].duration_ns), (1_000_000_000, 2000));
        for bad in [
            input.lines().take(2).collect::<Vec<_>>().join("\n"),
            input.lines().last().unwrap().to_owned(),
            input.replace("<... mmap resumed>", "<... brk resumed>"),
        ] {
            assert!(decode(bad.as_bytes()).is_err());
        }
        assert_eq!(nanoseconds("1.123456789123").unwrap(), 1_123_456_789);
    }
    #[test]
    fn stdout_escapes_recover_bytes_without_exposing_addresses() {
        assert_eq!(unquote(r#""a\n\x41\101\\\"""#).unwrap(), b"a\nAA\\\"");
        for bad in [r#""\x0""#, r#""\777""#, r#""\u9999""#] {
            assert!(unquote(bad).is_err());
        }
    }

    fn fixture() -> (Vec<Value>, Vec<String>) {
        let rows:Vec<_>=(0..12).map(|i| json!({"case":"encode_stream:10:4:16777216:4194304","index":i,"kind":if i<3 {"warmup"} else {"sample"},"iterations":6})).collect();
        let mut lines = vec![
            call(1, "brk(NULL) = 0x1000", 7),
            call(
                2,
                "mmap(NULL, 16384, PROT_NONE, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0) = 0x10000",
                7,
            ),
            call(3, "munmap(0x10000, 4096) = 0", 7),
            call(4, "munmap(0x13000, 4096) = 0", 7),
        ];
        for (i, row) in rows.iter().enumerate() {
            if i == 1 {
                for (time, body) in [
                    (20, "madvise(0x11000, 4096, MADV_DONTNEED) = 0"),
                    (
                        21,
                        "mmap(0x11000, 4096, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANONYMOUS|MAP_FIXED, -1, 0) = 0x11000",
                    ),
                    (22, "munmap(0x11000, 4096) = -1 EINVAL (Invalid argument)"),
                    (23, "madvise(0x11000, 4096, MADV_DONTNEED) = 0"),
                    (24, "brk(0x3000) = 0x3000"),
                    (25, "brk(0x2000) = 0x2000"),
                ] {
                    lines.push(call(time, body, 7));
                }
            }
            lines.push(write(10 + i * 20, "RSE_TIMING ", row));
        }
        for (i, row) in rows.iter().enumerate() {
            lines.push(write(300 + i, "RSE_SAMPLE ", row));
        }
        lines.push(call(
            400,
            "mmap(NULL, 4096, PROT_READ, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0) = 0x90000",
            9,
        ));
        (rows, lines)
    }
    fn call(time: usize, body: &str, tid: u64) -> String {
        format!("{tid} {time}.000000 {body} <0.000001>\n")
    }
    fn write(time: usize, prefix: &str, row: &Value) -> String {
        let payload = format!("{prefix}{row}\n");
        let quoted = serde_json::to_string(&payload).unwrap();
        call(
            time,
            &format!("write(1, {quoted}, {}) = {}", payload.len(), payload.len()),
            7,
        )
    }
    #[test]
    fn deferred_journals_preserve_split_mapping_identity_fixed_replacement_and_failures() {
        let (rows, lines) = fixture();
        let report = analyze(lines.concat().as_bytes(), rows).unwrap();
        assert_eq!(report["observations"], 12);
        assert_eq!(report["other_task_syscalls"], 1);
        let group = &report["groups"]["observation_1"];
        assert_eq!(group["successful_madvise_bytes"]["MADV_DONTNEED"], 8192);
        assert_eq!(group["brk_grow_bytes"], 8192);
        assert_eq!(group["brk_shrink_bytes"], 4096);
        let advice: Vec<_> = report["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "madvise")
            .collect();
        assert_eq!(advice[0]["matches"][0]["offset"], 4096);
        assert_eq!(advice[1]["matches"][0]["mapping_id"], 1);
        assert!(
            report["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["kind"] == "munmap" && e["success"] == false)
        );
        assert_eq!(report["release_approval"], false);
        assert!(report["candidate_attribution"].is_null());
        let exported = report.to_string();
        for private in ["0x11000", "0x10000", "\"tid\"", "\"time\""] {
            assert!(!exported.contains(private));
        }
    }
    #[test]
    fn complete_schedule_and_exact_numeric_journals_are_required() {
        let (rows, lines) = fixture();
        let mut missing = rows.clone();
        missing.pop();
        assert!(analyze(lines.concat().as_bytes(), missing).is_err());
        let mut changed = rows.clone();
        changed[2]["iterations"] = 7.into();
        assert!(analyze(lines.concat().as_bytes(), changed).is_err());
        let first = lines.iter().position(|s| s.contains("RSE_TIMING")).unwrap();
        let second = lines.iter().position(|s| s.contains("RSE_SAMPLE")).unwrap();
        let mut reordered = lines.clone();
        let a = lines[first].split_once("write(").unwrap();
        let b = lines[second].split_once("write(").unwrap();
        reordered[first] = format!("{}write({}", a.0, b.1);
        reordered[second] = format!("{}write({}", b.0, a.1);
        assert!(analyze(reordered.concat().as_bytes(), rows.clone()).is_err());
        assert!(analyze(lines[..lines.len() - 3].concat().as_bytes(), rows).is_err());
    }
    #[test]
    fn stdout_fragments_are_reassembled_and_truncated_writes_rejected() {
        let (rows, mut lines) = fixture();
        let index = lines.iter().position(|s| s.contains("RSE_TIMING")).unwrap();
        let prefix = PATTERNS.prefix.captures(lines[index].trim_end()).unwrap();
        let full = PATTERNS.call.captures(&prefix[3]).unwrap();
        let fields = PATTERNS.stdout.captures(&full[2]).unwrap();
        let payload = unquote(&fields[1]).unwrap();
        let mut fragments = Vec::new();
        for part in [&payload[..10], &payload[10..]] {
            let quoted = serde_json::to_string(std::str::from_utf8(part).unwrap()).unwrap();
            fragments.push(call(
                10,
                &format!("write(1, {quoted}, {}) = {}", part.len(), part.len()),
                7,
            ));
        }
        lines.splice(index..index + 1, fragments);
        assert_eq!(
            analyze(lines.concat().as_bytes(), rows.clone()).unwrap()["observations"],
            12
        );
        lines[index] = lines[index].replace("= 10 ", "= 10000 ");
        assert!(
            analyze(lines.concat().as_bytes(), rows)
                .unwrap_err()
                .to_string()
                .contains("truncated")
        );
    }
    #[test]
    fn failed_vm_calls_and_unsatisfied_brk_do_not_fabricate_residency() {
        let (rows, mut lines) = fixture();
        lines.insert(
            4,
            call(
                5,
                "madvise(NULL, 4096, MADV_DONTNEED) = -1 ENOMEM (Cannot allocate memory)",
                7,
            ),
        );
        lines.insert(5, call(6, "brk(0x9000) = 0x1000", 7));
        let report = analyze(lines.concat().as_bytes(), rows).unwrap();
        assert_eq!(report["coverage"]["physical_residency_observed"], false);
        assert_eq!(report["coverage"]["mprotect_observed"], false);
        assert!(
            report["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["kind"] == "brk" && e["request_satisfied"] == false)
        );
        assert!(
            report["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["protection_at_mmap"] == "PROT_NONE")
        );
    }
}
