use codec_new::{CodecFamily, CodecOptions, galois_8::ReedSolomon as NewCodec};
use codec_old::{
    CodecFamily as OldFamily, CodecOptions as OldOptions, galois_8::ReedSolomon as OldCodec,
};

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn validate() -> usize {
    let mut cases = 0usize;
    for (data, parity) in [
        (4usize, 3usize),
        (10, 4),
        (16, 8),
        (20, 3),
        (63, 5),
        (150, 3),
        (224, 17),
        (225, 17),
        (224, 32),
        (248, 8),
        (249, 8),
    ] {
        let old = OldCodec::with_options(
            data,
            parity,
            OldOptions::builder()
                .codec_family(OldFamily::LeopardGF16)
                .build(),
        )
        .unwrap();
        let new = NewCodec::with_options(
            data,
            parity,
            CodecOptions::builder()
                .codec_family(CodecFamily::LeopardGF16)
                .build(),
        )
        .unwrap();
        for size in [64usize, 128, 4096] {
            let initial: Vec<Vec<u8>> = (0..data + parity)
                .map(|i| {
                    (0..size)
                        .map(|j| (i.wrapping_mul(31) + j.wrapping_mul(17) + j / 11) as u8)
                        .collect()
                })
                .collect();
            let mut old_shards = initial.clone();
            let mut new_shards = initial;
            old.encode(&mut old_shards).unwrap();
            new.encode(&mut new_shards).unwrap();
            assert_eq!(old_shards, new_shards, "parity {data}+{parity}/{size}");
            let mut masks = vec![
                vec![0],
                vec![data - 1],
                vec![data],
                vec![data + parity - 1],
                vec![0, data],
                vec![data, data + 1],
            ];
            if parity >= 4 {
                masks.push(vec![0, data / 2, data, data + 1]);
                masks.push(vec![data + 2, data + 3]);
            }
            if data == 10 && parity == 4 && size == 64 {
                masks.clear();
                for a in 0..data + parity {
                    masks.push(vec![a]);
                    for b in a + 1..data + parity {
                        masks.push(vec![a, b]);
                        for c in b + 1..data + parity {
                            masks.push(vec![a, b, c]);
                            for d in c + 1..data + parity {
                                masks.push(vec![a, b, c, d]);
                            }
                        }
                    }
                }
            }
            for mask in masks {
                let mut from_new: Vec<_> = new_shards.iter().cloned().map(Some).collect();
                let mut from_old: Vec<_> = old_shards.iter().cloned().map(Some).collect();
                for &i in &mask {
                    from_new[i] = None;
                    from_old[i] = None;
                }
                old.reconstruct(&mut from_new).unwrap();
                new.reconstruct(&mut from_old).unwrap();
                assert!(
                    from_new
                        .iter()
                        .zip(&old_shards)
                        .all(|(a, b)| a.as_ref() == Some(b)),
                    "old decode {data}+{parity}/{size} mask={mask:?}"
                );
                assert!(
                    from_old
                        .iter()
                        .zip(&new_shards)
                        .all(|(a, b)| a.as_ref() == Some(b)),
                    "new decode {data}+{parity}/{size} mask={mask:?}"
                );
                cases += 1;
            }
        }
    }
    validate_generic_targets();
    cases
}

fn validate_generic_targets() {
    use codec_new::{Error, ShardSlot};
    let codec = NewCodec::with_options(
        10,
        4,
        CodecOptions::builder()
            .codec_family(CodecFamily::LeopardGF16)
            .build(),
    )
    .unwrap();
    let mut source: Vec<Vec<u8>> = (0..14)
        .map(|i| (0..64).map(|j| (i * 31 + j * 17 + j / 11) as u8).collect())
        .collect();
    codec.encode(&mut source).unwrap();
    let erased = [0usize, 3, 10, 11];
    let mut required = [false; 14];
    required[0] = true;
    required[10] = true;

    let mut slots: Vec<ShardSlot<Vec<u8>>> =
        source.iter().cloned().map(ShardSlot::new_present).collect();
    for &i in &erased {
        slots[i].as_inner_mut().fill(0xa5);
        slots[i].mark_missing();
    }
    codec.reconstruct_some(&mut slots, &required).unwrap();
    for &i in &[0usize, 10] {
        assert!(slots[i].is_present());
        assert_eq!(slots[i].as_inner(), &source[i]);
    }
    for &i in &[3usize, 11] {
        assert!(!slots[i].is_present());
        assert!(slots[i].as_inner().iter().all(|&b| b == 0xa5));
    }

    let mut tuples: Vec<(Vec<u8>, bool)> = source.iter().cloned().map(|x| (x, true)).collect();
    for &i in &erased {
        tuples[i].0.fill(0xa5);
        tuples[i].1 = false;
    }
    codec.reconstruct_data(&mut tuples).unwrap();
    for &i in &[0usize, 3] {
        assert!(tuples[i].1);
        assert_eq!(tuples[i].0, source[i]);
    }
    for &i in &[10usize, 11] {
        assert!(!tuples[i].1);
        assert!(tuples[i].0.iter().all(|&b| b == 0xa5));
    }

    let mut rejected: Vec<(Vec<u8>, bool)> = source.iter().cloned().map(|x| (x, true)).collect();
    rejected[0] = (vec![0x5a; 63], false);
    rejected[10] = (vec![0x5a; 64], false);
    let before = rejected.clone();
    assert_eq!(
        codec.reconstruct_some(&mut rejected, &required),
        Err(Error::IncorrectShardSize)
    );
    assert_eq!(rejected, before);
}
