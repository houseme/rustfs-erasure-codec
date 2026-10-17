use rustfs_erasure_codec::{CodecFamily, CodecOptions, Field, MatrixMode, ReedSolomon};

fn probe<F: Field>(snapshots: &mut Vec<super::Snapshot>, field: &str, family: CodecFamily)
where
    F::Elem: super::ElementBytes,
{
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
            let options = CodecOptions::builder()
                .codec_family(family)
                .matrix_mode(mode)
                .fast_one_parity(false)
                .build();
            let codec = if family == CodecFamily::Classic && mode == MatrixMode::Custom {
                let reference = ReedSolomon::<F>::new(k, m).unwrap();
                let basis: Vec<Vec<_>> = (0..k)
                    .map(|i| {
                        (0..k)
                            .map(|j| if i == j { F::one() } else { F::zero() })
                            .collect()
                    })
                    .collect();
                let mut rows = vec![vec![F::zero(); k]; m];
                reference.encode_sep(&basis, &mut rows).unwrap();
                ReedSolomon::<F>::with_custom_matrix(k, m, &rows, options)
            } else {
                ReedSolomon::<F>::with_options(k, m, options)
            }
            .unwrap();
            let lengths: &[usize] = if family == CodecFamily::Classic {
                &[1, 65, 192]
            } else {
                &[64, 192]
            };
            for &len in lengths {
                let id = format!("{field}/{family:?}/{k}+{m}/matrix{mi}/{len}");
                let mut shards: Vec<Vec<_>> = (0..k + m)
                    .map(|row| {
                        (0..len)
                            .map(|offset| F::nth((row * 257 + offset * 109 + 1) % F::ORDER))
                            .collect()
                    })
                    .collect();
                codec.encode(&mut shards).unwrap();
                assert!(codec.verify(&shards).unwrap(), "verification: {id}");
                let mut lost: Vec<_> = shards.iter().cloned().map(Some).collect();
                lost[0] = None;
                codec.reconstruct(&mut lost).unwrap();
                assert!(
                    lost.iter()
                        .zip(&shards)
                        .all(|(actual, expected)| actual.as_ref() == Some(expected)),
                    "roundtrip: {id}"
                );
                snapshots.push(super::Snapshot {
                    id,
                    parity: shards[k..]
                        .iter()
                        .map(|shard| {
                            let mut bytes =
                                Vec::with_capacity(shard.len() * core::mem::size_of::<F::Elem>());
                            for &element in shard {
                                super::ElementBytes::append_bytes(element, &mut bytes);
                            }
                            bytes
                        })
                        .collect(),
                });
            }
        }
    }
}

pub(super) fn run() -> Vec<super::Snapshot> {
    let mut snapshots = Vec::new();
    for family in [
        CodecFamily::Classic,
        CodecFamily::LeopardGF8,
        CodecFamily::LeopardGF16,
    ] {
        probe::<rustfs_erasure_codec::galois_8::Field>(&mut snapshots, "gf8", family);
    }
    probe::<rustfs_erasure_codec::galois_16::Field>(
        &mut snapshots,
        "tower-gf16",
        CodecFamily::Classic,
    );
    snapshots
}
