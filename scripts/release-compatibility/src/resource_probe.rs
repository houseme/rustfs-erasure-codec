use rustfs_erasure_codec::{CodecFamily, CodecOptions, Field, ReedSolomon};

fn options(family: CodecFamily, cache: bool) -> CodecOptions {
    CodecOptions::builder()
        .codec_family(family)
        .fast_one_parity(false)
        .inversion_cache(cache)
        .build()
}
fn data<F: Field>(k: usize, m: usize, len: usize) -> Vec<Vec<F::Elem>> {
    (0..k + m)
        .map(|row| {
            (0..len)
                .map(|offset| F::nth((row * 29 + offset * 17 + 13) % F::ORDER))
                .collect()
        })
        .collect()
}

fn kernels<F: Field>(
    out: &mut Vec<super::super::ResourceCase>,
    name: &str,
    family: CodecFamily,
    k: usize,
    m: usize,
    len: usize,
    cache: bool,
) where
    F::Elem: super::super::ElementBytes,
{
    let codec = ReedSolomon::<F>::with_options(k, m, options(family, cache)).unwrap();
    let mut source = data::<F>(k, m, len);
    // Both operations warm twice. The cold-inverse scenario explicitly disables
    // the inverse cache, so it remains uncached despite rehearing the operation.
    for _ in 0..2 {
        codec.encode(&mut source).unwrap();
    }
    let expected = source.clone();
    let parity_bytes: Vec<Vec<u8>> = expected[k..]
        .iter()
        .map(|shard| {
            let mut bytes = Vec::with_capacity(shard.len() * core::mem::size_of::<F::Elem>());
            for &element in shard {
                super::super::ElementBytes::append_bytes(element, &mut bytes);
            }
            bytes
        })
        .collect();
    let mut counts = Vec::with_capacity(3);
    for _ in 0..3 {
        let (_, measured) =
            super::super::allocation::measure(|| codec.encode(&mut source).unwrap());
        assert!(source == expected);
        counts.push(measured);
    }
    out.push(super::super::ResourceCase {
        id: format!("{name}/{family:?}/{k}+{m}/{len}/encode"),
        counts,
        parity: parity_bytes.clone(),
    });
    let mut counts = Vec::with_capacity(3);
    for index in 0..5 {
        let mut missing: Vec<_> = source.iter().cloned().map(Some).collect();
        missing[0] = None;
        let measured = if index < 2 {
            codec.reconstruct_data(&mut missing).unwrap();
            None
        } else {
            let (_, count) =
                super::super::allocation::measure(|| codec.reconstruct_data(&mut missing).unwrap());
            Some(count)
        };
        assert!(
            missing
                .iter()
                .zip(&expected)
                .all(|(a, b)| a.as_ref() == Some(b))
        );
        if let Some(count) = measured {
            counts.push(count);
        }
    }
    out.push(super::super::ResourceCase {
        id: format!("{name}/{family:?}/{k}+{m}/{len}/reconstruct_data/cache={cache}"),
        counts,
        parity: parity_bytes,
    });
}

fn streams(out: &mut Vec<super::super::ResourceCase>) {
    use rustfs_erasure_codec::stream::{StreamIoMode, StreamOptions};
    use std::io::Cursor;
    let codec = rustfs_erasure_codec::galois_8::ReedSolomon::new(4, 2).unwrap();
    let mut source = data::<rustfs_erasure_codec::galois_8::Field>(4, 2, 65536);
    codec.encode(&mut source).unwrap();
    let opts = StreamOptions::new()
        .with_block_size(4096)
        .with_io_mode(StreamIoMode::Serial);
    for operation in ["encode_stream", "verify_stream", "reconstruct_stream"] {
        let mut counts = Vec::with_capacity(3);
        for index in 0..5 {
            let mut readers: Vec<_> = source.iter().map(Vec::as_slice).collect();
            let mut writers: Vec<Vec<u8>> = (0..2).map(|_| Vec::with_capacity(65536)).collect();
            let mut cursors: Vec<_> = source.iter().cloned().map(Cursor::new).collect();
            cursors[0] = Cursor::new(Vec::new());
            cursors[4] = Cursor::new(Vec::new());
            let mut work = || match operation {
                "encode_stream" => codec
                    .encode_stream(&mut readers[..4], &mut writers, &opts)
                    .unwrap(),
                "verify_stream" => assert!(codec.verify_stream(&mut readers, &opts).unwrap()),
                "reconstruct_stream" => codec.reconstruct_stream(&mut cursors, &opts).unwrap(),
                _ => unreachable!(),
            };
            if index < 2 {
                work();
            } else {
                counts.push(super::super::allocation::measure(work).1);
            }
            match operation {
                "encode_stream" => assert!(writers == source[4..]),
                "reconstruct_stream" => {
                    assert!(cursors.iter().zip(&source).all(|(a, b)| a.get_ref() == b))
                }
                _ => {}
            }
        }
        out.push(super::super::ResourceCase {
            id: format!("gf8/Classic/4+2/65536/{operation}/block=4096/io=serial"),
            counts,
            parity: source[4..].to_vec(),
        });
    }
}

pub(super) fn resources() -> Vec<super::super::ResourceCase> {
    let mut out = Vec::new();
    kernels::<rustfs_erasure_codec::galois_8::Field>(
        &mut out,
        "gf8",
        CodecFamily::Classic,
        4,
        2,
        65536,
        true,
    );
    kernels::<rustfs_erasure_codec::galois_8::Field>(
        &mut out,
        "gf8",
        CodecFamily::Classic,
        10,
        4,
        524288,
        true,
    );
    kernels::<rustfs_erasure_codec::galois_8::Field>(
        &mut out,
        "gf8-cold-inverse",
        CodecFamily::Classic,
        10,
        4,
        16384,
        false,
    );
    kernels::<rustfs_erasure_codec::galois_16::Field>(
        &mut out,
        "tower-gf16-cold-inverse",
        CodecFamily::Classic,
        10,
        4,
        65,
        false,
    );
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        kernels::<rustfs_erasure_codec::galois_8::Field>(
            &mut out, "gf8", family, 10, 4, 32768, true,
        );
    }
    streams(&mut out);
    out
}
