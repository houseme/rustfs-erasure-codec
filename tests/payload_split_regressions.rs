use rustfs_erasure_codec::galois_8::ReedSolomon;
use rustfs_erasure_codec::{CodecFamily, CodecOptions, LeopardMode, MatrixMode};

fn payload(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 37 + i / 251) as u8).collect()
}

fn check_roundtrip(codec: &ReedSolomon, len: usize) {
    let data = payload(len);
    let parts = codec.split(&data).unwrap();
    assert_eq!(parts.len(), codec.data_shard_count());
    let shard_len = parts[0].len();
    let mut shards = parts.clone();
    shards.extend((0..codec.parity_shard_count()).map(|_| vec![0; shard_len]));
    codec.encode(&mut shards).unwrap();
    assert!(codec.verify(&shards).unwrap());
    assert!(
        parts
            .iter()
            .all(|s| s.len() == shard_len && s.len().is_multiple_of(64))
    );
    assert_eq!(codec.join(&parts, data.len()).unwrap(), data);

    let mut missing: Vec<_> = shards.iter().cloned().map(Some).collect();
    missing[0] = None;
    if codec.parity_shard_count() > 1 {
        missing[codec.data_shard_count()] = None;
    }
    codec.reconstruct(&mut missing).unwrap();
    assert_eq!(
        missing,
        shards.iter().cloned().map(Some).collect::<Vec<_>>()
    );
    let recovered: Vec<_> = missing[..codec.data_shard_count()]
        .iter()
        .map(|s| s.as_ref().unwrap())
        .collect();
    assert_eq!(codec.join(&recovered, data.len()).unwrap(), data);

    if codec.parity_shard_count() > 1 {
        let mut selected: Vec<_> = shards.iter().cloned().map(Some).collect();
        selected[0] = None;
        selected[codec.data_shard_count()] = None;
        let mut required = vec![false; codec.total_shard_count()];
        required[codec.data_shard_count()] = true;
        codec.reconstruct_some(&mut selected, &required).unwrap();
        assert!(selected[0].is_none());
        assert_eq!(
            selected[codec.data_shard_count()].as_ref(),
            Some(&shards[codec.data_shard_count()])
        );
    }

    #[cfg(feature = "std")]
    {
        let mut optimized = parts;
        optimized.extend((0..codec.parity_shard_count()).map(|_| vec![0; shard_len]));
        codec.encode_opt(&mut optimized).unwrap();
        assert_eq!(optimized, shards);
        let mut missing: Vec<_> = optimized.into_iter().map(Some).collect();
        missing[0] = None;
        codec.reconstruct_opt(&mut missing).unwrap();
        assert_eq!(missing, shards.into_iter().map(Some).collect::<Vec<_>>());
    }
}

#[test]
fn explicit_leopard_split_encode_recover_join_handles_arbitrary_payloads() {
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        if family == CodecFamily::LeopardGF16 && cfg!(target_endian = "big") {
            continue;
        }
        for (data, parity) in [(1, 1), (4, 2), (10, 4)] {
            let codec = ReedSolomon::with_options(
                data,
                parity,
                CodecOptions::builder().codec_family(family).build(),
            )
            .unwrap();
            for len in [5, 0, 1, 63, 64, 65, 255, 256, 257, 640, 641, 4109] {
                check_roundtrip(&codec, len);
            }
            // Both native Leopard engines use 32 KiB processing blocks.
            check_roundtrip(&codec, data * (32 * 1024) + 1);
        }
    }
}

#[test]
fn split_uses_resolved_family_after_automatic_selection() {
    for mode in [LeopardMode::PreferLeopard, LeopardMode::PreferGF16] {
        if mode == LeopardMode::PreferGF16 && cfg!(target_endian = "big") {
            continue;
        }
        let codec =
            ReedSolomon::with_options(4, 2, CodecOptions::builder().leopard_mode(mode).build())
                .unwrap();
        assert_ne!(codec.codec_family(), CodecFamily::Classic);
        for len in [5, 0, 257] {
            check_roundtrip(&codec, len);
        }
    }
    if cfg!(target_endian = "little") {
        let codec = ReedSolomon::with_options(
            256,
            1,
            CodecOptions::builder()
                .leopard_mode(LeopardMode::AsNeeded)
                .build(),
        )
        .unwrap();
        assert_eq!(codec.codec_family(), CodecFamily::LeopardGF16);
        check_roundtrip(&codec, 257);
    }
}

#[test]
fn classic_split_keeps_existing_padding_including_empty_and_custom_matrices() {
    let mut codecs = Vec::new();
    for matrix in [
        MatrixMode::Vandermonde,
        MatrixMode::Cauchy,
        MatrixMode::JerasureLike,
    ] {
        codecs.push(
            ReedSolomon::with_options(4, 2, CodecOptions::builder().matrix_mode(matrix).build())
                .unwrap(),
        );
    }
    codecs.push(
        ReedSolomon::with_custom_matrix(
            4,
            2,
            &[vec![1; 4], vec![1, 2, 4, 8]],
            CodecOptions::builder()
                .leopard_mode(LeopardMode::PreferGF16)
                .build(),
        )
        .unwrap(),
    );
    for codec in codecs {
        assert_eq!(codec.codec_family(), CodecFamily::Classic);
        for len in [0, 1, 5, 64, 65, 257] {
            let data = payload(len);
            let parts = codec.split(&data).unwrap();
            let shard_len = len.div_ceil(4);
            let expected: Vec<_> = (0..4)
                .map(|i| {
                    let mut part = vec![0; shard_len];
                    let start = i * shard_len;
                    if start < len {
                        let end = (start + shard_len).min(len);
                        part[..end - start].copy_from_slice(&data[start..end]);
                    }
                    part
                })
                .collect();
            assert_eq!(parts, expected);
        }
    }
}

#[test]
fn classic_tower_gf16_split_and_join_use_element_counts() {
    let codec = rustfs_erasure_codec::galois_16::ReedSolomon::new(3, 2).unwrap();
    let data = [[1, 2], [3, 4], [5, 6], [7, 8], [9, 10]];
    let parts = codec.split(&data).unwrap();
    assert_eq!(
        parts,
        vec![
            vec![[1, 2], [3, 4]],
            vec![[5, 6], [7, 8]],
            vec![[9, 10], [0, 0]]
        ]
    );
    assert_eq!(codec.join(&parts, data.len()).unwrap(), data);
}

#[test]
fn already_supported_leopard_payloads_preserve_the_legacy_split_layout() {
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        if family == CodecFamily::LeopardGF16 && cfg!(target_endian = "big") {
            continue;
        }
        for (data_count, parity_count) in [(4, 2), (10, 4)] {
            let codec = ReedSolomon::with_options(
                data_count,
                parity_count,
                CodecOptions::builder().codec_family(family).build(),
            )
            .unwrap();
            for shard_len in [64, 128] {
                for len in [
                    data_count * shard_len - data_count + 1,
                    data_count * shard_len,
                ] {
                    let data = payload(len);
                    assert_eq!(len.div_ceil(data_count), shard_len);
                    let legacy: Vec<_> = data
                        .chunks(shard_len)
                        .map(|chunk| {
                            let mut bytes = vec![0; shard_len];
                            bytes[..chunk.len()].copy_from_slice(chunk);
                            bytes
                        })
                        .collect();
                    assert_eq!(legacy.len(), data_count);
                    assert_eq!(codec.split(&data).unwrap(), legacy);
                }
            }
        }
    }
}
