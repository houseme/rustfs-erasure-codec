//! A/B/B/A update probe; two updates restore the same parity for every sample.
use std::hint::black_box;
use std::io::Write as _;
use std::time::{Duration, Instant};

fn measure(batch: usize, mut op: impl FnMut()) -> ([f64; 3], [u64; 3]) {
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
            for _ in 0..batch {
                op();
            }
            n += batch as u64;
            if start.elapsed() >= Duration::from_millis(300) {
                break;
            }
        }
        *sample = start.elapsed().as_secs_f64() * 1e9 / n as f64;
        *count = n;
    }
    (samples, iterations)
}

fn main() {
    let check_only = std::env::args().skip(1).any(|arg| arg == "--check-only");
    if !check_only {
        assert_eq!(std::thread::available_parallelism().unwrap().get(), 1);
        assert_eq!(rayon::current_num_threads(), 1);
    }
    let backend = base_codec::galois_8::active_backend_name();
    assert_eq!(backend, next_codec::galois_8::active_backend_name());
    let mut cases = Vec::new();
    for (data, parity) in [(4, 2), (10, 4)] {
        for len in [64, 256, 257, 4096, 1_048_576] {
            cases.push((data, parity, len, "changed"));
        }
    }
    cases.push((4, 2, 64, "none"));
    cases.push((4, 1, 64, "fast"));
    for (data, parity, len, condition) in cases {
        let fast = condition == "fast";
        let base = base_codec::galois_8::ReedSolomon::with_options(
            data,
            parity,
            base_codec::CodecOptions::builder()
                .fast_one_parity(fast)
                .build(),
        )
        .unwrap();
        let next = next_codec::galois_8::ReedSolomon::with_options(
            data,
            parity,
            next_codec::CodecOptions::builder()
                .fast_one_parity(fast)
                .build(),
        )
        .unwrap();
        let original: Vec<Vec<u8>> = (0..data)
            .map(|row| {
                (0..len)
                    .map(|i| (row * 73 + i * 19 + i / 17) as u8)
                    .collect()
            })
            .collect();
        let mut changed = original.clone();
        if condition != "none" {
            changed[0][0] ^= 0x5a;
        }
        let old_refs: Vec<_> = original.iter().map(Vec::as_slice).collect();
        let new_refs: Vec<_> = changed.iter().map(Vec::as_slice).collect();
        let mut forward = vec![None; data];
        let mut reverse = vec![None; data];
        if condition != "none" {
            forward[0] = Some(new_refs[0]);
            reverse[0] = Some(old_refs[0]);
        }
        let mut old_parity = vec![vec![0; len]; parity];
        let mut expected = old_parity.clone();
        base.encode_sep(&old_refs, &mut old_parity).unwrap();
        base.encode_sep(&new_refs, &mut expected).unwrap();
        let mut next_old = vec![vec![0; len]; parity];
        let mut next_expected = next_old.clone();
        next.encode_sep(&old_refs, &mut next_old).unwrap();
        next.encode_sep(&new_refs, &mut next_expected).unwrap();
        assert_eq!(old_parity, next_old);
        assert_eq!(expected, next_expected);
        for which in 0..2 {
            let mut current = old_parity.clone();
            if which == 0 {
                base.update(&old_refs, &forward, &mut current).unwrap();
                assert_eq!(current, expected);
                base.update(&new_refs, &reverse, &mut current).unwrap();
            } else {
                next.update(&old_refs, &forward, &mut current).unwrap();
                assert_eq!(current, expected);
                next.update(&new_refs, &reverse, &mut current).unwrap();
            }
            assert_eq!(current, old_parity);
        }
        if check_only {
            println!("CHECK_OK small_update/{data}x{parity}/{len}/{condition} backend={backend}");
            continue;
        }
        let mut a = old_parity.clone();
        let mut b = old_parity.clone();
        // Amortize clock reads for tiny operations, while bounding overshoot
        // for the 1 MiB control.
        let batch = if len > 65_536 { 1 } else { 16 };
        let a1 = measure(batch, || {
            base.update(black_box(&old_refs), black_box(&forward), black_box(&mut a))
                .unwrap();
            base.update(black_box(&new_refs), black_box(&reverse), black_box(&mut a))
                .unwrap();
        });
        assert_eq!(a, old_parity);
        let b1 = measure(batch, || {
            next.update(black_box(&old_refs), black_box(&forward), black_box(&mut b))
                .unwrap();
            next.update(black_box(&new_refs), black_box(&reverse), black_box(&mut b))
                .unwrap();
        });
        assert_eq!(b, old_parity);
        let b2 = measure(batch, || {
            next.update(black_box(&old_refs), black_box(&forward), black_box(&mut b))
                .unwrap();
            next.update(black_box(&new_refs), black_box(&reverse), black_box(&mut b))
                .unwrap();
        });
        assert_eq!(b, old_parity);
        let a2 = measure(batch, || {
            base.update(black_box(&old_refs), black_box(&forward), black_box(&mut a))
                .unwrap();
            base.update(black_box(&new_refs), black_box(&reverse), black_box(&mut a))
                .unwrap();
        });
        assert_eq!(a, old_parity);
        println!(
            "{{\"case_id\":\"small_update/{data}x{parity}/{len}/{condition}\",\"backend\":\"{backend}\",\"A1\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"B1\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"B2\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"A2\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"chunk_len\":{len},\"jobs\":1,\"available_parallelism\":1}}",
            a1.0, a1.1, b1.0, b1.1, b2.0, b2.1, a2.0, a2.1
        );
        std::io::stdout().flush().unwrap();
        let mut acknowledgement = String::new();
        if std::io::stdin().read_line(&mut acknowledgement).unwrap() == 0
            || acknowledgement.trim() != "continue"
        {
            std::process::exit(2);
        }
    }
}
