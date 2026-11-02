use std::hint::black_box;
use std::time::{Duration, Instant};
fn measure(mut f: impl FnMut()) -> f64 {
    let warm = Instant::now();
    while warm.elapsed() < Duration::from_millis(300) {
        f();
    }
    let mut samples = [0.0; 3];
    for sample in &mut samples {
        let start = Instant::now();
        let mut n = 0u64;
        while start.elapsed() < Duration::from_millis(250) {
            for _ in 0..32 {
                f();
            }
            n += 32;
        }
        *sample = start.elapsed().as_secs_f64() * 1e9 / n as f64;
    }
    samples.sort_by(f64::total_cmp);
    samples[1]
}
fn main() {
    let candidate = std::env::args().any(|s| s == "candidate");
    for (data, parity, len, fast) in [
        (4, 2, 64, false),
        (4, 2, 256, false),
        (10, 4, 64, false),
        (10, 4, 256, false),
        (4, 1, 64, true),
        (2, 33, 64, false),
        (4, 2, 4096, false),
    ] {
        let a = base_codec::galois_8::ReedSolomon::with_options(
            data,
            parity,
            base_codec::CodecOptions::builder()
                .fast_one_parity(fast)
                .max_parallel_jobs(1)
                .build(),
        )
        .unwrap();
        let b = next_codec::galois_8::ReedSolomon::with_options(
            data,
            parity,
            next_codec::CodecOptions::builder()
                .fast_one_parity(fast)
                .max_parallel_jobs(1)
                .build(),
        )
        .unwrap();
        let mut shards = vec![vec![0u8; len]; data + parity];
        for (r, s) in shards.iter_mut().take(data).enumerate() {
            for (i, v) in s.iter_mut().enumerate() {
                *v = (r * 37 + i * 13) as u8;
            }
        }
        a.encode(&mut shards).unwrap();
        let mut wa = base_codec::VerifyWorkspace::new(&a, len);
        let mut wb = next_codec::VerifyWorkspace::new(&b, len);
        assert!(a.verify_with_workspace_opt(&shards, &mut wa).unwrap());
        assert!(b.verify_with_workspace_opt(&shards, &mut wb).unwrap());
        let a1 = measure(|| {
            black_box(
                a.verify_with_workspace_opt(black_box(&shards), black_box(&mut wa))
                    .unwrap(),
            );
        });
        let b1 = measure(|| {
            black_box(
                b.verify_with_workspace_opt(black_box(&shards), black_box(&mut wb))
                    .unwrap(),
            );
        });
        let b2 = measure(|| {
            black_box(
                b.verify_with_workspace_opt(black_box(&shards), black_box(&mut wb))
                    .unwrap(),
            );
        });
        let a2 = measure(|| {
            black_box(
                a.verify_with_workspace_opt(black_box(&shards), black_box(&mut wa))
                    .unwrap(),
            );
        });
        let da = (a2 / a1 - 1.) * 100.;
        let db = (b2 / b1 - 1.) * 100.;
        let diff = ((b1 + b2) / (a1 + a2) - 1.) * 100.;
        println!(
            "{data}+{parity}/{len} fast={fast} A_drift={da:.2}% B_drift={db:.2}% difference={diff:.2}%"
        );
        if da.abs() > 5. || db.abs() > 5. || (!candidate && diff.abs() > 5.) {
            eprintln!("STOP: same-source or phase gate failed");
            std::process::exit(2)
        }
    }
}
