//! Constructor A/B/B/A probe; validates parity and recovery before timing.
use std::hint::black_box;
use std::io::Write as _;
use std::time::{Duration, Instant};

fn measure(mut op: impl FnMut()) -> ([f64; 3], [u64; 3]) {
    let warmup = Instant::now();
    while warmup.elapsed() < Duration::from_secs(1) {
        op();
    }
    let mut samples = [0.0; 3];
    let mut iterations = [0; 3];
    for (sample, count) in samples.iter_mut().zip(&mut iterations) {
        let start = Instant::now();
        let mut n = 0u64;
        loop {
            op();
            n += 1;
            if start.elapsed() >= Duration::from_millis(300) {
                break;
            }
        }
        *sample = start.elapsed().as_secs_f64() * 1e9 / n as f64;
        *count = n;
    }
    (samples, iterations)
}

macro_rules! run_case {
    ($base:ty, $next:ty, $field:expr, $element:expr, $data:expr, $parity:expr, $check:expr, $backend:expr) => {{
        let data_count = $data;
        let parity_count = $parity;
        let base = <$base>::new(data_count, parity_count).unwrap();
        let next = <$next>::new(data_count, parity_count).unwrap();
        let input: Vec<Vec<_>> = (0..data_count + parity_count)
            .map(|row| (0..65).map(|i| ($element)(row, i)).collect())
            .collect();
        let mut base_shards = input.clone();
        let mut next_shards = input;
        base.encode(&mut base_shards).unwrap();
        next.encode(&mut next_shards).unwrap();
        assert_eq!(base_shards, next_shards);
        let mut lost: Vec<_> = base_shards.iter().cloned().map(Some).collect();
        lost[0] = None;
        next.reconstruct_data(&mut lost).unwrap();
        assert_eq!(lost[0].as_deref(), Some(base_shards[0].as_slice()));
        let mut lost: Vec<_> = next_shards.iter().cloned().map(Some).collect();
        lost[0] = None;
        base.reconstruct_data(&mut lost).unwrap();
        assert_eq!(lost[0].as_deref(), Some(next_shards[0].as_slice()));
        if $check {
            println!("CHECK_OK matrix_create/{}/{data_count}x{parity_count} backend={}", $field, $backend);
        } else {
            let a1 = measure(|| { black_box(<$base>::new(data_count, parity_count).unwrap()); });
            let b1 = measure(|| { black_box(<$next>::new(data_count, parity_count).unwrap()); });
            let b2 = measure(|| { black_box(<$next>::new(data_count, parity_count).unwrap()); });
            let a2 = measure(|| { black_box(<$base>::new(data_count, parity_count).unwrap()); });
            println!(
                "{{\"case_id\":\"matrix_create/{}/{data_count}x{parity_count}\",\"backend\":\"{}\",\"A1\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"B1\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"B2\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"A2\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"chunk_len\":{},\"jobs\":1,\"available_parallelism\":1}}",
                $field, $backend, a1.0,a1.1,b1.0,b1.1,b2.0,b2.1,a2.0,a2.1, data_count * (data_count + parity_count)
            );
            std::io::stdout().flush().unwrap();
            let mut acknowledgement = String::new();
            if std::io::stdin().read_line(&mut acknowledgement).unwrap() == 0 || acknowledgement.trim() != "continue" {
                std::process::exit(2);
            }
        }
    }};
}

fn main() {
    let check_only = std::env::args().skip(1).any(|arg| arg == "--check-only");
    if !check_only {
        assert_eq!(std::thread::available_parallelism().unwrap().get(), 1);
        assert_eq!(rayon::current_num_threads(), 1);
    }
    let backend = base_codec::galois_8::active_backend_name();
    assert_eq!(backend, next_codec::galois_8::active_backend_name());
    for (data, parity) in [(4, 2), (6, 2), (10, 4), (16, 8), (32, 16)] {
        run_case!(
            base_codec::galois_8::ReedSolomon,
            next_codec::galois_8::ReedSolomon,
            "gf8",
            |r: usize, i: usize| (r * 37 + i * 13 + 1) as u8,
            data,
            parity,
            check_only,
            backend
        );
        run_case!(
            base_codec::galois_16::ReedSolomon,
            next_codec::galois_16::ReedSolomon,
            "gf16",
            |r: usize, i: usize| [(r * 37 + i * 13 + 1) as u8, (r * 11 + i * 17) as u8],
            data,
            parity,
            check_only,
            backend
        );
    }
}
