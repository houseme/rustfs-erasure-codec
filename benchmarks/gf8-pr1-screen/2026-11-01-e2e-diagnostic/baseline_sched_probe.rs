use std::hint::black_box;
use std::io::Write as _;
use std::time::{Duration, Instant};

struct Stage {
    samples: [f64; 3],
    iterations: [u64; 3],
    warmup_iterations: u64,
    thread_sched: Vec<[u64; 3]>,
    faults: [u64; 2],
    wall_ns: u128,
}

fn scheduler_snapshot() -> std::collections::BTreeMap<u32, [u64; 3]> {
    let mut result = std::collections::BTreeMap::new();
    for entry in std::fs::read_dir("/proc/self/task").unwrap() {
        let entry = entry.unwrap();
        let tid: u32 = entry.file_name().to_string_lossy().parse().unwrap();
        let text = std::fs::read_to_string(entry.path().join("schedstat")).unwrap();
        let values: Vec<u64> = text
            .split_whitespace()
            .map(|value| value.parse().unwrap())
            .collect();
        assert_eq!(values.len(), 3);
        result.insert(tid, [values[0], values[1], values[2]]);
    }
    result
}

fn parse_faults(text: &str) -> Option<[u64; 2]> {
    let body = text.get(text.rfind(") ")? + 2..)?;
    let fields: Vec<_> = body.split_whitespace().collect();
    Some([fields.get(7)?.parse().ok()?, fields.get(9)?.parse().ok()?])
}

fn faults() -> [u64; 2] {
    parse_faults(&std::fs::read_to_string("/proc/self/stat").unwrap()).unwrap()
}

fn measure(mut op: impl FnMut()) -> Stage {
    let before_sched = scheduler_snapshot();
    let before_faults = faults();
    let started = Instant::now();
    let warmup = Instant::now();
    let mut warmup_iterations = 0;
    while warmup.elapsed() < Duration::from_secs(2) {
        op();
        warmup_iterations += 1;
    }
    let mut samples = [0.; 3];
    let mut iterations = [0; 3];
    for (sample, count) in samples.iter_mut().zip(&mut iterations) {
        let start = Instant::now();
        let mut iters = 0u64;
        loop {
            for _ in 0..4 {
                op();
            }
            iters += 4;
            if start.elapsed() >= Duration::from_millis(500) {
                break;
            }
        }
        *sample = start.elapsed().as_secs_f64() * 1e9 / iters as f64;
        *count = iters;
    }
    let wall_ns = started.elapsed().as_nanos();
    let after_faults = faults();
    let after_sched = scheduler_snapshot();
    assert_eq!(before_sched.len(), after_sched.len(), "thread set changed");
    let thread_sched = before_sched
        .into_iter()
        .map(|(tid, before)| {
            let after = after_sched.get(&tid).expect("thread set changed");
            std::array::from_fn(|i| after[i].checked_sub(before[i]).unwrap())
        })
        .collect();
    Stage {
        samples,
        iterations,
        warmup_iterations,
        thread_sched,
        faults: std::array::from_fn(|i| after_faults[i].checked_sub(before_faults[i]).unwrap()),
        wall_ns,
    }
}

fn stage_json(stage: &Stage) -> String {
    format!(
        "{{\"samples_ns\":{:?},\"iterations\":{:?},\"warmup_iterations\":{},\"thread_sched\":{:?},\"faults\":{:?},\"wall_ns\":{}}}",
        stage.samples,
        stage.iterations,
        stage.warmup_iterations,
        stage.thread_sched,
        stage.faults,
        stage.wall_ns
    )
}

#[cfg(test)]
mod tests {
    use super::parse_faults;
    #[test]
    fn parses_fault_fields_after_parenthesized_comm() {
        assert_eq!(
            parse_faults("123 (worker ) name) R 1 2 3 4 5 6 7 8 9 10 11"),
            Some([7, 9])
        );
        assert_eq!(parse_faults("123 (truncated) R 1"), None);
        assert_eq!(parse_faults("missing parentheses"), None);
    }
}

fn main() {
    assert_eq!(base_codec::galois_8::active_backend_name(), "rust-avx2");
    assert_eq!(next_codec::galois_8::active_backend_name(), "rust-avx2");
    assert_eq!(rayon::current_num_threads(), 4);
    let available = std::thread::available_parallelism().unwrap().get();
    assert_eq!(available, 5);
    for (data_count, parity_count, len, missing) in [
        (4, 2, 1_048_576, 0),
        (10, 4, 1_048_576, 0),
        (10, 4, 1_048_577, 3),
    ] {
        let base = base_codec::galois_8::ReedSolomon::with_options(
            data_count,
            parity_count,
            base_codec::CodecOptions::builder()
                .max_parallel_jobs(4)
                .build(),
        )
        .unwrap();
        let next = next_codec::galois_8::ReedSolomon::with_options(
            data_count,
            parity_count,
            next_codec::CodecOptions::builder()
                .max_parallel_jobs(4)
                .build(),
        )
        .unwrap();
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
        let a1 = measure(|| {
            shards[missing] = None;
            base.reconstruct_data_opt(black_box(&mut shards)).unwrap();
        });
        assert_eq!(
            shards[missing].as_deref(),
            Some(original[missing].as_slice())
        );
        let b1 = measure(|| {
            shards[missing] = None;
            next.reconstruct_data_opt(black_box(&mut shards)).unwrap();
        });
        assert_eq!(
            shards[missing].as_deref(),
            Some(original[missing].as_slice())
        );
        let b2 = measure(|| {
            shards[missing] = None;
            next.reconstruct_data_opt(black_box(&mut shards)).unwrap();
        });
        assert_eq!(
            shards[missing].as_deref(),
            Some(original[missing].as_slice())
        );
        let a2 = measure(|| {
            shards[missing] = None;
            base.reconstruct_data_opt(black_box(&mut shards)).unwrap();
        });
        assert_eq!(
            shards[missing].as_deref(),
            Some(original[missing].as_slice())
        );
        println!(
            "{{\"case_id\":\"end_to_end_recovery/{data_count}x{parity_count}/{len}/missing{missing}\",\"A1\":{},\"B1\":{},\"B2\":{},\"A2\":{},\"chunk_len\":{},\"jobs\":{},\"available_parallelism\":{available}}}",
            stage_json(&a1),
            stage_json(&b1),
            stage_json(&b2),
            stage_json(&a2),
            decision.chunk_len,
            decision.jobs
        );
        std::io::stdout().flush().unwrap();
        // The parent validates this case before permitting the next case.
        let mut acknowledgement = String::new();
        if std::io::stdin().read_line(&mut acknowledgement).unwrap() == 0
            || acknowledgement.trim() != "continue"
        {
            return;
        }
    }
}
