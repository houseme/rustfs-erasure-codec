//! Standalone parity snapshot probe; see docs/erasure-codec-completion-audit-2026-10-17.md.

use rustfs_erasure_codec::{CodecFamily, CodecOptions, Field, MatrixMode, ReedSolomon};
fn probe<F: Field>(field: &str, family: CodecFamily) {
    for (k, m) in [(4, 2), (10, 4), (4, 1), (8, 3), (32, 16)] {
        for (mi, mode) in [
            MatrixMode::Vandermonde,
            MatrixMode::Cauchy,
            MatrixMode::JerasureLike,
            MatrixMode::Custom,
        ]
        .into_iter()
        .enumerate()
        {
            for fast in [false, true] {
                let options = CodecOptions::builder()
                    .codec_family(family)
                    .matrix_mode(mode)
                    .fast_one_parity(fast)
                    .build();
                let codec = if family == CodecFamily::Classic && mode == MatrixMode::Custom {
                    let reference = ReedSolomon::<F>::with_options(
                        k,
                        m,
                        CodecOptions::builder().fast_one_parity(fast).build(),
                    )
                    .unwrap();
                    let basis: Vec<Vec<_>> = (0..k)
                        .map(|i| {
                            (0..k)
                                .map(|j| if i == j { F::one() } else { F::zero() })
                                .collect()
                        })
                        .collect();
                    let mut rows = vec![vec![F::zero(); k]; m];
                    reference.encode_sep(&basis, &mut rows).unwrap();
                    ReedSolomon::<F>::with_custom_matrix(k, m, &rows, options).unwrap()
                } else {
                    ReedSolomon::<F>::with_options(k, m, options).unwrap()
                };
                let lengths: &[usize] = if family == CodecFamily::Classic {
                    &[1, 65, 192]
                } else {
                    &[64, 192]
                };
                for &len in lengths {
                    let mut shards: Vec<Vec<_>> = (0..k + m)
                        .map(|row| {
                            (0..len)
                                .map(|offset| F::nth((row * 257 + offset * 109 + 1) % F::ORDER))
                                .collect()
                        })
                        .collect();
                    codec.encode(&mut shards).unwrap();
                    assert!(codec.verify(&shards).unwrap());
                    println!(
                        "{{\"id\":\"{field}/{family:?}/{k}+{m}/matrix{mi}/fast{fast}/{len}\",\"parity\":{:?}}}",
                        &shards[k..]
                    );
                }
            }
        }
    }
}
fn wide_probe() {
    use std::io::Write;
    let mut output = std::io::stdout().lock();
    for (family, k, m, len) in [
        (CodecFamily::LeopardGF8, 129, 64, 128 * 1024 + 64),
        (CodecFamily::LeopardGF8, 128, 128, 128 * 1024 + 64),
        (CodecFamily::LeopardGF16, 4, 511, 64 * 1024 + 64),
        (CodecFamily::LeopardGF16, 4, 512, 64 * 1024 + 64),
        (CodecFamily::LeopardGF16, 4, 513, 64 * 1024 + 64),
    ] {
        let codec = rustfs_erasure_codec::galois_8::ReedSolomon::with_options(
            k,
            m,
            CodecOptions::builder().codec_family(family).build(),
        )
        .unwrap();
        let mut shards: Vec<Vec<u8>> = (0..k + m)
            .map(|row| {
                (0..len)
                    .map(|offset| (row * 53 + offset * 109 + 1) as u8)
                    .collect()
            })
            .collect();
        codec.encode(&mut shards).unwrap();
        assert!(codec.verify(&shards).unwrap());
        writeln!(output, "{family:?}/{k}+{m}/{len}").unwrap();
        for parity in &shards[k..] {
            output.write_all(parity).unwrap();
        }
    }
}

fn main() {
    // Wide snapshots contain deterministic headers followed by raw parity bytes.
    // Keep them out of the small JSONL fixtures to avoid huge decimal arrays.
    if std::env::args().any(|arg| arg == "--wide") {
        wide_probe();
        return;
    }

    for family in [
        CodecFamily::Classic,
        CodecFamily::LeopardGF8,
        CodecFamily::LeopardGF16,
    ] {
        probe::<rustfs_erasure_codec::galois_8::Field>("gf8", family);
    }
    probe::<rustfs_erasure_codec::galois_16::Field>("tower-gf16", CodecFamily::Classic);
}
