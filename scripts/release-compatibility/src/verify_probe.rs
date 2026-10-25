use crate::ElementBytes;
use crate::verify_report::{Api, Config, Damage, FieldKind, Fixture, Observation, Sample};
use rustfs_erasure_codec::{
    CodecFamily, CodecOptions, Field, MatrixMode, ReedSolomon, VerifyWorkspace,
};

pub(super) fn fixture(field: FieldKind, config: Config) -> Fixture {
    match field {
        FieldKind::Gf8 => run::<rustfs_erasure_codec::galois_8::Field>(field, config),
        FieldKind::TowerGf16 => run::<rustfs_erasure_codec::galois_16::Field>(field, config),
    }
}

fn run<F: Field>(field: FieldKind, config: Config) -> Fixture
where
    F::Elem: ElementBytes,
{
    let Config {
        data,
        parity,
        elements,
    } = config;
    let codec = ReedSolomon::<F>::with_options(
        data,
        parity,
        CodecOptions::builder()
            .codec_family(CodecFamily::Classic)
            .matrix_mode(MatrixMode::Vandermonde)
            .fast_one_parity(false)
            .max_parallel_jobs(1)
            .build(),
    )
    .unwrap();
    let mut shards: Vec<Vec<F::Elem>> = (0..data + parity)
        .map(|row| {
            (0..elements)
                .map(|offset| F::nth((row * 37 + offset * 19 + offset / 67 + 1) % F::ORDER))
                .collect()
        })
        .collect();
    codec.encode(&mut shards).unwrap();
    // Keep each API's lifecycle explicit. Caller buffers and workspace are
    // allocated once outside every window; ordinary verify owns its scratch.
    let mut buffer = vec![vec![F::zero(); elements]; parity];
    let mut workspace = VerifyWorkspace::new(&codec, elements);
    let mut observations = Vec::with_capacity(9);
    for damage in Damage::ALL {
        let corruption = damage.position(config);
        if let Some((row, offset)) = corruption {
            shards[row][offset] = F::add(shards[row][offset], F::one());
        }
        let inputs: Vec<_> = shards.iter().map(Vec::as_slice).collect();
        for api in Api::ALL {
            let mut samples = Vec::with_capacity(3);
            let mut verify = || match api {
                Api::Ordinary => codec.verify(&inputs),
                Api::Buffer => codec.verify_with_buffer(&inputs, &mut buffer),
                Api::Workspace => codec.verify_with_workspace(&inputs, &mut workspace),
            };
            for _ in 0..2 {
                assert_eq!(verify().unwrap(), damage.valid());
            }
            for _ in 0..3 {
                let (result, counts) = crate::allocation::measure(&mut verify);
                let valid = result.unwrap();
                assert_eq!(
                    valid,
                    damage.valid(),
                    "{field:?} {config:?} {api:?} {damage:?}"
                );
                samples.push(Sample { valid, counts });
            }
            observations.push(Observation {
                api,
                damage,
                corruption,
                samples,
            });
        }
        drop(inputs);
        if let Some((row, offset)) = corruption {
            shards[row][offset] = F::add(shards[row][offset], F::one());
        }
    }
    let parity_bytes = shards[data..]
        .iter()
        .map(|shard| {
            let mut bytes = Vec::with_capacity(elements * size_of::<F::Elem>());
            for &value in shard {
                value.append_bytes(&mut bytes);
            }
            bytes
        })
        .collect();
    Fixture {
        field,
        config,
        element_bytes: size_of::<F::Elem>(),
        parity_bytes,
        observations,
    }
}
