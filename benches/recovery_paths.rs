//! Small, reproducible warm-path probe; see docs/benchmark-methodology.md.

use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write as _;
use std::hint::black_box;
use std::io::Cursor;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use rustfs_erasure_codec::galois_8::{
    ReedSolomon, active_backend_id, active_backend_kind, active_backend_name, mark_missing_slots,
    shards_to_slots,
};
use rustfs_erasure_codec::stream::{StreamIoMode, StreamOptions};
use rustfs_erasure_codec::{CodecFamily, CodecOptions, Error, ShardSlot, VerifyWorkspace};

const FIXTURE_SEED: u64 = 0x817a_bdef_7649_2104;
const WARMUP_ITERATIONS: usize = 20;
const SAMPLE_COUNT: usize = 3;
const ITERATIONS_PER_BATCH: usize = 8;

struct CountingAllocator;

static COUNT_ALLOCATIONS: AtomicBool = AtomicBool::new(false);
static ALLOCATION_CALLS: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);

fn record_allocation(bytes: usize) {
    if COUNT_ALLOCATIONS.load(Ordering::Relaxed) {
        ALLOCATION_CALLS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(bytes, Ordering::Relaxed);
    }
}

// SAFETY: Every allocation operation delegates unchanged pointer/layout arguments
// to System. Tracking uses only atomics and never allocates or changes ownership.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation(layout.size());
        // SAFETY: GlobalAlloc callers supply a valid allocation layout.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation(layout.size());
        // SAFETY: Preserve the caller's valid layout and zero-initialization request.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_allocation(new_size);
        // SAFETY: The caller guarantees pointer/layout describe a live allocation
        // from this allocator, which delegates every allocation to System.
        unsafe { System.realloc(pointer, layout, new_size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: The caller supplies a live pointer and its original layout;
        // all allocations made by this allocator originate from System.
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

struct Config {
    sample_ms: u64,
    filter: Option<String>,
    features: String,
    git_revision: String,
    git_dirty: Option<bool>,
    target_triple: String,
    backend_override: String,
    available_parallelism: usize,
    rayon_threads: usize,
}

impl Config {
    fn read() -> Result<Self, String> {
        let sample_ms = match std::env::var("RSE_RECOVERY_SAMPLE_MS") {
            Ok(value) => value
                .parse::<u64>()
                .ok()
                .filter(|&value| value > 0)
                .ok_or("RSE_RECOVERY_SAMPLE_MS must be a positive integer")?,
            Err(std::env::VarError::NotPresent) => 100,
            Err(error) => return Err(error.to_string()),
        };
        let filter = match std::env::var("RSE_RECOVERY_CASE_FILTER") {
            Ok(value) => Some(value),
            Err(std::env::VarError::NotPresent) => None,
            Err(error) => return Err(error.to_string()),
        };
        let features = [
            ("std", cfg!(feature = "std")),
            ("simd-accel", cfg!(feature = "simd-accel")),
            ("simd-neon", cfg!(feature = "simd-neon")),
            ("simd-ssse3", cfg!(feature = "simd-ssse3")),
            ("simd-avx2", cfg!(feature = "simd-avx2")),
            ("simd-avx512", cfg!(feature = "simd-avx512")),
            ("simd-gfni", cfg!(feature = "simd-gfni")),
            ("simd-vsx", cfg!(feature = "simd-vsx")),
            ("benchmark-metrics", cfg!(feature = "benchmark-metrics")),
        ]
        .into_iter()
        .filter_map(|(name, enabled)| enabled.then_some(name))
        .collect::<Vec<_>>()
        .join("|");
        Ok(Self {
            sample_ms,
            filter,
            features,
            git_revision: git_output(&["rev-parse", "--short", "HEAD"])
                .map(|revision| revision.trim().to_owned())
                .filter(|revision| !revision.is_empty())
                .unwrap_or_else(|| "unknown".to_owned()),
            git_dirty: git_output(&["status", "--porcelain", "--untracked-files=normal"])
                .map(|status| !status.is_empty()),
            // Match benches/common's historical ARCH-OS-ENV convention.
            target_triple: format!(
                "{}-{}-{}",
                std::env::consts::ARCH,
                std::env::consts::OS,
                option_env!("CARGO_CFG_TARGET_ENV").unwrap_or("unknown"),
            ),
            backend_override: std::env::var("RSE_BACKEND_OVERRIDE")
                .unwrap_or_else(|_| "auto".to_owned()),
            available_parallelism: std::thread::available_parallelism().map_or(1, usize::from),
            rayon_threads: rayon::current_num_threads(),
        })
    }

    fn includes(&self, case: &Case) -> bool {
        self.filter
            .as_ref()
            .is_none_or(|filter| case.id().contains(filter))
    }
}

fn git_output(arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(arguments)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn json_quoted(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for character in value.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            '\u{08}' => quoted.push_str("\\b"),
            '\u{0c}' => quoted.push_str("\\f"),
            character if character <= '\u{1f}' => {
                write!(quoted, "\\u{:04x}", u32::from(character))
                    .expect("format JSON control character");
            }
            character => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}

struct Case {
    operation: &'static str,
    data_shards: usize,
    parity_shards: usize,
    shard_size: usize,
    missing_data_shards: usize,
    missing_parity_shards: usize,
    stream_block_size: Option<usize>,
}

impl Case {
    fn id(&self) -> String {
        format!(
            "{}/{}x{}/{}/missing{}+{}/block{}",
            self.operation,
            self.data_shards,
            self.parity_shards,
            self.shard_size,
            self.missing_data_shards,
            self.missing_parity_shards,
            self.stream_block_size.unwrap_or(0),
        )
    }
}

struct Measurement {
    allocations: usize,
    allocated_bytes: usize,
    samples_ns: [f64; SAMPLE_COUNT],
    sample_iterations: [usize; SAMPLE_COUNT],
}

fn measure(config: &Config, mut operation: impl FnMut()) -> Measurement {
    for _ in 0..WARMUP_ITERATIONS {
        operation();
    }
    ALLOCATION_CALLS.store(0, Ordering::Relaxed);
    ALLOCATED_BYTES.store(0, Ordering::Relaxed);
    COUNT_ALLOCATIONS.store(true, Ordering::Relaxed);
    operation();
    COUNT_ALLOCATIONS.store(false, Ordering::Relaxed);
    let allocations = ALLOCATION_CALLS.load(Ordering::Relaxed);
    let allocated_bytes = ALLOCATED_BYTES.load(Ordering::Relaxed);

    let duration = Duration::from_millis(config.sample_ms);
    let mut samples_ns = [0.0; SAMPLE_COUNT];
    let mut sample_iterations = [0; SAMPLE_COUNT];
    for (sample, iterations) in samples_ns.iter_mut().zip(&mut sample_iterations) {
        let start = Instant::now();
        // Always execute one batch, even if this process is descheduled before
        // its first timer check. That keeps every reported sample finite.
        loop {
            for _ in 0..ITERATIONS_PER_BATCH {
                operation();
            }
            *iterations += ITERATIONS_PER_BATCH;
            if start.elapsed() >= duration {
                break;
            }
        }
        *sample = start.elapsed().as_secs_f64() * 1e9 / *iterations as f64;
    }
    Measurement {
        allocations,
        allocated_bytes,
        samples_ns,
        sample_iterations,
    }
}

fn report(config: &Config, case: &Case, measurement: Measurement) {
    let mut sorted = measurement.samples_ns;
    sorted.sort_by(f64::total_cmp);
    let median_ns = sorted[SAMPLE_COUNT / 2];
    let block = case
        .stream_block_size
        .map_or_else(|| "null".to_owned(), |size| size.to_string());
    let io_mode = if case.stream_block_size.is_some() {
        "serial"
    } else {
        "not-applicable"
    };
    let git_dirty = config
        .git_dirty
        .map_or("null", |dirty| if dirty { "true" } else { "false" });
    println!(
        concat!(
            "{{\"schema_version\":1,\"artifact_kind\":\"recovery-path-results\",",
            "\"case_id\":\"{}\",\"operation\":\"{}\",\"data_shards\":{},\"parity_shards\":{},",
            "\"shard_size\":{},\"missing_data_shards\":{},\"missing_parity_shards\":{},",
            "\"stream_block_size\":{},\"stream_io_mode\":\"{}\",",
            "\"allocations\":{},\"allocated_bytes\":{},\"samples_ns\":{:?},\"sample_iterations\":{:?},",
            "\"median_ns_per_iter\":{:.2},\"sample_ms\":{},\"warmup_iterations\":{},",
            "\"fixture_seed\":\"817abdef76492104\",\"allocation_iterations\":1,",
            "\"git_revision\":{},\"git_dirty\":{},\"target_triple\":{},\"backend_override\":{},",
            "\"package_version\":\"{}\",\"target_arch\":\"{}\",\"target_os\":\"{}\",",
            "\"features\":\"{}\",\"benchmark_metrics_enabled\":{},\"backend\":\"{}\",",
            "\"backend_id\":\"{:?}\",\"backend_kind\":\"{:?}\",",
            "\"available_parallelism\":{},\"rayon_threads\":{}}}"
        ),
        case.id(),
        case.operation,
        case.data_shards,
        case.parity_shards,
        case.shard_size,
        case.missing_data_shards,
        case.missing_parity_shards,
        block,
        io_mode,
        measurement.allocations,
        measurement.allocated_bytes,
        measurement.samples_ns,
        measurement.sample_iterations,
        median_ns,
        config.sample_ms,
        WARMUP_ITERATIONS,
        json_quoted(&config.git_revision),
        git_dirty,
        json_quoted(&config.target_triple),
        json_quoted(&config.backend_override),
        env!("CARGO_PKG_VERSION"),
        std::env::consts::ARCH,
        std::env::consts::OS,
        config.features,
        cfg!(feature = "benchmark-metrics"),
        active_backend_name(),
        active_backend_id(),
        active_backend_kind(),
        config.available_parallelism,
        config.rayon_threads,
    );
}

fn fixture(codec: &ReedSolomon, shard_size: usize) -> Vec<Vec<u8>> {
    let mut state = FIXTURE_SEED;
    let mut shards: Vec<Vec<u8>> = (0..codec.total_shard_count())
        .map(|_| {
            (0..shard_size)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    state as u8
                })
                .collect()
        })
        .collect();
    codec
        .encode(&mut shards)
        .expect("encode deterministic fixture");
    shards
}

fn poison_missing(slots: &mut [ShardSlot<Vec<u8>>], missing_indices: &[usize]) {
    for &index in missing_indices {
        slots[index].mark_missing();
        for byte in slots[index].as_inner_mut() {
            *byte ^= 0xff;
        }
    }
}

fn assert_complete_slots(slots: &[ShardSlot<Vec<u8>>], original: &[Vec<u8>]) {
    for (slot, expected) in slots.iter().zip(original) {
        assert!(slot.is_present(), "recovered shard must be marked present");
        assert_eq!(slot.as_inner(), expected);
    }
}

fn assert_selective_slots(
    slots: &[ShardSlot<Vec<u8>>],
    original: &[Vec<u8>],
    unrequested_data: &[u8],
) {
    assert!(
        !slots[0].is_present(),
        "unrequested data must remain missing"
    );
    assert_eq!(slots[0].as_inner().as_slice(), unrequested_data);
    assert_complete_slots(&slots[1..], &original[1..]);
}

fn assert_complete_options(shards: &[Option<Vec<u8>>], original: &[Vec<u8>]) {
    for (shard, expected) in shards.iter().zip(original) {
        assert_eq!(shard.as_deref(), Some(expected.as_slice()));
    }
}

// The selected downstream decode engine calls ordinary verify after recovery.
// Keep each existing API as a distinct case: reusable scratch is a caller
// lifecycle choice, not an implicit replacement for the ordinary release case.
fn verification_cases(config: &Config) -> usize {
    let mut completed = 0;
    for (data, parity, size) in [(4, 2, 16384), (10, 4, 32768)] {
        if ![
            "verify.ordinary",
            "verify.caller_buffer",
            "verify.workspace",
        ]
        .iter()
        .any(|&operation| {
            config.includes(&Case {
                operation,
                data_shards: data,
                parity_shards: parity,
                shard_size: size,
                missing_data_shards: 0,
                missing_parity_shards: 0,
                stream_block_size: None,
            })
        }) {
            continue;
        }
        let codec = ReedSolomon::new(data, parity).expect("construct verification codec");
        let source = fixture(&codec, size);
        let mut corrupt = source.clone();
        corrupt[data][size - 1] ^= 0x5a;
        // One retained source and reference adapter are shared by every API.
        let inputs: Vec<_> = source.iter().map(Vec::as_slice).collect();
        let invalid: Vec<_> = corrupt.iter().map(Vec::as_slice).collect();
        let mut scratch: Vec<_> = (0..parity).map(|_| vec![0u8; size]).collect();
        let mut workspace = VerifyWorkspace::new(&codec, size);
        for operation in [
            "verify.ordinary",
            "verify.caller_buffer",
            "verify.workspace",
        ] {
            let case = Case {
                operation,
                data_shards: data,
                parity_shards: parity,
                shard_size: size,
                missing_data_shards: 0,
                missing_parity_shards: 0,
                stream_block_size: None,
            };
            if !config.includes(&case) {
                continue;
            }
            let mut verify = |shards: &[&[u8]]| {
                match operation {
                    "verify.ordinary" => codec.verify(shards),
                    "verify.caller_buffer" => codec.verify_with_buffer(shards, &mut scratch),
                    "verify.workspace" => codec.verify_with_workspace(shards, &mut workspace),
                    _ => unreachable!(),
                }
                .expect("verification API")
            };
            // Validate a damaged parity tail too; successful no-op verification
            // cannot satisfy these oracles. Preparation is outside every timer.
            assert!(verify(&inputs));
            assert!(!verify(&invalid));
            let measurement = measure(config, || {
                assert!(black_box(verify(black_box(&inputs))));
            });
            assert!(verify(&inputs));
            assert!(!verify(&invalid));
            report(config, &case, measurement);
            completed += 1;
        }
        assert_eq!(source[data][size - 1], corrupt[data][size - 1] ^ 0x5a);
    }
    completed
}

fn main() -> Result<(), String> {
    let config = Config::read()?;
    let mut completed = verification_cases(&config);
    for (family, operation) in [
        (CodecFamily::LeopardGF8, "gf8.reconstruct"),
        (CodecFamily::LeopardGF16, "gf16.reconstruct"),
    ] {
        for (data, parity, size, missing) in [
            (10, 4, 1024, 1),
            (10, 4, 32768, 1),
            (10, 4, 32768, 2),
            (10, 4, 131136, 1),
            (32, 16, 32768, 2),
        ] {
            let case = Case {
                operation,
                data_shards: data,
                parity_shards: parity,
                shard_size: size,
                missing_data_shards: missing,
                missing_parity_shards: 0,
                stream_block_size: None,
            };
            if !config.includes(&case) {
                continue;
            }
            let codec = ReedSolomon::with_options(
                data,
                parity,
                CodecOptions::builder().codec_family(family).build(),
            )
            .expect("construct recovery codec");
            let original = fixture(&codec, size);
            let mut slots = shards_to_slots(&original);
            let indices: Vec<_> = (0..missing).collect();
            // Validate real recovery outside the measured loop. Merely marking
            // correct bytes missing would let a successful no-op pass the oracle.
            poison_missing(&mut slots, &indices);
            codec.reconstruct(&mut slots).expect("recovery preflight");
            assert_complete_slots(&slots, &original);
            let measurement = measure(&config, || {
                mark_missing_slots(&mut slots, &indices);
                codec
                    .reconstruct(black_box(&mut slots))
                    .expect("recover missing data");
            });
            assert_complete_slots(&slots, &original);
            report(&config, &case, measurement);
            completed += 1;
        }
    }
    for (data, parity, size, block) in [
        (10, 4, 1048576, 4096),
        (10, 4, 1048576, 65536),
        (32, 16, 1048576, 4096),
    ] {
        let case = Case {
            operation: "stream.reconstruct",
            data_shards: data,
            parity_shards: parity,
            shard_size: size,
            missing_data_shards: 1,
            missing_parity_shards: 1,
            stream_block_size: Some(block),
        };
        if !config.includes(&case) {
            continue;
        }
        let codec = ReedSolomon::new(data, parity).expect("construct stream codec");
        let original = fixture(&codec, size);
        let mut streams: Vec<_> = original.iter().cloned().map(Cursor::new).collect();
        let options = StreamOptions::default()
            .with_block_size(block)
            .with_io_mode(StreamIoMode::Serial);
        for index in [0, data] {
            streams[index].get_mut().clear();
        }
        codec
            .reconstruct_stream(&mut streams, &options)
            .expect("stream recovery preflight");
        for (stream, expected) in streams.iter().zip(&original) {
            assert_eq!(stream.get_ref(), expected);
        }
        let measurement = measure(&config, || {
            for index in [0, data] {
                streams[index].get_mut().clear();
            }
            codec
                .reconstruct_stream(black_box(&mut streams), &options)
                .expect("recover stream");
        });
        for (stream, expected) in streams.iter().zip(&original) {
            assert_eq!(stream.get_ref(), expected);
        }
        report(&config, &case, measurement);
        completed += 1;
    }
    for (data, parity, size) in [(4, 2, 262144), (10, 4, 262144), (10, 4, 1048576)] {
        let case = Case {
            operation: "encode.parallel",
            data_shards: data,
            parity_shards: parity,
            shard_size: size,
            missing_data_shards: 0,
            missing_parity_shards: 0,
            stream_block_size: None,
        };
        if !config.includes(&case) {
            continue;
        }
        let codec = ReedSolomon::new(data, parity).expect("construct parallel codec");
        let mut shards = fixture(&codec, size);
        for shard in &mut shards[data..] {
            for byte in shard {
                *byte ^= 0xff;
            }
        }
        codec
            .encode_par(&mut shards)
            .expect("parallel encode preflight");
        assert!(codec.verify(&shards).expect("verify preflight parity"));
        let measurement = measure(&config, || {
            codec
                .encode_par(black_box(&mut shards))
                .expect("parallel encode")
        });
        assert!(codec.verify(&shards).expect("verify parallel parity"));
        report(&config, &case, measurement);
        completed += 1;
    }
    for (family, operation) in [
        (CodecFamily::Classic, "classic.construct"),
        (CodecFamily::LeopardGF8, "gf8.construct"),
        (CodecFamily::LeopardGF16, "gf16.construct"),
    ] {
        for (data, parity) in [(10, 4), (32, 16), (128, 64)] {
            let case = Case {
                operation,
                data_shards: data,
                parity_shards: parity,
                shard_size: 0,
                missing_data_shards: 0,
                missing_parity_shards: 0,
                stream_block_size: None,
            };
            if !config.includes(&case) {
                continue;
            }
            let options = CodecOptions::builder().codec_family(family).build();
            let preflight =
                ReedSolomon::with_options(data, parity, options).expect("constructor preflight");
            assert_eq!(preflight.data_shard_count(), data);
            assert_eq!(preflight.parity_shard_count(), parity);
            assert_eq!(preflight.codec_family(), family);
            drop(preflight);
            let measurement = measure(&config, || {
                let codec = ReedSolomon::with_options(
                    black_box(data),
                    black_box(parity),
                    black_box(options),
                )
                .expect("construct codec");
                drop(black_box(codec));
            });
            report(&config, &case, measurement);
            completed += 1;
        }
    }
    for (inversion_cache, operation) in [
        (true, "classic.reconstruct_parity.cached"),
        (false, "classic.reconstruct_parity.uncached"),
    ] {
        for (data, parity, size) in [(10, 4, 1024), (32, 16, 4096)] {
            let case = Case {
                operation,
                data_shards: data,
                parity_shards: parity,
                shard_size: size,
                missing_data_shards: 0,
                missing_parity_shards: 1,
                stream_block_size: None,
            };
            if !config.includes(&case) {
                continue;
            }
            let codec = ReedSolomon::with_options(
                data,
                parity,
                CodecOptions::builder()
                    .inversion_cache(inversion_cache)
                    .build(),
            )
            .expect("construct parity recovery codec");
            let original = fixture(&codec, size);
            let mut slots = shards_to_slots(&original);
            let missing = [data];
            poison_missing(&mut slots, &missing);
            codec
                .reconstruct(&mut slots)
                .expect("parity recovery preflight");
            assert_complete_slots(&slots, &original);
            let measurement = measure(&config, || {
                mark_missing_slots(&mut slots, &missing);
                codec
                    .reconstruct(black_box(&mut slots))
                    .expect("recover missing parity");
            });
            assert_complete_slots(&slots, &original);
            report(&config, &case, measurement);
            completed += 1;
        }
    }
    let case = Case {
        operation: "classic.reconstruct_some_parity",
        data_shards: 32,
        parity_shards: 16,
        shard_size: 65536,
        missing_data_shards: 1,
        missing_parity_shards: 1,
        stream_block_size: None,
    };
    if config.includes(&case) {
        let codec = ReedSolomon::new(case.data_shards, case.parity_shards)
            .expect("construct selective recovery codec");
        let original = fixture(&codec, case.shard_size);
        let mut slots = shards_to_slots(&original);
        let missing = [0, case.data_shards];
        let mut required = vec![false; codec.total_shard_count()];
        required[case.data_shards] = true;
        poison_missing(&mut slots, &missing);
        let unrequested_data = slots[0].as_inner().clone();
        codec
            .reconstruct_some(&mut slots, &required)
            .expect("selective recovery preflight");
        assert_selective_slots(&slots, &original, &unrequested_data);
        let measurement = measure(&config, || {
            mark_missing_slots(&mut slots, &missing);
            codec
                .reconstruct_some(black_box(&mut slots), black_box(&required))
                .expect("recover requested parity");
        });
        assert_selective_slots(&slots, &original, &unrequested_data);
        report(&config, &case, measurement);
        completed += 1;
    }
    // Match the ordinary generic entry point used by the pinned ECStore
    // consumer. Keep retained buffers and Option<Vec> adapter costs separate.
    for (data, parity, size, missing) in [
        (4, 2, 16384, 1),
        (4, 2, 16384, 2),
        (10, 4, 32768, 1),
        (10, 4, 32768, 2),
    ] {
        for retained in [false, true] {
            let case = Case {
                operation: if retained {
                    "classic.reconstruct_data_slots"
                } else {
                    "classic.reconstruct_data_vec"
                },
                data_shards: data,
                parity_shards: parity,
                shard_size: size,
                missing_data_shards: missing,
                missing_parity_shards: 0,
                stream_block_size: None,
            };
            if !config.includes(&case) {
                continue;
            }
            let codec = ReedSolomon::with_options(
                data,
                parity,
                CodecOptions::builder()
                    .codec_family(CodecFamily::Classic)
                    .build(),
            )
            .expect("construct generic data recovery codec");
            let original = fixture(&codec, size);
            let measurement = if retained {
                let mut slots = shards_to_slots(&original);
                let missing_indices: Vec<_> = (0..missing).collect();
                poison_missing(&mut slots, &missing_indices);
                codec
                    .reconstruct_data(&mut slots)
                    .expect("generic slot preflight");
                assert_complete_slots(&slots, &original);
                let measurement = measure(&config, || {
                    mark_missing_slots(&mut slots, &missing_indices);
                    codec
                        .reconstruct_data(black_box(&mut slots))
                        .expect("recover generic slots");
                });
                assert_complete_slots(&slots, &original);
                measurement
            } else {
                let mut shards: Vec<_> = original.iter().cloned().map(Some).collect();
                for shard in &mut shards[..missing] {
                    drop(shard.take());
                }
                codec
                    .reconstruct_data(&mut shards)
                    .expect("generic vec preflight");
                assert_complete_options(&shards, &original);
                let measurement = measure(&config, || {
                    for shard in &mut shards[..missing] {
                        drop(shard.take());
                    }
                    codec
                        .reconstruct_data(black_box(&mut shards))
                        .expect("recover generic vec");
                });
                assert_complete_options(&shards, &original);
                measurement
            };
            report(&config, &case, measurement);
            completed += 1;
        }
    }
    for (size, missing) in [(32768, 1), (32768, 2), (1048576, 1), (1048576, 2)] {
        let case = Case {
            operation: "classic.reconstruct_data_opt",
            data_shards: 10,
            parity_shards: 4,
            shard_size: size,
            missing_data_shards: missing,
            missing_parity_shards: 0,
            stream_block_size: None,
        };
        if !config.includes(&case) {
            continue;
        }
        let codec = ReedSolomon::new(case.data_shards, case.parity_shards)
            .expect("construct option-vec data recovery codec");
        let original = fixture(&codec, size);
        let mut shards: Vec<_> = original.iter().cloned().map(Some).collect();
        for shard in &mut shards[..missing] {
            for byte in shard.as_mut().expect("present preflight data") {
                *byte ^= 0xff;
            }
            drop(shard.take());
        }
        codec
            .reconstruct_data_opt(&mut shards)
            .expect("option-vec data recovery preflight");
        assert_complete_options(&shards, &original);
        let measurement = measure(&config, || {
            // This API requires Option<Vec<u8>>: replacing missing data buffers
            // is part of its measured cost, unlike retained ShardSlot storage.
            for shard in &mut shards[..missing] {
                drop(shard.take());
            }
            codec
                .reconstruct_data_opt(black_box(&mut shards))
                .expect("recover missing option-vec data");
        });
        assert_complete_options(&shards, &original);
        report(&config, &case, measurement);
        completed += 1;
    }
    for (data, parity) in [(10, 4), (12, 4), (8, 3), (8, 4), (6, 3), (4, 2)] {
        for size in [1024, 32768, 1048576] {
            let case = Case {
                operation: "encode.serial",
                data_shards: data,
                parity_shards: parity,
                shard_size: size,
                missing_data_shards: 0,
                missing_parity_shards: 0,
                stream_block_size: None,
            };
            if !config.includes(&case) {
                continue;
            }
            let codec = ReedSolomon::new(case.data_shards, case.parity_shards)
                .expect("construct serial codec");
            let original = fixture(&codec, size);
            let mut shards = original.clone();
            for shard in &mut shards[case.data_shards..] {
                for byte in shard {
                    *byte ^= 0xff;
                }
            }
            codec.encode(&mut shards).expect("serial encode preflight");
            assert_eq!(shards, original);
            assert!(
                codec
                    .verify(&shards)
                    .expect("independent serial parity check")
            );
            let measurement = measure(&config, || {
                codec.encode(black_box(&mut shards)).expect("serial encode");
            });
            assert_eq!(shards, original);
            assert!(
                codec
                    .verify(&shards)
                    .expect("independent serial parity check")
            );
            report(&config, &case, measurement);
            completed += 1;
        }
    }
    for (optimized, operation) in [
        (false, "classic.reconstruct_all_present"),
        (true, "classic.reconstruct_opt_all_present"),
    ] {
        for (data, parity) in [(10, 4), (40, 8), (128, 64)] {
            let case = Case {
                operation,
                data_shards: data,
                parity_shards: parity,
                shard_size: 65,
                missing_data_shards: 0,
                missing_parity_shards: 0,
                stream_block_size: None,
            };
            if !config.includes(&case) {
                continue;
            }
            let codec = ReedSolomon::new(data, parity).expect("construct all-present codec");
            let original = fixture(&codec, case.shard_size);
            let measurement = if optimized {
                let mut shards: Vec<_> = original.iter().cloned().map(Some).collect();
                {
                    let mut invalid = shards.clone();
                    invalid[0].as_mut().expect("present shard").pop();
                    assert_eq!(
                        codec.reconstruct_opt(&mut invalid),
                        Err(Error::IncorrectShardSize),
                    );
                }
                codec
                    .reconstruct_opt(&mut shards)
                    .expect("all-present opt preflight");
                assert_complete_options(&shards, &original);
                let measurement = measure(&config, || {
                    codec
                        .reconstruct_opt(black_box(&mut shards))
                        .expect("all-present opt recovery");
                });
                assert_complete_options(&shards, &original);
                measurement
            } else {
                let mut slots = shards_to_slots(&original);
                {
                    let mut invalid = shards_to_slots(&original);
                    invalid[0].as_inner_mut().pop();
                    assert_eq!(
                        codec.reconstruct(&mut invalid),
                        Err(Error::IncorrectShardSize),
                    );
                }
                codec
                    .reconstruct(&mut slots)
                    .expect("all-present preflight");
                assert_complete_slots(&slots, &original);
                let measurement = measure(&config, || {
                    codec
                        .reconstruct(black_box(&mut slots))
                        .expect("all-present recovery");
                });
                assert_complete_slots(&slots, &original);
                measurement
            };
            report(&config, &case, measurement);
            completed += 1;
        }
    }
    if completed == 0 {
        return Err("RSE_RECOVERY_CASE_FILTER matched no cases".to_owned());
    }
    Ok(())
}
