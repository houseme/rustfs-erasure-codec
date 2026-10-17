//! Standalone DecodeIdx planning experiment; this is not a public workspace API.
//!
//! Link explicitly against one already-built library artifact and its matching
//! dependency directory. No Cargo build or additional crate dependency is needed:
//!
//! ```text
//! rustc --edition=2024 -O scripts/decode_idx_workspace_probe.rs \
//!   -L dependency=LIBRARY_DEPS \
//!   --extern rustfs_erasure_codec=LIBRARY_RLIB -o decode-idx-probe
//! ./decode-idx-probe --allocations
//! ./decode-idx-probe --timing
//! ```
//!
//! To compare library revisions, compile this identical source twice, changing
//! only LIBRARY_RLIB and its matching LIBRARY_DEPS, and save both JSONL outputs.
//! Keep the original artifacts separate; a shared Cargo target can overwrite an
//! rlib at the same filename after a source change. Pass both flags to run both
//! phases; with no flags only allocation checks run.
//!
//! A is the linked library's decode_idx; B is a temporary prepared-row prototype.
//! All input/output buffers are preallocated and reused. B's preparation is NOT
//! timed. Each ABBA window starts with zeroed destinations outside the timer;
//! odd iteration counts preserve repeated-XOR semantics and permit exact final
//! byte checks. Retain every result; neither-side drift may exceed 5% for a row
//! to be accepted. This probe neither reserves the host nor proves cold/first-
//! object benefits, remaining gains after a different runtime fix, or external
//! isolation acceptance. The source preserves all 24 original timing cases.
//!
//! The prototype has a fixed source basis and target mask, owns coefficient rows
//! and index scratch, and retains no caller data references. It does not add
//! duplicate-input tracking or replace merge mode. Actual production workspace
//! behavior and API design remain unevaluated here.

use rustfs_erasure_codec::{
    CodecFamily, EncodingFormat, Error,
    galois_8::{self, ReedSolomon},
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
    time::Instant,
};
thread_local! {
    static ALLOC: Cell<Option<(usize, usize)>> = const { Cell::new(None) };
}
struct Counter;
fn record(bytes: usize) {
    let _ = ALLOC.try_with(|count| {
        if let Some((calls, total)) = count.get() {
            count.set(Some((calls + 1, total + bytes)));
        }
    });
}
// SAFETY: requests are delegated unchanged to System. The thread-local
// counter records only integers and does not allocate or retain pointers.
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        // SAFETY: System receives the caller's original allocation layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        // SAFETY: System receives the caller's original allocation layout.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: pointer and layout identify the original System allocation.
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record(new_size);
        // SAFETY: System receives its own allocation and the requested size.
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}
#[global_allocator]
static COUNTER: Counter = Counter;
fn alloc_measure<T>(measure: impl FnOnce() -> T) -> (T, (usize, usize)) {
    ALLOC.with(|count| count.set(Some((0, 0))));
    let result = measure();
    let allocations = ALLOC.with(|count| count.replace(None).unwrap());
    (result, allocations)
}
type Shards = Vec<Option<Vec<u8>>>;
struct Plan {
    format: EncodingFormat<galois_8::Field>,
    expected: Vec<bool>,
    targets: Vec<bool>,
    sources: Vec<usize>,
    rows: Vec<Vec<u8>>,
    positions: Vec<usize>,
}
impl Plan {
    fn new(rs: &ReedSolomon, expect: &[bool], targets: &[bool]) -> Self {
        let k = rs.data_shard_count();
        let n = rs.total_shard_count();
        let sources: Vec<_> = expect
            .iter()
            .enumerate()
            .filter_map(|(i, b)| b.then_some(i))
            .collect();
        assert_eq!(sources.len(), k);
        let mut unit = vec![None; n];
        for (col, &idx) in sources.iter().enumerate() {
            let mut v = vec![0; k];
            v[col] = 1;
            unit[idx] = Some(v);
        }
        let mut rows: Shards = targets.iter().map(|b| b.then(|| vec![0; k])).collect();
        rs.decode_idx(&mut rows, Some(expect), &unit).unwrap();
        Self {
            format: rs.encoding_format().clone(),
            expected: expect.to_vec(),
            targets: targets.to_vec(),
            sources,
            rows: rows.into_iter().flatten().collect(),
            positions: Vec::with_capacity(k),
        }
    }
    fn run(
        &mut self,
        rs: &ReedSolomon,
        dst: &mut [Option<Vec<u8>>],
        expect: &[bool],
        input: &[Option<Vec<u8>>],
    ) -> Result<(), Error> {
        if rs.codec_family() != CodecFamily::Classic {
            return Err(Error::UnsupportedCodecFamily);
        }
        if dst.len() != rs.total_shard_count() || input.len() != rs.total_shard_count() {
            return Err(Error::TooFewShards);
        }
        if rs.encoding_format() != &self.format {
            return Err(Error::InvalidCustomMatrix);
        }
        if expect != self.expected {
            return Err(Error::InvalidShardFlags);
        }
        let len = input
            .iter()
            .chain(dst.iter())
            .flatten()
            .next()
            .map(Vec::len)
            .ok_or(Error::TooFewShardsPresent)?;
        if input
            .iter()
            .chain(dst.iter())
            .flatten()
            .any(|s| s.len() != len)
        {
            return Err(Error::IncorrectShardSize);
        }
        if dst
            .iter()
            .zip(&self.targets)
            .any(|(d, b)| d.is_some() != *b)
        {
            return Err(Error::InvalidShardFlags);
        }
        self.positions.clear();
        for (column, &idx) in self.sources.iter().enumerate() {
            if input[idx].is_some() {
                self.positions.push(column)
            }
        }
        if self.positions.is_empty() || self.rows.is_empty() || len == 0 {
            return Ok(());
        }
        let chunk = if len <= 16384 {
            len
        } else if len <= 65536 {
            16384
        } else if len <= 4 * 1024 * 1024 {
            65536
        } else {
            262144
        };
        let mut output = dst.iter_mut().flatten();
        if self.rows.len() == 2 {
            let a = output.next().unwrap();
            let b = output.next().unwrap();
            for (part, (ao, bo)) in a.chunks_mut(chunk).zip(b.chunks_mut(chunk)).enumerate() {
                let start = part * chunk;
                let end = start + ao.len();
                for &col in &self.positions {
                    let src = &input[self.sources[col]].as_ref().unwrap()[start..end];
                    galois_8::mul_slice_xor(self.rows[0][col], src, ao);
                    galois_8::mul_slice_xor(self.rows[1][col], src, bo);
                }
            }
        } else {
            for (row, out) in self.rows.iter().zip(output) {
                for (part, chunkout) in out.chunks_mut(chunk).enumerate() {
                    let start = part * chunk;
                    let end = start + chunkout.len();
                    for &col in &self.positions {
                        galois_8::mul_slice_xor(
                            row[col],
                            &input[self.sources[col]].as_ref().unwrap()[start..end],
                            chunkout,
                        );
                    }
                }
            }
        }
        Ok(())
    }
}
#[allow(clippy::type_complexity)]
fn fixture(
    k: usize,
    p: usize,
    len: usize,
    nb: usize,
    parity: bool,
) -> (
    ReedSolomon,
    Vec<Vec<u8>>,
    Vec<bool>,
    Vec<bool>,
    Vec<Shards>,
    Shards,
) {
    let rs = ReedSolomon::new(k, p).unwrap();
    let mut original: Vec<Vec<u8>> = (0..k + p)
        .map(|i| (0..len).map(|j| (i * 53 + j * 17 + j / 7) as u8).collect())
        .collect();
    rs.encode(&mut original).unwrap();
    let mut expect = vec![false; k + p];
    for (i, expected) in expect.iter_mut().enumerate().take(k) {
        if i != 0 && i != 2 {
            *expected = true
        }
    }
    expect[k] = true;
    expect[k + 1] = true;
    let mut targets = vec![false; k + p];
    if parity {
        targets[k + p - 2] = true;
        targets[k + p - 1] = true;
    } else {
        targets[0] = true;
        targets[2] = true;
    }
    let mut batches = vec![vec![None; k + p]; nb];
    for (col, idx) in expect
        .iter()
        .enumerate()
        .filter_map(|(i, b)| b.then_some(i))
        .enumerate()
    {
        batches[col % nb][idx] = Some(original[idx].clone());
    }
    let dst = targets.iter().map(|b| b.then(|| vec![0; len])).collect();
    (rs, original, expect, targets, batches, dst)
}
fn clear(dst: &mut Shards) {
    for d in dst.iter_mut().flatten() {
        d.fill(0)
    }
}
fn check(dst: &Shards, original: &[Vec<u8>]) {
    for (i, d) in dst.iter().enumerate() {
        if let Some(d) = d {
            assert_eq!(d, &original[i]);
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn time_run(
    rs: &ReedSolomon,
    plan: &mut Plan,
    dst: &mut Shards,
    expect: &[bool],
    batches: &[Shards],
    iters: usize,
    prepared: bool,
) -> f64 {
    clear(dst);
    let start = Instant::now();
    for _ in 0..iters {
        for batch in batches {
            if prepared {
                plan.run(
                    black_box(rs),
                    black_box(dst),
                    black_box(expect),
                    black_box(batch),
                )
                .unwrap()
            } else {
                rs.decode_idx(black_box(dst), Some(black_box(expect)), black_box(batch))
                    .unwrap()
            }
        }
    }
    black_box(dst);
    start.elapsed().as_nanos() as f64 / iters as f64
}
fn main() -> Result<(), String> {
    let Some((allocations, timing)) = parse_modes()? else {
        return Ok(());
    };
    println!(
        r#"{{"kind":"metadata","schema_version":1,"prototype":"temporary prepared rows; not a library workspace API","preparation_timed":false,"host_reservation":"not checked by this probe","timing_order":"A1,B1,B2,A2","relative_drift_limit":0.05,"allocation_metric":"allocation calls and cumulative requested bytes, not peak memory"}}"#
    );
    if allocations {
        for (k, p) in [(4, 2), (10, 4), (32, 16), (64, 16)] {
            for nb in [2, 4, k] {
                for parity in [false, true] {
                    let (rs, original, expect, targets, batches, mut dst) =
                        fixture(k, p, 32768, nb, parity);
                    for batch in &batches {
                        rs.decode_idx(&mut dst, Some(&expect), batch).unwrap()
                    }
                    check(&dst, &original);
                    clear(&mut dst);
                    let (_, base) = alloc_measure(|| {
                        for batch in &batches {
                            rs.decode_idx(&mut dst, Some(&expect), batch).unwrap()
                        }
                    });
                    check(&dst, &original);
                    clear(&mut dst);
                    let mut plan = Plan::new(&rs, &expect, &targets);
                    let (_, candidate) = alloc_measure(|| {
                        for batch in &batches {
                            plan.run(&rs, &mut dst, &expect, batch).unwrap()
                        }
                    });
                    check(&dst, &original);
                    emit_allocations(k, p, 32768, nb, parity, base, candidate);
                }
            }
        }
    }
    if timing {
        for (k, p, len, nb) in [
            (4, 2, 64, 2),
            (10, 4, 64, 2),
            (10, 4, 1024, 10),
            (10, 4, 32768, 2),
            (10, 4, 32768, 10),
            (10, 4, 1048576, 2),
            (32, 16, 64, 8),
            (32, 16, 32768, 8),
            (64, 16, 64, 16),
            (64, 16, 32768, 16),
            (32, 16, 32768, 32),
            (64, 16, 32768, 64),
        ] {
            for parity in [false, true] {
                let (rs, original, expect, targets, batches, mut dst) =
                    fixture(k, p, len, nb, parity);
                let mut plan = Plan::new(&rs, &expect, &targets);
                let iters = (128 * 1024 * 1024 / (k * len)).clamp(65, 65535) | 1;
                let mut values = [0.; 4];
                for (pass, prepared) in [false, true, true, false].into_iter().enumerate() {
                    values[pass] =
                        time_run(&rs, &mut plan, &mut dst, &expect, &batches, iters, prepared);
                    check(&dst, &original);
                }
                emit_timing(k, p, len, nb, parity, iters, values);
            }
        }
    }
    let rs = ReedSolomon::new(4, 2).unwrap();
    let mut dst = vec![None; 6];
    dst[0] = Some(vec![0; 64]);
    let mut input = vec![None; 6];
    input[1] = Some(vec![1; 64]);
    let expect = [true; 6];
    assert_eq!(
        rs.decode_idx(&mut dst, Some(&expect), &input),
        Err(Error::TooFewShardsPresent)
    );
    println!(
        r#"{{"kind":"contract","case":"overcomplete expected inputs with active input/output","error":"TooFewShardsPresent"}}"#
    );
    Ok(())
}

fn parse_modes() -> Result<Option<(bool, bool)>, String> {
    let mut allocations = false;
    let mut timing = false;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--allocations" => allocations = true,
            "--timing" => timing = true,
            "--help" | "-h" => {
                println!("Usage: decode-idx-probe [--allocations] [--timing]");
                println!(
                    "Default: allocations only. Results are JSONL; timing excludes prototype preparation."
                );
                return Ok(None);
            }
            _ => return Err(format!("unknown argument {argument:?}; use --help")),
        }
    }
    Ok(Some((allocations || !timing, timing)))
}

fn emit_allocations(
    k: usize,
    p: usize,
    len: usize,
    batches: usize,
    parity: bool,
    library: (usize, usize),
    prototype: (usize, usize),
) {
    println!(
        "{{\"kind\":\"allocations\",\"data_shards\":{k},\"parity_shards\":{p},\"shard_bytes\":{len},\"batches\":{batches},\"parity_targets\":{parity},\"library_calls\":{},\"library_requested_bytes\":{},\"prototype_calls\":{},\"prototype_requested_bytes\":{}}}",
        library.0, library.1, prototype.0, prototype.1
    );
}

fn emit_timing(
    k: usize,
    p: usize,
    len: usize,
    batches: usize,
    parity: bool,
    iterations: usize,
    values: [f64; 4],
) {
    let a_drift = (values[3] / values[0] - 1.0).abs();
    let b_drift = (values[2] / values[1] - 1.0).abs();
    let accepted = a_drift <= 0.05 && b_drift <= 0.05;
    let raw_speedup = (values[0] + values[3]) / (values[1] + values[2]);
    println!(
        "{{\"kind\":\"timing\",\"data_shards\":{k},\"parity_shards\":{p},\"shard_bytes\":{len},\"batches\":{batches},\"parity_targets\":{parity},\"iterations\":{iterations},\"a1_ns_per_restore\":{:.1},\"b1_ns_per_restore\":{:.1},\"b2_ns_per_restore\":{:.1},\"a2_ns_per_restore\":{:.1},\"raw_speedup\":{raw_speedup:.4},\"a_relative_drift\":{a_drift:.4},\"b_relative_drift\":{b_drift:.4},\"accepted\":{accepted}}}",
        values[0], values[1], values[2], values[3]
    );
}
