use rustfs_erasure_codec::{CodecFamily, CodecOptions, Field, MatrixMode, ReedSolomon};

/// Retained from the former Python-2-style Sage worksheet. In GF(256), `a=2`
/// uses x^8+x^4+x^3+x^2+1; the extension is X^2+a*X+a^7.
/// Element bytes are [X coefficient, constant coefficient].
pub(super) fn field_reference_vectors() {
    type Tower = rustfs_erasure_codec::galois_16::Field;
    // (a^7+a^6+a^4+a)*X + a^3+a^2+a+1
    let e1 = [0xd2, 0x0f];
    // (a^7+a^5+a^2)*X + a^7+a^4+a^3+a
    let e2 = [0xa4, 0x9a];
    assert_eq!(Tower::add(e1, e2), [0x76, 0x95], "Sage addition vector");
    assert_eq!(Tower::mul(e1, e2), [0x17, 0xaa], "Sage multiplication vector");
    assert_eq!(Tower::div(e1, e2), [0xfd, 0x4a], "Sage division vector");
    assert_eq!(Tower::div(Tower::one(), [1, 0]), [0x1b, 0x36], "Sage inverse-X vector");
}

fn probe<F: Field>(snapshots: &mut Vec<super::Snapshot>, field: &str, family: CodecFamily, peer: Option<&[super::Snapshot]>)
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
                if let Some(peer) = peer {
                    let reference = &peer[snapshots.len()];
                    assert_eq!(reference.id, id, "cross-version case ordering");
                    assert_eq!(reference.parity.len(), m);
                    for (destination, bytes) in shards[k..].iter_mut().zip(&reference.parity) {
                        *destination = <F::Elem as super::ElementBytes>::from_bytes(bytes);
                    }
                    assert!(codec.verify(&shards).unwrap(), "cross-version verification: {id}");
                }
                let mut lost: Vec<_> = shards.iter().cloned().map(Some).collect();
                lost[0] = None;
                if peer.is_some() && m > 1 { lost[k + m - 1] = None; }
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

fn run_with_peer(peer: Option<&[super::Snapshot]>) -> Vec<super::Snapshot> {
    let mut snapshots = Vec::new();
    for family in [
        CodecFamily::Classic,
        CodecFamily::LeopardGF8,
        CodecFamily::LeopardGF16,
    ] {
        probe::<rustfs_erasure_codec::galois_8::Field>(&mut snapshots, "gf8", family, peer);
    }
    probe::<rustfs_erasure_codec::galois_16::Field>(
        &mut snapshots,
        "tower-gf16",
        CodecFamily::Classic,
        peer,
    );
    snapshots
}

pub(super) fn run() -> Vec<super::Snapshot> { run_with_peer(None) }

/// Compare the byte format around the generated AVX-512 activation boundary.
pub(super) fn large_gf8(peer: Option<&[super::Snapshot]>) -> Vec<super::Snapshot> {
    gf8_boundaries(peer, &[(4, 2), (10, 4), (12, 4)], &[32_767, 32_768, 32_769])
}

/// Preserve 6+2 compatibility around the candidate AVX2 dispatch boundary.
pub(super) fn six_plus_two(peer: Option<&[super::Snapshot]>) -> Vec<super::Snapshot> {
    gf8_boundaries(peer, &[(6, 2)], &[4_095, 4_096, 4_097])
}

fn gf8_boundaries(
    peer: Option<&[super::Snapshot]>,
    shapes: &[(usize, usize)],
    lengths: &[usize],
) -> Vec<super::Snapshot> {
    use rustfs_erasure_codec::galois_8::ReedSolomon;

    let mut snapshots = Vec::new();
    if let Some(peer) = peer {
        assert_eq!(peer.len(), shapes.len() * 4 * lengths.len());
    }
    for &(data_count, parity_count) in shapes {
        for (mode_index, mode) in [
            MatrixMode::Vandermonde,
            MatrixMode::Cauchy,
            MatrixMode::JerasureLike,
            MatrixMode::Custom,
        ]
        .into_iter()
        .enumerate()
        {
            let options = CodecOptions::builder()
                .matrix_mode(mode)
                .fast_one_parity(false)
                .max_parallel_jobs(1)
                .build();
            let codec = if mode == MatrixMode::Custom {
                let reference = ReedSolomon::new(data_count, parity_count).unwrap();
                let basis: Vec<Vec<u8>> = (0..data_count)
                    .map(|row| {
                        (0..data_count)
                            .map(|column| u8::from(row == column))
                            .collect()
                    })
                    .collect();
                let mut rows = vec![vec![0; data_count]; parity_count];
                reference.encode_sep(&basis, &mut rows).unwrap();
                rows.reverse();
                ReedSolomon::with_custom_matrix(data_count, parity_count, &rows, options).unwrap()
            } else {
                ReedSolomon::with_options(data_count, parity_count, options).unwrap()
            };
            for &len in lengths {
                let id =
                    format!("gf8/Classic/{data_count}+{parity_count}/matrix{mode_index}/{len}");
                let mut shards: Vec<Vec<u8>> = (0..data_count + parity_count)
                    .map(|row| {
                        (0..len)
                            .map(|offset| (row * 73 + offset * 19 + offset / 17) as u8)
                            .collect()
                    })
                    .collect();
                codec.encode(&mut shards).unwrap();
                if let Some(peer) = peer {
                    let reference = &peer[snapshots.len()];
                    assert_eq!(reference.id, id, "cross-version case ordering");
                    for (destination, bytes) in
                        shards[data_count..].iter_mut().zip(&reference.parity)
                    {
                        destination.clone_from(bytes);
                    }
                }
                assert!(codec.verify(&shards).unwrap(), "verification: {id}");
                let mut lost: Vec<_> = shards.iter().cloned().map(Some).collect();
                lost[data_count / 2] = None;
                lost[data_count + parity_count - 1] = None;
                codec.reconstruct(&mut lost).unwrap();
                assert!(
                    lost.iter()
                        .zip(&shards)
                        .all(|(actual, expected)| actual.as_ref() == Some(expected)),
                    "roundtrip: {id}"
                );
                snapshots.push(super::Snapshot {
                    id,
                    parity: shards[data_count..].to_vec(),
                });
            }
        }
    }
    snapshots
}

pub(super) fn cross_decode(peer: &[super::Snapshot]) {
    assert_eq!(peer.len(), 200);
    let decoded = run_with_peer(Some(peer));
    assert_eq!(decoded, peer, "cross-version decoded parity");
}
