#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|input: &[u8]| {
    if input.len() < 4 {
        return;
    }
    let data = 1 + usize::from(input[0] % 32);
    let parity = 1 + usize::from(input[1] % 16);
    let size = [64usize, 128, 256][usize::from(input[2] % 3)];
    let missing = 1 + usize::from(input[3]) % parity;
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
    let mut shards: Vec<Vec<u8>> = (0..data + parity)
        .map(|i| {
            (0..size)
                .map(|j| {
                    input[(i * 13 + j * 7) % input.len()].wrapping_add((i * 31 + j * 17) as u8)
                })
                .collect()
        })
        .collect();
    let mut old_shards = shards.clone();
    old.encode(&mut old_shards).unwrap();
    new.encode(&mut shards).unwrap();
    assert_eq!(shards, old_shards);
    let mut positions: Vec<usize> = (0..data + parity).collect();
    for i in 0..missing {
        let j = i + usize::from(input[(i + 4) % input.len()]) % (positions.len() - i);
        positions.swap(i, j);
    }
    let mut old_out: Vec<_> = shards.iter().cloned().map(Some).collect();
    let mut new_out = old_out.clone();
    for &i in &positions[..missing] {
        old_out[i] = None;
        new_out[i] = None;
    }
    old.reconstruct(&mut old_out).unwrap();
    new.reconstruct(&mut new_out).unwrap();
    assert!(
        old_out
            .iter()
            .zip(&shards)
            .all(|(a, b)| a.as_ref() == Some(b))
    );
    assert!(
        new_out
            .iter()
            .zip(&shards)
            .all(|(a, b)| a.as_ref() == Some(b))
    );
});
