fn main() {
    let mut checks = 0;
    for (data, parity) in [(4usize, 3usize), (10, 4), (16, 8)] {
        let old = codec_old::galois_8::ReedSolomon::with_options(
            data,
            parity,
            codec_old::CodecOptions::builder()
                .codec_family(codec_old::CodecFamily::LeopardGF16)
                .build(),
        )
        .unwrap();
        let new = codec_new::galois_8::ReedSolomon::with_options(
            data,
            parity,
            codec_new::CodecOptions::builder()
                .codec_family(codec_new::CodecFamily::LeopardGF16)
                .build(),
        )
        .unwrap();
        for size in [
            64usize, 128, 192, 256, 320, 384, 448, 512, 576, 4096, 65536, 65600,
        ] {
            let initial: Vec<Vec<u8>> = (0..data + parity)
                .map(|i| {
                    (0..size)
                        .map(|j| (i * 31 + j * 17 + j / 11) as u8)
                        .collect()
                })
                .collect();
            let mut old_shards = initial.clone();
            let mut new_shards = initial;
            old.encode(&mut old_shards).unwrap();
            new.encode(&mut new_shards).unwrap();
            assert_eq!(old_shards, new_shards, "parity {data}+{parity} {size}");
            checks += 1;
            let mut masks = vec![vec![0], vec![data - 1, data]];
            if parity >= 4 {
                masks.push(vec![0, data / 2, data, data + 1]);
            }
            for mask in masks {
                let mut old_out: Vec<_> = new_shards.iter().cloned().map(Some).collect();
                let mut new_out: Vec<_> = old_shards.iter().cloned().map(Some).collect();
                for &index in &mask {
                    old_out[index] = None;
                    new_out[index] = None;
                }
                old.reconstruct(&mut old_out).unwrap();
                new.reconstruct(&mut new_out).unwrap();
                assert!(
                    old_out
                        .iter()
                        .zip(&old_shards)
                        .all(|(a, b)| a.as_ref() == Some(b))
                );
                assert!(
                    new_out
                        .iter()
                        .zip(&new_shards)
                        .all(|(a, b)| a.as_ref() == Some(b))
                );
                checks += 2;
            }
        }
    }
    assert_eq!(checks, 228);
    println!(
        "Native no_std GF16 threshold and cross-version checks: {checks}; x86_avx2_target={}",
        cfg!(all(target_arch = "x86_64", target_feature = "avx2"))
    );
}
