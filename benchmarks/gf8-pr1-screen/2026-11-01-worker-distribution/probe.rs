use std::hint::black_box;
use std::io::Write as _;
use std::time::{Duration, Instant};

fn measure(mut op: impl FnMut(), reset: fn(), groups: fn() -> [u64; 4]) -> ([f64; 3], [u64; 3], [[u64; 4]; 3]) {
    let warmup = Instant::now();
    while warmup.elapsed() < Duration::from_secs(1) {
        op();
    }
    let mut samples = [0.; 3];
    let mut counts = [0; 3];
    let mut distribution = [[0; 4]; 3];
    for ((sample, count), distribution) in samples.iter_mut().zip(&mut counts).zip(&mut distribution) {
        reset();
        let start = Instant::now();
        let mut iters = 0u64;
        loop {
            for _ in 0..4 {
                op();
            }
            iters += 4;
            if start.elapsed() >= Duration::from_millis(300) {
                break;
            }
        }
        *sample = start.elapsed().as_secs_f64() * 1e9 / iters as f64;
        *count = iters;
        *distribution = groups();
    }
    (samples, counts, distribution)
}

// Diagnostic only: pin the current Linux thread via taskset, outside timed work.
fn pin_current(cpu: usize) {
    let link = std::fs::read_link("/proc/thread-self").unwrap();
    let tid = link.file_name().unwrap().to_str().unwrap();
    let status = std::process::Command::new("taskset")
        .args(["-pc", &cpu.to_string(), tid])
        .output()
        .unwrap();
    assert!(status.status.success(), "pin thread failed");
    let text = std::fs::read_to_string("/proc/thread-self/status").unwrap();
    let allowed = text
        .lines()
        .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
        .unwrap()
        .trim();
    assert_eq!(allowed, cpu.to_string());
}

fn main() {
    assert_eq!(base_codec::galois_8::active_backend_name(), "rust-avx2");
    assert_eq!(next_codec::galois_8::active_backend_name(), "rust-avx2");
    let available = std::thread::available_parallelism().unwrap().get();
    assert_eq!(available, 5);
    let specs = [
        (10, 4, 1_048_576, 0),
        (10, 4, 1_048_577, 3),
        (4, 2, 1_048_576, 0),
    ];
    // Construct codecs while five vCPUs are visible; each codec caches this budget.
    let codecs: Vec<_> = specs
        .iter()
        .map(|&(data, parity, _, _)| {
            let base = base_codec::galois_8::ReedSolomon::with_options(
                data,
                parity,
                base_codec::CodecOptions::builder()
                    .max_parallel_jobs(4)
                    .build(),
            )
            .unwrap();
            let next = next_codec::galois_8::ReedSolomon::with_options(
                data,
                parity,
                next_codec::CodecOptions::builder()
                    .max_parallel_jobs(4)
                    .build(),
            )
            .unwrap();
            (base, next)
        })
        .collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .start_handler(|index| pin_current(8 + index))
        .build()
        .unwrap();
    let mut observed = pool.broadcast(|ctx| {
        let text = std::fs::read_to_string("/proc/thread-self/status").unwrap();
        let allowed = text
            .lines()
            .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
            .unwrap()
            .trim();
        assert_eq!(allowed, (8 + ctx.index()).to_string());
        ctx.index()
    });
    observed.sort_unstable();
    assert_eq!(observed, [0, 1, 2, 3]);
    pin_current(12);
    for ((data_count, parity_count, len, missing), (base, next)) in specs.into_iter().zip(codecs) {
        pool.install(|| {
            assert_eq!(rayon::current_num_threads(), 4);
        let mut original: Vec<Vec<u8>> = (0..data_count + parity_count)
            .map(|row| {
                (0..len)
                    .map(|i| (row * 73 + i * 19 + i / 17) as u8)
                    .collect()
            })
            .collect();
        base.encode(&mut original).unwrap();
        let mut next_encoded = original.clone();
        next.encode(&mut next_encoded).unwrap();
        assert_eq!(next_encoded, original);
        drop(next_encoded);
        let mut shards: Vec<Option<Vec<u8>>> = original.iter().cloned().map(Some).collect();
        shards[missing] = None;
        let (indices, row) = base_codec::diagnostic_row::recovery_row(&base, &mut shards).unwrap();
        let (next_indices, next_row) =
            next_codec::diagnostic_row::recovery_row(&next, &mut shards).unwrap();
        assert_eq!(indices, next_indices);
        assert_eq!(row, next_row);
        let inputs: Vec<&[u8]> = indices.iter().map(|&i| original[i].as_slice()).collect();
        let decision = base
            .reconstruct_stage_policies_for_bench(true)
            .0
            .decide(len, data_count, 1, available);
        let next_decision = next
            .reconstruct_stage_policies_for_bench(true)
            .0
            .decide(len, data_count, 1, available);
        assert!(decision.use_parallel && decision.jobs == 4);
        assert_eq!(decision.chunk_len, next_decision.chunk_len);
        assert_eq!(decision.jobs, next_decision.jobs);
        let mut output = vec![0xa5; len];
        let a1 = measure(|| {
            base_codec::diagnostic_row::run_row(
                &base,
                &row,
                &inputs,
                black_box(&mut output),
                decision.chunk_len,
                decision.jobs,
            )
        }, base_codec::diagnostic_row::reset_groups, base_codec::diagnostic_row::groups);
        assert_eq!(output, original[missing]);
        let b1 = measure(|| {
            next_codec::diagnostic_row::run_row(
                &next,
                &row,
                &inputs,
                black_box(&mut output),
                decision.chunk_len,
                decision.jobs,
            )
        }, next_codec::diagnostic_row::reset_groups, next_codec::diagnostic_row::groups);
        assert_eq!(output, original[missing]);
        let b2 = measure(|| {
            next_codec::diagnostic_row::run_row(
                &next,
                &row,
                &inputs,
                black_box(&mut output),
                decision.chunk_len,
                decision.jobs,
            )
        }, next_codec::diagnostic_row::reset_groups, next_codec::diagnostic_row::groups);
        assert_eq!(output, original[missing]);
        let a2 = measure(|| {
            base_codec::diagnostic_row::run_row(
                &base,
                &row,
                &inputs,
                black_box(&mut output),
                decision.chunk_len,
                decision.jobs,
            )
        }, base_codec::diagnostic_row::reset_groups, base_codec::diagnostic_row::groups);
        assert_eq!(output, original[missing]);
        for phase in [&a1, &b1, &b2, &a2] {
            let chunk_count = len.div_ceil(decision.chunk_len);
            let group_len = chunk_count.div_ceil(decision.jobs) * decision.chunk_len;
            let groups_per_op = len.div_ceil(group_len) as u64;
            for (counts, iterations) in phase.2.iter().zip(phase.1) {
                assert_eq!(counts.iter().sum::<u64>(), iterations * groups_per_op);
            }
        }
        eprintln!("distribution {data_count}x{parity_count}/{len} A1={:?} B1={:?} B2={:?} A2={:?}", a1.2, b1.2, b2.2, a2.2);
        println!(
            "{{\"case_id\":\"retained_recovery_row/{data_count}x{parity_count}/{len}/missing{missing}\",\"A1\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"B1\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"B2\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"A2\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"chunk_len\":{},\"jobs\":{},\"available_parallelism\":{available}}}",
            a1.0, a1.1, b1.0, b1.1, b2.0, b2.1, a2.0, a2.1, decision.chunk_len, decision.jobs
        );
        std::io::stdout().flush().unwrap();
        // The parent validates this case before permitting the next case.
        let mut acknowledgement = String::new();
        if std::io::stdin().read_line(&mut acknowledgement).unwrap() == 0
            || acknowledgement.trim() != "continue"
        {
            std::process::exit(2);
        }
        });
    }
}
