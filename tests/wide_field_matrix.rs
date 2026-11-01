#![cfg(feature = "experimental-matrix-inline512")]

use rustfs_erasure_codec::{CodecOptions, Field, MatrixMode, ReedSolomon};

#[derive(Clone, Copy, Debug, PartialEq)]
struct Wide([u8; 256]);
impl Default for Wide {
    fn default() -> Self {
        Self([0; 256])
    }
}

struct WideField;
impl WideField {
    fn wrap(x: u8) -> Wide {
        Wide([x; 256])
    }
}
impl Field for WideField {
    const ORDER: usize = 256;
    type Elem = Wide;
    fn add(a: Wide, b: Wide) -> Wide {
        Self::wrap(rustfs_erasure_codec::galois_8::add(a.0[0], b.0[0]))
    }
    fn mul(a: Wide, b: Wide) -> Wide {
        Self::wrap(rustfs_erasure_codec::galois_8::mul(a.0[0], b.0[0]))
    }
    fn div(a: Wide, b: Wide) -> Wide {
        Self::wrap(rustfs_erasure_codec::galois_8::div(a.0[0], b.0[0]))
    }
    fn exp(a: Wide, n: usize) -> Wide {
        Self::wrap(rustfs_erasure_codec::galois_8::exp(a.0[0], n))
    }
    fn zero() -> Wide {
        Self::wrap(0)
    }
    fn one() -> Wide {
        Self::wrap(1)
    }
    fn nth_internal(n: usize) -> Wide {
        Self::wrap(n as u8)
    }
}

fn check() {
    for (data_count, parity_count) in [(4, 2), (10, 4)] {
        for mode in [
            MatrixMode::Vandermonde,
            MatrixMode::Cauchy,
            MatrixMode::JerasureLike,
            MatrixMode::Custom,
        ] {
            let options = CodecOptions::builder().matrix_mode(mode).build();
            let codec = if mode == MatrixMode::Custom {
                let reference = ReedSolomon::<WideField>::new(data_count, parity_count).unwrap();
                let mut rows: Vec<_> = (data_count..data_count + parity_count)
                    .map(|row| {
                        reference
                            .encoding_format()
                            .matrix_row(row)
                            .unwrap()
                            .to_vec()
                    })
                    .collect();
                rows.reverse();
                ReedSolomon::<WideField>::with_custom_matrix(
                    data_count,
                    parity_count,
                    &rows,
                    options,
                )
                .unwrap()
            } else {
                ReedSolomon::<WideField>::with_options(data_count, parity_count, options).unwrap()
            };
            for len in [1, 64, 257] {
                let data: Vec<Vec<Wide>> = (0..data_count)
                    .map(|row| {
                        (0..len)
                            .map(|i| WideField::nth((row * 37 + i * 13) % 256))
                            .collect()
                    })
                    .collect();
                let mut parity = vec![vec![WideField::zero(); len]; parity_count];
                codec.encode_sep(&data, &mut parity).unwrap();
                let mut expected = vec![vec![WideField::zero(); len]; parity_count];
                for (index, input) in data.iter().enumerate() {
                    codec
                        .encode_single_sep(index, input, &mut expected)
                        .unwrap();
                }
                assert_eq!(parity, expected, "mode={mode:?}, len={len}");
                let mut shards: Vec<_> = data
                    .iter()
                    .cloned()
                    .map(Some)
                    .chain(parity.into_iter().map(Some))
                    .collect();
                shards[0] = None;
                codec.reconstruct_data(&mut shards).unwrap();
                assert_eq!(shards[0].as_deref(), Some(data[0].as_slice()));
            }
        }
    }
}

#[test]
fn wide_field_preserves_matrix_parity_and_recovery() {
    // The generic field makes matrix values large even with the smaller inline
    // capacity. Use a controlled stack for portable debug and release tests;
    // the constrained-stack experiment is documented separately.
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(check)
        .unwrap()
        .join()
        .unwrap();
}

#[cfg(all(target_os = "macos", target_arch = "aarch64", not(debug_assertions)))]
#[test]
fn wide_field_roundtrips_on_the_local_default_test_stack() {
    const CHILD: &str = "RSE_WIDE_MATRIX_DEFAULT_STACK_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let codec = ReedSolomon::<WideField>::new(4, 2).unwrap();
        let mut shards: Vec<Vec<Wide>> = (0..6)
            .map(|row| {
                (0..65)
                    .map(|i| WideField::nth((row * 37 + i * 13) % 256))
                    .collect()
            })
            .collect();
        codec.encode(&mut shards).unwrap();
        let original = shards.clone();
        let mut missing: Vec<_> = shards.into_iter().map(Some).collect();
        missing[0] = None;
        codec.reconstruct_data(&mut missing).unwrap();
        assert_eq!(missing[0].as_deref(), Some(original[0].as_slice()));
        return;
    }
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "wide_field_roundtrips_on_the_local_default_test_stack",
        ])
        .env(CHILD, "1")
        .env_remove("RUST_MIN_STACK")
        .status()
        .unwrap();
    assert!(
        status.success(),
        "wide-field child failed on its default test stack"
    );
}
