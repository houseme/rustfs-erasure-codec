//! Local A1/B1/B2/A2 screen: published 9.0.0 vs path 9.1.0 encode/reconstruct/verify.
//!
//! Not release acceptance. Requires both crates linked as `base_codec` / `next_codec`
//! with matching `simd-accel` features. Unset `RSE_BACKEND_OVERRIDE`.
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

fn measure(mut operation: impl FnMut()) -> ([f64; 3], [u64; 3]) {
    for _ in 0..50 {
        operation();
    }
    let mut samples = [0.0; 3];
    let mut counts = [0u64; 3];
    for (value, count) in samples.iter_mut().zip(&mut counts) {
        let begin = Instant::now();
        let mut iterations = 0u64;
        loop {
            for _ in 0..8 {
                operation();
            }
            iterations += 8;
            if begin.elapsed() >= Duration::from_millis(300) {
                break;
            }
        }
        *value = begin.elapsed().as_secs_f64() * 1e9 / iterations as f64;
        *count = iterations;
    }
    (samples, counts)
}

fn median(samples: [f64; 3]) -> f64 {
    let mut s = samples;
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s[1]
}

fn phase_ratio(a: [f64; 3], b: [f64; 3]) -> f64 {
    median(b) / median(a)
}

fn drift(a: [f64; 3], b: [f64; 3]) -> f64 {
    let m1 = median(a);
    let m2 = median(b);
    (m2 - m1).abs() / m1.max(m2).max(1e-12)
}

fn make_original(data: usize, parity: usize, len: usize) -> Vec<Vec<u8>> {
    (0..data + parity)
        .map(|row| {
            (0..len)
                .map(|offset| (row * 73 + offset * 19 + offset / 17) as u8)
                .collect()
        })
        .collect()
}

/// Local screen catalog: smoke + small-file + recovery-adjacent shapes.
fn shapes() -> Vec<(usize, usize, usize, &'static str)> {
    let mut out = Vec::new();
    for (d, p) in [(4usize, 2usize), (10, 4), (6, 2), (8, 4), (12, 4), (32, 16)] {
        for len in [1024usize, 4096, 32 * 1024, 64 * 1024, 512 * 1024, 1024 * 1024] {
            // Skip the heaviest wide shape at 1 MiB to keep the local screen bounded.
            if d >= 32 && len > 64 * 1024 {
                continue;
            }
            if d >= 12 && len > 512 * 1024 {
                continue;
            }
            let label = if len < 1024 {
                format!("{d}x{p}_{len}b")
            } else if len >= 1024 * 1024 {
                format!("{d}x{p}_{}m", len / (1024 * 1024))
            } else {
                format!("{d}x{p}_{}k", len / 1024)
            };
            let label: &'static str = Box::leak(label.into_boxed_str());
            out.push((d, p, len, label));
        }
    }
    out
}

fn drift_threshold() -> f64 {
    std::env::var("RSE_ABBA_LOCAL_DRIFT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.08)
}

fn main() {
    assert!(
        std::env::var_os("RSE_BACKEND_OVERRIDE").is_none(),
        "unset RSE_BACKEND_OVERRIDE for generated-kernel comparisons"
    );
    let base_backend = base_codec::galois_8::active_backend_name();
    let next_backend = next_codec::galois_8::active_backend_name();
    assert_eq!(
        base_backend, next_backend,
        "backend mismatch: {base_backend} vs {next_backend}"
    );
    let threshold = drift_threshold();
    println!(
        "{{\"meta\":{{\"schema\":\"rse-local-abba-v1\",\"baseline\":\"9.0.0\",\"candidate\":\"9.1.0\",\"backend\":\"{}\",\"arch\":\"{}\",\"rayon_threads\":\"{}\",\"drift_threshold_pct\":{:.1},\"release_approval\":false}}}}",
        base_backend,
        std::env::consts::ARCH,
        std::env::var("RAYON_NUM_THREADS").unwrap_or_else(|_| "default".into()),
        threshold * 100.0
    );

    let mut rows: Vec<(String, f64, f64, f64, f64, bool)> = Vec::new();
    let mut max_drift = 0.0f64;

    for (data, parity, len, label) in shapes() {
        let before = base_codec::galois_8::ReedSolomon::new(data, parity).unwrap();
        let after = next_codec::galois_8::ReedSolomon::new(data, parity).unwrap();
        let original = make_original(data, parity, len);

        let mut a = original.clone();
        let mut b = original.clone();
        before.encode(&mut a).unwrap();
        after.encode(&mut b).unwrap();
        assert_eq!(a, b, "encode parity mismatch {label}");
        assert!(before.verify(&a).unwrap());
        assert!(after.verify(&b).unwrap());

        let mut ra: Vec<Option<Vec<u8>>> = a.clone().into_iter().map(Some).collect();
        let mut rb: Vec<Option<Vec<u8>>> = b.clone().into_iter().map(Some).collect();
        ra[0] = None;
        rb[0] = None;
        if parity >= 2 {
            ra[data] = None;
            rb[data] = None;
        }
        before.reconstruct(&mut ra).unwrap();
        after.reconstruct(&mut rb).unwrap();
        for i in 0..data + parity {
            assert_eq!(ra[i].as_ref(), Some(&a[i]), "base reconstruct {label} shard {i}");
            assert_eq!(rb[i].as_ref(), Some(&b[i]), "next reconstruct {label} shard {i}");
        }

        before.encode(&mut a).unwrap();
        let a1 = measure(|| before.encode(black_box(&mut a)).unwrap());
        let b1 = measure(|| after.encode(black_box(&mut a)).unwrap());
        let b2 = measure(|| after.encode(black_box(&mut a)).unwrap());
        let a2 = measure(|| before.encode(black_box(&mut a)).unwrap());
        assert!(before.verify(&a).unwrap());
        assert!(after.verify(&a).unwrap());

        let full = a.clone();
        let prep = || -> Vec<Option<Vec<u8>>> {
            let mut v: Vec<Option<Vec<u8>>> = full.iter().cloned().map(Some).collect();
            v[0] = None;
            if parity >= 2 {
                v[data] = None;
            }
            v
        };
        for _ in 0..5 {
            let mut t = prep();
            before.reconstruct(&mut t).unwrap();
            let mut t = prep();
            after.reconstruct(&mut t).unwrap();
        }
        let r1 = measure(|| {
            let mut t = prep();
            before.reconstruct(black_box(&mut t)).unwrap();
        });
        let r2 = measure(|| {
            let mut t = prep();
            after.reconstruct(black_box(&mut t)).unwrap();
        });
        let r3 = measure(|| {
            let mut t = prep();
            after.reconstruct(black_box(&mut t)).unwrap();
        });
        let r4 = measure(|| {
            let mut t = prep();
            before.reconstruct(black_box(&mut t)).unwrap();
        });

        let v1 = measure(|| {
            assert!(before.verify(black_box(&a)).unwrap());
        });
        let v2 = measure(|| {
            assert!(after.verify(black_box(&a)).unwrap());
        });
        let v3 = measure(|| {
            assert!(after.verify(black_box(&a)).unwrap());
        });
        let v4 = measure(|| {
            assert!(before.verify(black_box(&a)).unwrap());
        });

        let enc_drift = drift(a1.0, a2.0).max(drift(b1.0, b2.0));
        let rec_drift = drift(r1.0, r4.0).max(drift(r2.0, r3.0));
        let ver_drift = drift(v1.0, v4.0).max(drift(v2.0, v3.0));
        // Local screen gates encode/reconstruct; verify-only drift is reported but not fatal.
        let case_max_drift = enc_drift.max(rec_drift);
        max_drift = max_drift.max(case_max_drift);

        let enc_ratio = phase_ratio(a1.0, b1.0);
        let enc_ratio2 = phase_ratio(a2.0, b2.0);
        let rec_ratio = phase_ratio(r1.0, r2.0);
        let rec_ratio2 = phase_ratio(r4.0, r3.0);
        let ver_ratio = phase_ratio(v1.0, v2.0);
        let ver_ratio2 = phase_ratio(v4.0, v3.0);

        let enc_ns = median(a1.0).min(median(a2.0));
        let enc_ns_new = median(b1.0).min(median(b2.0));
        let rec_ns = median(r1.0).min(median(r4.0));
        let rec_ns_new = median(r2.0).min(median(r3.0));
        let ver_ns = median(v1.0).min(median(v4.0));
        let ver_ns_new = median(v2.0).min(median(v3.0));
        let payload = (data * len) as f64;

        let refused = case_max_drift > threshold;
        let enc_delta = (enc_ratio - 1.0) * 100.0;
        let rec_delta = (rec_ratio - 1.0) * 100.0;
        let ver_delta = (ver_ratio - 1.0) * 100.0;
        println!(
            "{{\"case_id\":\"{label}/{data}x{parity}/{len}\",\"encode\":{{\"base_ns\":{enc_ns:.1},\"next_ns\":{enc_ns_new:.1},\"ratio_b_over_a\":{enc_ratio:.4},\"ratio_b2_over_a2\":{enc_ratio2:.4},\"base_GBps\":{enc_gbps_old:.3},\"next_GBps\":{enc_gbps_new:.3},\"delta_pct\":{enc_delta:.2},\"A1\":{{\"samples_ns\":{a1_samples:?},\"iterations\":{a1_iters:?}}},\"B1\":{{\"samples_ns\":{b1_samples:?},\"iterations\":{b1_iters:?}}},\"B2\":{{\"samples_ns\":{b2_samples:?},\"iterations\":{b2_iters:?}}},\"A2\":{{\"samples_ns\":{a2_samples:?},\"iterations\":{a2_iters:?}}}}},\"reconstruct\":{{\"base_ns\":{rec_ns:.1},\"next_ns\":{rec_ns_new:.1},\"ratio_b_over_a\":{rec_ratio:.4},\"ratio_b2_over_a2\":{rec_ratio2:.4},\"base_GBps\":{rec_gbps_old:.3},\"next_GBps\":{rec_gbps_new:.3},\"delta_pct\":{rec_delta:.2},\"A1\":{{\"samples_ns\":{r1_samples:?},\"iterations\":{r1_iters:?}}},\"B1\":{{\"samples_ns\":{r2_samples:?},\"iterations\":{r2_iters:?}}},\"B2\":{{\"samples_ns\":{r3_samples:?},\"iterations\":{r3_iters:?}}},\"A2\":{{\"samples_ns\":{r4_samples:?},\"iterations\":{r4_iters:?}}}}},\"verify\":{{\"base_ns\":{ver_ns:.1},\"next_ns\":{ver_ns_new:.1},\"ratio_b_over_a\":{ver_ratio:.4},\"ratio_b2_over_a2\":{ver_ratio2:.4},\"delta_pct\":{ver_delta:.2},\"drift_pct\":{ver_drift_pct:.2}}},\"max_drift_pct\":{case_drift_pct:.2},\"verify_drift_pct\":{ver_drift_pct:.2},\"drift_refused\":{refused}}}",
            label = label,
            data = data,
            parity = parity,
            len = len,
            enc_ns = enc_ns,
            enc_ns_new = enc_ns_new,
            enc_ratio = enc_ratio,
            enc_ratio2 = enc_ratio2,
            enc_gbps_old = payload / enc_ns,
            enc_gbps_new = payload / enc_ns_new,
            enc_delta = enc_delta,
            a1_samples = a1.0,
            a1_iters = a1.1,
            b1_samples = b1.0,
            b1_iters = b1.1,
            b2_samples = b2.0,
            b2_iters = b2.1,
            a2_samples = a2.0,
            a2_iters = a2.1,
            rec_ns = rec_ns,
            rec_ns_new = rec_ns_new,
            rec_ratio = rec_ratio,
            rec_ratio2 = rec_ratio2,
            rec_gbps_old = payload / rec_ns,
            rec_gbps_new = payload / rec_ns_new,
            rec_delta = rec_delta,
            r1_samples = r1.0,
            r1_iters = r1.1,
            r2_samples = r2.0,
            r2_iters = r2.1,
            r3_samples = r3.0,
            r3_iters = r3.1,
            r4_samples = r4.0,
            r4_iters = r4.1,
            ver_ns = ver_ns,
            ver_ns_new = ver_ns_new,
            ver_ratio = ver_ratio,
            ver_ratio2 = ver_ratio2,
            ver_delta = ver_delta,
            ver_drift_pct = ver_drift * 100.0,
            case_drift_pct = case_max_drift * 100.0,
            refused = refused,
        );

        if refused {
            eprintln!(
                "DRIFT REFUSAL {label}: {:.2}% > {:.1}% (enc {:.2}%% rec {:.2}%%) — retained, not attributed",
                case_max_drift * 100.0,
                threshold * 100.0,
                enc_drift * 100.0,
                rec_drift * 100.0
            );
        }
        rows.push((label.to_string(), enc_ratio, rec_ratio, ver_ratio, case_max_drift, refused));
    }

    println!("=== SUMMARY (candidate/baseline; >1 = candidate slower) ===");
    let mut sum_enc = 0.0;
    let mut sum_rec = 0.0;
    let mut sum_ver = 0.0;
    for (label, e, r, v, d, refused) in &rows {
        let _ = refused;
        sum_enc += e;
        sum_rec += r;
        sum_ver += v;
        println!(
            "{label}: encode x{e:.3} ({:+.1}%), reconstruct x{r:.3} ({:+.1}%), verify x{v:.3} ({:+.1}%), drift {:.1}%",
            (e - 1.0) * 100.0,
            (r - 1.0) * 100.0,
            (v - 1.0) * 100.0,
            d * 100.0
        );
    }
    let n = rows.len() as f64;
    println!(
        "MEAN encode x{:.3} ({:+.1}%), reconstruct x{:.3} ({:+.1}%), verify x{:.3} ({:+.1}%), max_drift {:.1}%",
        sum_enc / n,
        (sum_enc / n - 1.0) * 100.0,
        sum_rec / n,
        (sum_rec / n - 1.0) * 100.0,
        sum_ver / n,
        (sum_ver / n - 1.0) * 100.0,
        max_drift * 100.0
    );
    let refusals = rows.iter().filter(|r| r.5).count();
    println!(
        "{{\"summary\":{{\"schema\":\"rse-local-abba-v1\",\"cases\":{},\"drift_refusals\":{},\"mean_encode_ratio\":{:.4},\"mean_reconstruct_ratio\":{:.4},\"mean_verify_ratio\":{:.4},\"max_drift_pct\":{:.2},\"release_approval\":false}}}}",
        rows.len(),
        refusals,
        sum_enc / n,
        sum_rec / n,
        sum_ver / n,
        max_drift * 100.0
    );
    if refusals > 0 {
        std::process::exit(3);
    }
}
