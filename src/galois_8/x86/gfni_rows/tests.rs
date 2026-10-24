use super::*;
extern crate alloc;
use alloc::{vec, vec::Vec};

fn compare(
    kernel: SelectedRowKernel,
    n: usize,
    p: usize,
    len: usize,
    shifted: bool,
    chunk: usize,
    seed: u8,
) {
    let input_start = usize::from(shifted);
    let output_start = if shifted { 3 } else { 0 };
    let data: Vec<Vec<u8>> = (0..n)
        .map(|j| {
            (0..len + input_start + 5)
                .map(|i| (j * 73 + i * 109 + 17) as u8)
                .collect()
        })
        .collect();
    let inputs: Vec<&[u8]> = data
        .iter()
        .map(|v| &v[input_start..input_start + len])
        .collect();
    let rows: Vec<Vec<u8>> = (0..p)
        .map(|j| {
            (0..n)
                .map(|i| match (i + j) % 4 {
                    0 => 0,
                    1 => 1,
                    _ => seed.wrapping_add((i * 37 + j * 19) as u8),
                })
                .collect()
        })
        .collect();
    let row_refs: Vec<&[u8]> = rows.iter().map(Vec::as_slice).collect();
    let mut output = vec![vec![0xa5; len + output_start + 7]; p];
    let mut expected = output.clone();
    for (row, out) in rows.iter().zip(&mut expected) {
        for index in 0..len {
            out[output_start + index] = row.iter().zip(&inputs).fold(0, |sum, (&c, data)| {
                sum ^ super::super::super::mul(c, data[index])
            });
        }
    }
    let mut views: Vec<&mut [u8]> = output
        .iter_mut()
        .map(|v| &mut v[output_start..output_start + len])
        .collect();
    kernel
        .execute(&row_refs, &inputs, &mut views, len, chunk)
        .unwrap();
    assert_eq!(
        output, expected,
        "inputs={n}, outputs={p}, len={len}, chunk={chunk}, shifted={shifted}"
    );
}

#[test]
fn row_selection_never_substitutes_another_arithmetic_backend() {
    for backend in [
        BackendId::ScalarRust,
        BackendId::RustAvx2,
        BackendId::RustAvx512,
        BackendId::RustSsse3,
        BackendId::RustNeon,
        BackendId::RustVsx,
    ] {
        assert!(SelectedRowKernel::from_backend(backend).is_none());
    }
    assert_eq!(
        SelectedRowKernel::from_backend(BackendId::RustGfniAvx2)
            .unwrap()
            .width,
        32
    );
    assert_eq!(
        SelectedRowKernel::from_backend(BackendId::RustGfniAvx512)
            .unwrap()
            .width,
        64
    );
}

#[cfg(feature = "std")]
#[test]
fn native_gfni_rows_match_scalar_across_coefficients_groups_offsets_and_tails() {
    let avx2 = std::is_x86_feature_detected!("gfni") && std::is_x86_feature_detected!("avx2");
    let avx512 = std::is_x86_feature_detected!("gfni")
        && std::is_x86_feature_detected!("avx512f")
        && std::is_x86_feature_detected!("avx512bw");
    for (supported, backend) in [
        (avx2, BackendId::RustGfniAvx2),
        (avx512, BackendId::RustGfniAvx512),
    ] {
        if !supported {
            continue;
        }
        let kernel = SelectedRowKernel::from_backend(backend).unwrap();
        for n in [4, 5, 8, 10, 31, 32] {
            for p in [1, 4] {
                for len in [
                    0, 1, 31, 32, 33, 63, 64, 65, 255, 1023, 1024, 1025, 4096, 4097,
                ] {
                    for shifted in [false, true] {
                        for chunk in [97, 4096] {
                            compare(kernel, n, p, len, shifted, chunk, 0x9a);
                        }
                    }
                }
            }
        }
        for c in 0..=u8::MAX {
            compare(kernel, 4, 1, 257, true, 64, c);
        }
        let data = [[3u8; 65], [5; 65], [7; 65], [11; 65]];
        let inputs = data.each_ref().map(|v| v.as_slice());
        let rows = [[0u8; 4], [1; 4], [0, 1, 0, 0], [0, 0, 0, 0x9a]];
        let row_refs = rows.each_ref().map(|v| v.as_slice());
        let mut output = vec![vec![0xa5; 65]; 4];
        let mut views: Vec<&mut [u8]> = output.iter_mut().map(Vec::as_mut_slice).collect();
        kernel
            .execute(&row_refs, &inputs, &mut views, 65, 64)
            .unwrap();
        assert_eq!(
            output,
            vec![
                vec![0; 65],
                vec![3 ^ 5 ^ 7 ^ 11; 65],
                vec![5; 65],
                vec![super::super::super::mul(0x9a, 11); 65]
            ]
        );
    }
    if avx2 || avx512 {
        eprintln!("GFNI row kernels executed: avx2={avx2} avx512={avx512}");
    }
}

#[test]
fn row_execution_rejects_complete_invalid_views_before_any_write() {
    // This mock callback has no ISA precondition and would visibly overwrite
    // the first output if validation happened incrementally.
    unsafe fn overwrite(_: &[u8], _: &[&[u8]], out: &mut [u8], _: usize) {
        out.fill(0);
    }
    let kernel = SelectedRowKernel {
        code: overwrite,
        width: 1,
    };
    let input = [0x3c; 64];
    let short = [0x3c; 63];
    let rows = [[1; 4]; 2];
    let rows: Vec<&[u8]> = rows.iter().map(|row| row.as_slice()).collect();
    for invalid in 0..7 {
        let mut rows = rows.clone();
        if invalid == 5 {
            rows[1] = &input[..3];
        }
        if invalid == 6 {
            rows.truncate(1);
        }
        let mut data = vec![&input[..]; 4];
        if invalid == 0 {
            data[3] = &short;
        }
        if invalid == 1 {
            data.truncate(3);
        }
        if invalid == 2 {
            data.resize(33, &input);
        }
        let mut a = [0xa5; 64];
        let mut b = [0xa5; 64];
        let mut outputs = [
            &mut a[..],
            if invalid == 3 {
                &mut b[..63]
            } else {
                &mut b[..]
            },
        ];
        assert!(
            kernel
                .execute(
                    &rows,
                    &data,
                    &mut outputs,
                    64,
                    if invalid == 4 { 0 } else { 64 }
                )
                .is_err()
        );
        assert_eq!(a, [0xa5; 64]);
        assert_eq!(b, [0xa5; 64]);
    }
}
