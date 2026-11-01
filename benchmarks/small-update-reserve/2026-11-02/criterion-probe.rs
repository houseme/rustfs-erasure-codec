use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rustfs_erasure_codec::{CodecOptions, Field, ReedSolomon, galois_8, galois_16};
use std::hint::black_box;

fn field<F: Field>(c: &mut Criterion, name: &str) {
    let mut group = c.benchmark_group("small_update");
    for (data, parity) in [(4usize, 2usize), (10, 4), (4, 1)] {
        let codec = ReedSolomon::<F>::with_options(
            data,
            parity,
            CodecOptions::builder()
                .fast_one_parity(parity == 1)
                .max_parallel_jobs(1)
                .build(),
        )
        .unwrap();
        for len in [64usize, 256, 257, 4096, 1_048_576] {
            if parity == 1 && len != 64 {
                continue;
            }
            let old: Vec<Vec<F::Elem>> = (0..data)
                .map(|i| {
                    (0..len)
                        .map(|j| F::nth((i * 31 + j * 17 + j / 11) % F::ORDER))
                        .collect()
                })
                .collect();
            for changed in ["one", "identical", "none"] {
                if changed != "one" && !(data == 4 && parity == 2 && len == 64) {
                    continue;
                }
                let mut new = old.clone();
                if changed == "one" {
                    new[0][0] = F::add(new[0][0], F::one());
                }
                let old_refs: Vec<_> = old.iter().map(Vec::as_slice).collect();
                let new_refs: Vec<_> = new.iter().map(Vec::as_slice).collect();
                let mut forward = vec![None; data];
                let mut reverse = vec![None; data];
                if changed != "none" {
                    forward[0] = Some(new[0].as_slice());
                    reverse[0] = Some(old[0].as_slice());
                }
                let mut initial = vec![vec![F::zero(); len]; parity];
                let mut expected = initial.clone();
                codec.encode_sep(&old, &mut initial).unwrap();
                codec.encode_sep(&new, &mut expected).unwrap();
                let mut out = initial.clone();
                codec.update(&old_refs, &forward, &mut out).unwrap();
                assert_eq!(out, expected);
                codec.update(&new_refs, &reverse, &mut out).unwrap();
                assert_eq!(out, initial);
                let label = format!("{name}/{data}x{parity}/{len}/{changed}");
                group.bench_function(BenchmarkId::new("roundtrip", label), |b| {
                    b.iter(|| {
                        codec
                            .update(
                                black_box(&old_refs),
                                black_box(&forward),
                                black_box(&mut out),
                            )
                            .unwrap();
                        codec
                            .update(
                                black_box(&new_refs),
                                black_box(&reverse),
                                black_box(&mut out),
                            )
                            .unwrap();
                    })
                });
            }
        }
    }
    group.finish();
}
fn run(c: &mut Criterion) {
    if let Ok(expected) = std::env::var("RSE_EXPECT_BACKEND") {
        assert_eq!(galois_8::active_backend_name(), expected);
    } else if std::env::var_os("RSE_GFNI_BENCH_PRESELECT").is_some() {
        assert_eq!(galois_8::active_backend_name(), "rust-gfni-avx2");
    }
    field::<galois_8::Field>(c, "gf8");
    field::<galois_16::Field>(c, "tower16");
}
criterion_group!(benches, run);
criterion_main!(benches);
