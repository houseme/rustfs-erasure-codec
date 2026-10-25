use std::{marker::PhantomData, rc::Rc};

// The marker changes auto traits without changing field bytes. This is a
// downstream serial-API compatibility fixture, not a native-byte opt-in.
#[repr(transparent)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
struct LocalElem(u8, PhantomData<Rc<()>>);

fn element(value: u8) -> LocalElem {
    LocalElem(value, PhantomData)
}

macro_rules! field {
    ($name:ident, $ec:ident) => {
        struct $name;
        impl $ec::Field for $name {
            const ORDER: usize = 256;
            type Elem = LocalElem;
            fn add(a: LocalElem, b: LocalElem) -> LocalElem {
                element($ec::galois_8::add(a.0, b.0))
            }
            fn mul(a: LocalElem, b: LocalElem) -> LocalElem {
                element($ec::galois_8::mul(a.0, b.0))
            }
            fn div(a: LocalElem, b: LocalElem) -> LocalElem {
                element($ec::galois_8::div(a.0, b.0))
            }
            fn exp(a: LocalElem, n: usize) -> LocalElem {
                element($ec::galois_8::exp(a.0, n))
            }
            fn zero() -> LocalElem {
                element(0)
            }
            fn one() -> LocalElem {
                element(1)
            }
            fn nth_internal(n: usize) -> LocalElem {
                element(n as u8)
            }
        }
    };
}
field!(OldField, codec_old);
field!(NewField, codec_new);

macro_rules! options {
    ($ec:ident, $mode:expr) => {
        $ec::CodecOptions::builder()
            .codec_family($ec::CodecFamily::Classic)
            .matrix_mode(match $mode {
                0 => $ec::MatrixMode::Vandermonde,
                1 => $ec::MatrixMode::Cauchy,
                2 => $ec::MatrixMode::JerasureLike,
                _ => unreachable!(),
            })
            .build()
    };
}

fn missing(original: &[Vec<LocalElem>], indices: &[usize]) -> Vec<Option<Vec<LocalElem>>> {
    let mut shards: Vec<_> = original.iter().cloned().map(Some).collect();
    for &i in indices {
        shards[i] = None;
    }
    shards
}

macro_rules! exercise_serial_api {
    ($ec:ident, $codec:expr, $reference:expr) => {{
        let codec = $codec;
        let reference: &[Vec<LocalElem>] = $reference;
        let len = reference[0].len();
        let mut counts = [0usize; 3];
        let mut parity = vec![vec![element(0); len]; 3];
        codec
            .encode_sep(&reference[..4], &mut parity)
            .expect("separate serial encode");
        assert_eq!(parity, reference[4..]);
        counts[0] += 1;

        let mut incremental = reference.to_vec();
        incremental[4..].iter_mut().for_each(|s| s.fill(element(0)));
        for i in 0..4 {
            codec
                .encode_single(i, &mut incremental)
                .expect("single serial encode");
        }
        assert_eq!(incremental, reference);
        counts[0] += 1;

        parity.iter_mut().for_each(|s| s.fill(element(0)));
        for (i, data) in reference[..4].iter().enumerate() {
            codec
                .encode_single_sep(i, data, &mut parity)
                .expect("single separated encode");
        }
        assert_eq!(parity, reference[4..]);
        counts[0] += 1;

        let mut workspace = $ec::VerifyWorkspace::new(codec, len);
        assert!(codec.verify(reference).expect("serial verify"));
        assert!(
            codec
                .verify_with_buffer(reference, &mut parity)
                .expect("buffer verify")
        );
        assert!(
            codec
                .verify_with_workspace(reference, &mut workspace)
                .expect("workspace verify")
        );
        counts[1] += 3;
        let mut corrupt = reference.to_vec();
        corrupt[6][len - 1].0 ^= 0x5a;
        assert!(!codec.verify(&corrupt).expect("corrupt serial verify"));
        assert!(
            !codec
                .verify_with_buffer(&corrupt, &mut parity)
                .expect("corrupt buffer verify")
        );
        assert!(
            !codec
                .verify_with_workspace(&corrupt, &mut workspace)
                .expect("corrupt workspace verify")
        );
        counts[1] += 3;

        for indices in [&[0][..], &[6][..], &[0, 6][..], &[0, 1, 6][..]] {
            let expected: Vec<_> = reference.iter().cloned().map(Some).collect();
            let mut shards = missing(reference, indices);
            codec
                .reconstruct(&mut shards)
                .expect("serial cross reconstruction");
            assert_eq!(shards, expected);
            counts[2] += 1;

            let mut shards = missing(reference, indices);
            codec
                .reconstruct_data(&mut shards)
                .expect("data-only serial cross reconstruction");
            assert_eq!(&shards[..4], &expected[..4]);
            for (i, (actual, expected)) in shards.iter().zip(&expected).enumerate().skip(4) {
                if indices.contains(&i) {
                    assert!(actual.is_none());
                } else {
                    assert_eq!(actual, expected);
                }
            }
            counts[2] += 1;

            let mut shards = missing(reference, indices);
            let mut required = [false; 7];
            for &i in indices {
                required[i] = true;
            }
            codec
                .reconstruct_some(&mut shards, &required)
                .expect("required serial cross reconstruction");
            assert_eq!(shards, expected);
            counts[2] += 1;
        }
        counts
    }};
}

#[test]
fn serial_public_apis_keep_non_send_and_non_sync_element_support() {
    let mut cases = 0;
    let mut totals = [0usize; 3];
    for mode in 0..3 {
        let old_codec =
            codec_old::ReedSolomon::<OldField>::with_options(4, 3, options!(codec_old, mode))
                .unwrap();
        let new_codec =
            codec_new::ReedSolomon::<NewField>::with_options(4, 3, options!(codec_new, mode))
                .unwrap();
        for len in [1, 15, 16, 31, 32, 33, 63, 64, 65, 255] {
            let mut original: Vec<_> = (0..7)
                .map(|row| {
                    (0..len)
                        .map(|column| {
                            element(if row < 4 {
                                (row * 41 + column * 13) as u8
                            } else {
                                0
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
            let mut candidate = original.clone();
            old_codec.encode(&mut original).unwrap();
            new_codec.encode(&mut candidate).unwrap();
            assert_eq!(candidate, original, "matrix {mode}, length {len}");
            totals[0] += 2;
            // Consume parity produced by the other version in each direction.
            let a = exercise_serial_api!(codec_old, &old_codec, &candidate);
            let b = exercise_serial_api!(codec_new, &new_codec, &original);
            for ((total, a), b) in totals.iter_mut().zip(a).zip(b) {
                *total += a + b;
            }
            cases += 1;
        }
    }
    assert_eq!(cases, 30);
    assert_eq!(totals, [240, 360, 720]);
    println!(
        "LOCAL_ELEMENT_COMPAT cases={cases} encoding_checks={} verification_checks={} cross_recoveries={} opaque_element=true",
        totals[0], totals[1], totals[2]
    );
}
