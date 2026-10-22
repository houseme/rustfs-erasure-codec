//! Streaming, address-free software-event accounting. Never release approval.
use crate::{Args, Result, artifact};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::{BufRead, Write},
    path::Path,
    sync::LazyLock,
};

const BAD_RECORDS: [&str; 4] = [
    "PERF_RECORD_LOST",
    "PERF_RECORD_LOST_SAMPLES",
    "PERF_RECORD_THROTTLE",
    "PERF_RECORD_UNTHROTTLE",
];
const MAX_FRAMES: usize = 256;
const MAX_STACKS: usize = 200_000;
const MAX_UNIQUE_FRAMES: usize = 200_000;

struct Patterns {
    header: Regex,
    frame: Regex,
    inline: Regex,
    address: Regex,
    source: Regex,
    registry: Regex,
    unresolved: Regex,
    worker: Regex,
    bad_log: Regex,
    stats: Regex,
}
static PATTERNS: LazyLock<Patterns> = LazyLock::new(|| {
    Patterns {
    header: Regex::new(r"^\s*(\d+)\.(\d{1,9}):\s+(\d+)\s+(\S+):\s*$").unwrap(),
    frame: Regex::new(r"^\s+([0-9a-fA-F]+)\s+(.+)\s+\((.*)\)\s*$").unwrap(),
    inline: Regex::new(r"^\s+([0-9a-fA-F]+)\s+(.+)\s*$").unwrap(),
    address: Regex::new(r"^(?:0x)?[0-9a-fA-F]+$").unwrap(),
    source: Regex::new(r"^(.+):(\d+)(?: \(discriminator \d+\))?$").unwrap(),
    registry: Regex::new(r"/registry/src/[^/]+/(.+)$").unwrap(),
    unresolved: Regex::new(r"^\s*(?:\S+\[[0-9a-fA-F]+\]|\?\?:[?0]|\[unknown\])(?: \(inlined\))?\s*$").unwrap(),
    worker: Regex::new(r"^rse_release_sampling-[0-9a-f]+$").unwrap(),
    bad_log: Regex::new(r"(?i)failed to|couldn.t decompress|data corruption|bad address|lost \d+ (?:chunks|samples)|check io/cpu overload").unwrap(),
    stats: Regex::new(r"^\s*([A-Z0-9_]+) events:\s+(\d+)(?:\s+\(\s*\d+(?:\.\d+)?%\))?\s*$").unwrap(),
}
});

#[derive(Serialize, Deserialize)]
struct Anchors {
    path: String,
    sha256: String,
    read_initialization: u64,
    parity_initialization: u64,
    writer_call: u64,
    encode_return: u64,
}

fn anchors(text: &str) -> Result<Anchors> {
    let start = text
        .find("pub fn encode_stream(")
        .ok_or("stream initialization source anchors changed")?;
    let end = text[start..]
        .find("pub fn verify_stream(")
        .map(|i| i + start)
        .ok_or("stream initialization source anchors changed")?;
    let reads: Vec<_> = text
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains("core::cmp::Ordering::Less => buf.resize(max_len, 0)"))
        .map(|(i, _)| i as u64 + 1)
        .collect();
    let first = text[..start].bytes().filter(|b| *b == b'\n').count() as u64 + 1;
    let mut parity = Vec::new();
    let mut writes = Vec::new();
    let mut closes = Vec::new();
    for (index, line) in text[start..end].lines().enumerate() {
        let number = first + index as u64;
        if line.contains("buf.resize(actual_len, 0);") {
            parity.push(number);
        }
        if line.trim() == "write_block_with_mode(" {
            writes.push(number);
        }
        if line == "    }" {
            closes.push(number);
        }
    }
    if reads.len() != 1 || parity.len() != 1 || writes.len() != 1 || closes.is_empty() {
        return Err("stream initialization source anchors changed".into());
    }
    let mut sha256 = String::with_capacity(64);
    {
        use std::fmt::Write;
        for byte in Sha256::digest(text.as_bytes()) {
            write!(sha256, "{byte:02x}").unwrap();
        }
    }
    Ok(Anchors {
        path: "src/core/stream.rs".into(),
        sha256,
        read_initialization: reads[0],
        parity_initialization: parity[0],
        writer_call: writes[0],
        encode_return: *closes.last().unwrap(),
    })
}

fn public_source(value: &str) -> Option<(String, u64)> {
    let trimmed = value.trim();
    let value = trimmed.strip_suffix(" (inlined)").unwrap_or(trimmed);
    let captures = PATTERNS.source.captures(value)?;
    let mut name = captures[1].to_owned();
    let line = captures[2].parse().ok()?;
    if ["??", "[unknown]"].contains(&name.as_str()) {
        return None;
    }
    if let Some(registry) = PATTERNS.registry.captures(&name) {
        return Some((format!("registry/{}", &registry[1]), line));
    }
    let mut found = false;
    for prefix in ["library/", "tests/", "benches/", "src/"] {
        if name.starts_with(prefix) {
            found = true;
            break;
        }
        if let Some((_, rest)) = name.rsplit_once(&format!("/{prefix}")) {
            name = format!("{prefix}{rest}");
            found = true;
            break;
        }
    }
    if !found {
        name = Path::new(&name).file_name()?.to_string_lossy().into_owned();
    }
    Some((name, line))
}

fn function_name(symbol: &str) -> String {
    let mut name = String::with_capacity(symbol.len());
    let mut depth = 0;
    for ch in symbol.chars() {
        if ch == '<' {
            depth += 1;
        } else if ch == '>' && depth > 0 {
            depth -= 1;
        } else if depth == 0 {
            name.push(ch);
        }
    }
    name
}

#[derive(Clone, Hash, PartialEq, Eq, Serialize)]
struct Frame {
    symbol: String,
    module: Option<String>,
    source: Option<String>,
    line: Option<u64>,
}

fn classify(frames: &[Frame], anchors: &Anchors) -> &'static str {
    let location = |line| {
        frames
            .iter()
            .any(|f| f.source.as_deref() == Some(&anchors.path) && f.line == Some(line))
    };
    if location(anchors.read_initialization) {
        return "read_scratch_initialization";
    }
    if location(anchors.parity_initialization) {
        return "parity_scratch_initialization";
    }
    let names: Vec<_> = frames.iter().map(|f| function_name(&f.symbol)).collect();
    let stream: HashSet<_> = frames
        .iter()
        .zip(&names)
        .filter(|(f, name)| {
            name.starts_with("rustfs_erasure_codec::core::stream::")
                || f.source.as_deref() == Some(&anchors.path)
        })
        .map(|(_, name)| name.rsplit("::").next().unwrap())
        .collect();
    if [
        "read_one_stream",
        "prepare_read_buffer",
        "read_block_par",
        "read_block_with_mode",
    ]
    .iter()
    .any(|v| stream.contains(v))
    {
        return "stream_read_path";
    }
    if ["write_block", "write_block_par", "write_block_with_mode"]
        .iter()
        .any(|v| stream.contains(v))
        || location(anchors.writer_call)
    {
        return "stream_write_path";
    }
    let freeing = names.iter().any(|name| {
        [
            "__rust_dealloc",
            "dealloc",
            "__GI___libc_free",
            "__libc_free",
            "_int_free",
            "free",
            "__munmap",
            "munmap",
            "madvise",
        ]
        .contains(&name.rsplit("::").next().unwrap())
    });
    let vm_release = frames.iter().zip(&names).any(|(frame, name)| {
        frame.module.as_deref() == Some("[kernel.kallsyms]")
            && ["zap_", "unmap_", "free_pages_", "tlb_finish_"]
                .iter()
                .any(|p| name.starts_with(p))
    });
    if stream.contains("encode_stream") {
        if freeing || vm_release && location(anchors.encode_return) {
            return "stream_encode_cleanup";
        }
        return "other_stream_encode";
    }
    if frames.iter().zip(&names).any(|(f, name)| {
        f.source.as_ref().is_some_and(|p| p.starts_with("src/"))
            && name.rsplit("::").next().unwrap().starts_with("rust_gfni_")
    }) {
        return "codec_arithmetic";
    }
    "other_or_unattributed"
}

#[derive(Default, Serialize)]
struct Totals {
    samples: u64,
    period_sum: u64,
}
impl Totals {
    fn add(&mut self, samples: u64, periods: u64) -> Result<()> {
        self.samples = self
            .samples
            .checked_add(samples)
            .ok_or("sample counter overflow")?;
        self.period_sum = self
            .period_sum
            .checked_add(periods)
            .ok_or("period counter overflow")?;
        Ok(())
    }
}
#[derive(Serialize)]
struct Stack {
    frame_ids: Vec<usize>,
    samples: u64,
    period_sum: u64,
    role: &'static str,
}
struct Sample {
    period: u64,
    frames: Vec<Frame>,
}
#[derive(Default)]
struct Inventory {
    frames: Vec<Frame>,
    frame_ids: HashMap<Frame, usize>,
    stacks: HashMap<Vec<usize>, Stack>,
    periods: BTreeMap<u64, u64>,
    totals: Totals,
}
impl Inventory {
    fn flush(&mut self, sample: Option<Sample>, anchors: &Anchors) -> Result<()> {
        let Some(mut sample) = sample else {
            return Ok(());
        };
        if sample.frames.is_empty() {
            sample.frames.push(Frame {
                symbol: "[unresolved-callchain]".into(),
                module: None,
                source: None,
                line: None,
            });
        }
        let role = classify(&sample.frames, anchors);
        let mut ids = Vec::with_capacity(sample.frames.len());
        for frame in sample.frames {
            let id = if let Some(id) = self.frame_ids.get(&frame) {
                *id
            } else {
                if self.frames.len() >= MAX_UNIQUE_FRAMES {
                    return Err("bounded unique frame inventory overflow".into());
                }
                let id = self.frames.len();
                self.frame_ids.insert(frame.clone(), id);
                self.frames.push(frame);
                id
            };
            ids.push(id);
        }
        if !self.stacks.contains_key(&ids) && self.stacks.len() >= MAX_STACKS {
            return Err("bounded stack inventory overflow".into());
        }
        let stack = self.stacks.entry(ids.clone()).or_insert_with(|| Stack {
            frame_ids: ids,
            samples: 0,
            period_sum: 0,
            role,
        });
        stack.samples = stack
            .samples
            .checked_add(1)
            .ok_or("sample counter overflow")?;
        stack.period_sum = stack
            .period_sum
            .checked_add(sample.period)
            .ok_or("period counter overflow")?;
        let count = self.periods.entry(sample.period).or_default();
        *count = count.checked_add(1).ok_or("sample counter overflow")?;
        self.totals.add(1, sample.period)?;
        Ok(())
    }
}

#[derive(Serialize)]
struct Period {
    period: u64,
    samples: u64,
}
#[derive(Serialize)]
struct Report<'a> {
    schema: &'static str,
    purpose: &'static str,
    event: &'a str,
    record_types: BTreeMap<String, u64>,
    samples: u64,
    period_sum: u64,
    actual_period_histogram: Vec<Period>,
    unresolved_leaf_samples: u64,
    empty_callchain_samples: u64,
    source_anchors: Anchors,
    roles: BTreeMap<&'static str, Totals>,
    frames: Vec<Frame>,
    stacks: Vec<Stack>,
    release_approval: bool,
    candidate_attribution: Option<()>,
    recording_quality: serde_json::Value,
}

fn audit<'a>(
    mut reader: impl BufRead,
    records: BTreeMap<String, u64>,
    anchors: Anchors,
    event: &'a str,
) -> Result<Report<'a>> {
    if !["minor-faults:u", "cpu-clock", "cpu-clock:u"].contains(&event) {
        return Err("unsupported software profile event".into());
    }
    if BAD_RECORDS
        .iter()
        .any(|k| records.get(*k).is_some_and(|v| *v > 0))
    {
        return Err("lost or throttled software profile; retain raw data".into());
    }
    let expected = *records
        .get("PERF_RECORD_SAMPLE")
        .filter(|v| **v > 0)
        .ok_or("raw sample count is required")?;
    let mut inventory = Inventory::default();
    let mut current: Option<Sample> = None;
    let mut last_time = 0;
    let mut buffer = String::new();
    let mut line_number = 0;
    loop {
        buffer.clear();
        if reader.read_line(&mut buffer)? == 0 {
            break;
        }
        line_number += 1;
        let line = buffer.trim_end_matches(['\r', '\n']);
        if line.trim().is_empty() {
            inventory.flush(current.take(), &anchors)?;
            continue;
        }
        if let Some(c) = PATTERNS.header.captures(line) {
            inventory.flush(current.take(), &anchors)?;
            let fraction = format!("{:0<9}", &c[2]);
            let timestamp = c[1]
                .parse::<u64>()?
                .checked_mul(1_000_000_000)
                .and_then(|v| v.checked_add(fraction.parse().ok()?))
                .ok_or("timestamp overflow")?;
            let period = c[3].parse::<u64>()?;
            if timestamp < last_time || period == 0 || &c[4] != event {
                return Err(format!("invalid time/period/event at line {line_number}").into());
            }
            last_time = timestamp;
            current = Some(Sample {
                period,
                frames: Vec::new(),
            });
            continue;
        }
        let full = PATTERNS.frame.captures(line);
        let inline = if full.is_none() {
            PATTERNS.inline.captures(line)
        } else {
            None
        };
        if let Some(c) = full.as_ref().or(inline.as_ref()) {
            let sample = current
                .as_mut()
                .ok_or_else(|| format!("orphan frame at line {line_number}"))?;
            let symbol = if PATTERNS.address.is_match(&c[2]) {
                "[unknown]"
            } else {
                c[2].trim_end()
            };
            let module = if full.is_some() {
                Path::new(&c[3])
                    .file_name()
                    .map(|v| v.to_string_lossy().into_owned())
            } else {
                None
            };
            let module = module.map(|v| {
                if v == "worker" || PATTERNS.worker.is_match(&v) {
                    "codec-worker".into()
                } else {
                    v
                }
            });
            sample.frames.push(Frame {
                symbol: symbol.into(),
                module,
                source: None,
                line: None,
            });
            if sample.frames.len() > MAX_FRAMES {
                return Err("bounded frame inventory overflow".into());
            }
            continue;
        }
        if let Some(frame) = current
            .as_mut()
            .and_then(|sample| sample.frames.last_mut())
            .filter(|_| line.starts_with("  "))
        {
            if let Some((source, number)) = public_source(line) {
                if frame.source.is_some() {
                    return Err(format!("duplicate source location at line {line_number}").into());
                }
                frame.source = Some(source);
                frame.line = Some(number);
            } else if !PATTERNS.unresolved.is_match(line) {
                return Err(format!("unrecognized source location at line {line_number}").into());
            }
            continue;
        }
        return Err(format!("unrecognized perf script input at line {line_number}").into());
    }
    inventory.flush(current, &anchors)?;
    if inventory.totals.samples != expected {
        return Err("decoded/raw sample conservation failed".into());
    }
    let mut stacks: Vec<_> = inventory.stacks.into_values().collect();
    stacks.sort_by(|a, b| {
        b.samples
            .cmp(&a.samples)
            .then_with(|| a.frame_ids.cmp(&b.frame_ids))
    });
    let mut roles: BTreeMap<_, Totals> = BTreeMap::new();
    let mut unresolved = 0;
    let mut empty = 0;
    for stack in &stacks {
        roles
            .entry(stack.role)
            .or_default()
            .add(stack.samples, stack.period_sum)?;
        let leaf = &inventory.frames[stack.frame_ids[0]].symbol;
        if ["[unknown]", "[unresolved-callchain]"].contains(&leaf.as_str()) {
            unresolved += stack.samples;
        }
        if leaf == "[unresolved-callchain]" {
            empty += stack.samples;
        }
    }
    Ok(Report {
        schema: "software-stack-inventory-v1",
        purpose: "descriptive stack/event accounting; not throughput or unique fault causality",
        event,
        record_types: records,
        samples: inventory.totals.samples,
        period_sum: inventory.totals.period_sum,
        actual_period_histogram: inventory
            .periods
            .into_iter()
            .map(|(period, samples)| Period { period, samples })
            .collect(),
        unresolved_leaf_samples: unresolved,
        empty_callchain_samples: empty,
        source_anchors: anchors,
        roles,
        frames: inventory.frames,
        stacks,
        release_approval: false,
        candidate_attribution: None,
        recording_quality: serde_json::Value::Null,
    })
}

fn recording_quality(text: &str, exit_code: i32) -> Result<serde_json::Value> {
    if exit_code != 0 || PATTERNS.bad_log.is_match(text) || !text.contains("Captured and wrote") {
        return Err("unusable perf capture; retain original log and data".into());
    }
    Ok(
        serde_json::json!({"returncode":exit_code,"capture_message_observed":true,"fatal_diagnostics":false}),
    )
}

fn record_stats(text: &str) -> Result<BTreeMap<String, u64>> {
    let mut started = false;
    let mut total = None;
    let mut counts = BTreeMap::new();
    for line in text.lines() {
        if !started {
            if line.trim() == "Aggregated stats:" {
                started = true;
            }
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with([' ', '\t']) && line.ends_with(" stats:") {
            break;
        }
        let capture = PATTERNS
            .stats
            .captures(line)
            .ok_or("unrecognized aggregate record statistic")?;
        let count = capture[2].parse::<u64>()?;
        if &capture[1] == "TOTAL" {
            if total.replace(count).is_some() {
                return Err("duplicate aggregate total".into());
            }
        } else if counts
            .insert(format!("PERF_RECORD_{}", &capture[1]), count)
            .is_some()
        {
            return Err("duplicate aggregate record type".into());
        }
    }
    let sum = counts.values().try_fold(0u64, |a, b| {
        a.checked_add(*b).ok_or("record counter overflow")
    })?;
    if !started || total.is_none_or(|v| v == 0 || v != sum) {
        return Err("incomplete aggregate record statistics".into());
    }
    Ok(counts)
}

pub(crate) fn run(args: &Args) -> Result<()> {
    #[derive(Deserialize)]
    struct Records {
        record_types: BTreeMap<String, u64>,
    }
    let path = Path::new(args.required("--output")?);
    if path.exists() {
        return Err("refuse to overwrite an existing inventory".into());
    }
    let quality = recording_quality(
        &std::fs::read_to_string(args.required("--record-log")?)?,
        args.required("--record-exit-code")?.parse()?,
    )?;
    let records: Records = artifact::read_json(Path::new(args.required("--records")?))?;
    let source = anchors(&std::fs::read_to_string(args.required("--source")?)?)?;
    let mut report = audit(
        std::io::BufReader::new(std::fs::File::open(args.required("--input")?)?),
        records.record_types,
        source,
        args.required("--event")?,
    )?;
    report.recording_quality = quality;
    // create_new preserves the original no-overwrite contract under a race.
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

pub(crate) fn stats(args: &Args) -> Result<()> {
    let counts = record_stats(&std::fs::read_to_string(args.required("--input")?)?)?;
    artifact::write_json(
        Path::new(args.required("--output")?),
        &serde_json::json!({"record_types":counts}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = "fn prepare_read_buffer() {\n    match n {\n        core::cmp::Ordering::Less => buf.resize(max_len, 0),\n    }\n}\npub fn encode_stream() {\n    for buf in parity_bufs.iter_mut() {\n        buf.resize(actual_len, 0);\n    }\n    write_block_with_mode(\n    );\n    }\npub fn verify_stream() {}\n";
    fn sample(
        time: &str,
        period: u64,
        event: &str,
        symbol: &str,
        source: &str,
        caller: Option<&str>,
    ) -> String {
        let mut text = format!(
            "{time}: {period} {event}:\n\t abcdef012345 {symbol} (/private/machine/lib/libc.so.6)\n  {source}\n"
        );
        if let Some(caller) = caller {
            text += &format!("\t 123456789abc {caller} (/private/machine/codec)\n  ??:0\n");
        }
        text + "\n"
    }
    fn run<'a>(text: &str, count: u64, event: &'a str) -> Result<Report<'a>> {
        audit(
            text.as_bytes(),
            BTreeMap::from([("PERF_RECORD_SAMPLE".into(), count)]),
            anchors(SOURCE)?,
            event,
        )
    }
    #[test]
    fn sample_periods_locations_and_unknowns_are_conserved_without_addresses() {
        let a = anchors(SOURCE).unwrap();
        let text = sample(
            "1.000001",
            1,
            "minor-faults:u",
            "[unknown]",
            &format!(
                "/private/project/src/core/stream.rs:{}",
                a.read_initialization
            ),
            None,
        ) + &sample(
            "1.000002",
            1,
            "minor-faults:u",
            "[unknown]",
            &format!(
                "/private/project/src/core/stream.rs:{}",
                a.parity_initialization
            ),
            None,
        ) + &sample(
            "1.000003",
            1009,
            "minor-faults:u",
            "unclassified_function",
            "libc.so.6[abcd]",
            None,
        );
        let r = run(&text, 3, "minor-faults:u").unwrap();
        assert_eq!(
            (r.samples, r.period_sum, r.unresolved_leaf_samples),
            (3, 1011, 2)
        );
        assert_eq!(r.roles.values().map(|r| r.samples).sum::<u64>(), 3);
        assert_eq!(r.roles.values().map(|r| r.period_sum).sum::<u64>(), 1011);
        let exported = serde_json::to_string(&r).unwrap();
        for secret in [
            "/private/",
            "abcdef012345",
            "123456789abc",
            "libc.so.6[abcd]",
        ] {
            assert!(!exported.contains(secret));
        }
        assert!(!r.release_approval);
        assert!(r.candidate_attribution.is_none());
    }
    #[test]
    fn classification_requires_real_frames_and_explicit_cleanup() {
        for (caller, leaf, role) in [
            (
                "rustfs_erasure_codec::core::stream::read_one_stream",
                "[unknown]",
                "stream_read_path",
            ),
            (
                "rustfs_erasure_codec::core::stream::write_block",
                "[unknown]",
                "stream_write_path",
            ),
            (
                "different_crate::read_one_stream",
                "[unknown]",
                "other_or_unattributed",
            ),
            (
                "rustfs_erasure_codec::core::stream::ReedSolomon::encode_stream",
                "__libc_free",
                "stream_encode_cleanup",
            ),
            (
                "rustfs_erasure_codec::core::stream::ReedSolomon::encode_stream",
                "core::ptr::drop_in_place<Guard>",
                "other_stream_encode",
            ),
            (
                "wrapper<rustfs_erasure_codec::core::stream::read_one_stream>",
                "[unknown]",
                "other_or_unattributed",
            ),
        ] {
            let text = sample(
                "1.000001",
                1,
                "minor-faults:u",
                leaf,
                "libc.so.6[abcd]",
                Some(caller),
            );
            assert!(
                run(&text, 1, "minor-faults:u")
                    .unwrap()
                    .roles
                    .contains_key(role),
                "{caller}"
            );
        }
    }
    #[test]
    fn malformed_missing_or_nonmonotonic_samples_fail() {
        let valid = sample(
            "1.000002",
            1,
            "minor-faults:u",
            "[unknown]",
            "libc.so.6[abcd]",
            None,
        );
        for (text, count) in [
            (String::new(), 1),
            (valid.clone(), 2),
            ("\t abcdef orphan (libc.so.6)\n".into(), 1),
            (valid.replace(" 1 minor-faults", " 0 minor-faults"), 1),
            (valid.replace("minor-faults:u", "cpu-clock"), 1),
            (valid.clone() + &valid.replace("1.000002", "1.000001"), 2),
            (valid.replace("libc.so.6[abcd]", "unrecognized source"), 1),
            (
                valid.replace(
                    "libc.so.6[abcd]",
                    "/private/file.rs:10\n  /private/file.rs:11",
                ),
                1,
            ),
        ] {
            assert!(run(&text, count, "minor-faults:u").is_err(), "{text}");
        }
    }
    #[test]
    fn lost_throttled_and_empty_callchains_are_handled_explicitly() {
        for bad in BAD_RECORDS {
            assert!(
                audit(
                    "".as_bytes(),
                    BTreeMap::from([("PERF_RECORD_SAMPLE".into(), 1), (bad.into(), 1)]),
                    anchors(SOURCE).unwrap(),
                    "cpu-clock"
                )
                .is_err()
            );
        }
        let r = run("1.000001: 2000011 cpu-clock:\n\n", 1, "cpu-clock").unwrap();
        assert_eq!(
            (
                r.empty_callchain_samples,
                r.unresolved_leaf_samples,
                r.period_sum
            ),
            (1, 1, 2000011)
        );
        assert_eq!(r.frames[0].symbol, "[unresolved-callchain]");
        assert_eq!(
            public_source("/private/.cargo/registry/src/index/rayon-1.12.0/src/iter.rs:9"),
            Some(("registry/rayon-1.12.0/src/iter.rs".into(), 9))
        );
    }
    #[test]
    fn zero_exit_does_not_override_corruption_or_lost_chunks() {
        let good = "[ perf record: Captured and wrote 1 MB (10 samples) ]";
        assert!(recording_quality(good, 0).is_ok());
        for (log, code) in [
            (good.to_owned(), 1),
            (String::new(), 0),
            (
                format!("{good}\nfailed to write perf data, error: Bad address"),
                0,
            ),
            (
                format!("{good}\nData corruption detected\nCouldn't decompress data"),
                0,
            ),
            (format!("{good}\nlost 2 chunks!"), 0),
        ] {
            assert!(recording_quality(&log, code).is_err());
        }
    }
    #[test]
    fn source_context_and_nested_type_names_are_not_runtime_callers() {
        assert_eq!(
            function_name(
                "rustfs_erasure_codec::core::stream::<impl ReedSolomon<Field>>::encode_stream<Vec<u8>>"
            ),
            "rustfs_erasure_codec::core::stream::::encode_stream"
        );
        assert_eq!(
            public_source("src/core/stream.rs:33 (inlined)"),
            Some(("src/core/stream.rs".into(), 33))
        );
        assert_eq!(
            public_source("src/backend/gfni.rs:40"),
            Some(("src/backend/gfni.rs".into(), 40))
        );
        let a = anchors(SOURCE).unwrap();
        let text = sample(
            "1.000001",
            1,
            "minor-faults:u",
            "zap_pte_range",
            "libc.so.6[abcd]",
            Some("rustfs_erasure_codec::core::stream::ReedSolomon::encode_stream"),
        )
        .replace("/private/machine/lib/libc.so.6", "[kernel.kallsyms]")
        .replace(
            "  ??:0",
            &format!("  src/core/stream.rs:{}", a.encode_return),
        );
        assert!(
            run(&text, 1, "minor-faults:u")
                .unwrap()
                .roles
                .contains_key("stream_encode_cleanup")
        );
        let text = sample(
            "1.000001",
            1,
            "minor-faults:u",
            "[unknown]",
            "libc.so.6[abcd]",
            None,
        )
        .replace("libc.so.6)", "rse_release_sampling-12345678)");
        assert_eq!(
            run(&text, 1, "minor-faults:u").unwrap().frames[0]
                .module
                .as_deref(),
            Some("codec-worker")
        );
    }
    #[test]
    fn source_anchors_and_aggregate_records_require_unique_complete_evidence() {
        for source in [
            SOURCE.replace("buf.resize(actual_len, 0);", ""),
            SOURCE.repeat(2),
        ] {
            assert!(anchors(&source).is_err());
        }
        let current = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/core/stream.rs"),
        )
        .unwrap();
        let a = anchors(&current).unwrap();
        assert!(
            current
                .lines()
                .nth(a.read_initialization as usize - 1)
                .unwrap()
                .contains("buf.resize(max_len, 0)")
        );
        let text =
            "Aggregated stats:\n TOTAL events: 4\n SAMPLE events: 3 (75.0%)\n COMM events: 1\n";
        assert_eq!(record_stats(text).unwrap()["PERF_RECORD_SAMPLE"], 3);
        for bad in [
            text.replace("TOTAL events: 4", "TOTAL events: 5"),
            text.to_owned() + " SAMPLE events: 3\n",
            String::new(),
        ] {
            assert!(record_stats(&bad).is_err());
        }
    }
}
