//! Selective parity must match published codewords without publishing dependencies.
use codec_new::{Field as NewField, MatrixMode as NewMode};
use codec_old::{Field as OldField, MatrixMode as OldMode};

fn compare<Old: OldField, New: NewField<Elem = Old::Elem>>() -> (usize, usize) {
    let mut configurations = 0;
    let mut recoveries = 0;
    for (old_mode, new_mode) in [
        (OldMode::Vandermonde, NewMode::Vandermonde),
        (OldMode::Cauchy, NewMode::Cauchy),
        (OldMode::JerasureLike, NewMode::JerasureLike),
    ] {
        let mut shapes = vec![(1usize, 2usize), (4, 5), (10, 4), (32, 16)];
        if Old::ORDER > 256 {
            // Force general tower coefficients outside the GF8 subfield.
            shapes.push((2, 255));
        }
        for (data, parity) in shapes {
            let old = codec_old::ReedSolomon::<Old>::with_options(
                data,
                parity,
                codec_old::CodecOptions::builder()
                    .matrix_mode(old_mode)
                    .build(),
            )
            .unwrap();
            let new = codec_new::ReedSolomon::<New>::with_options(
                data,
                parity,
                codec_new::CodecOptions::builder()
                    .matrix_mode(new_mode)
                    .build(),
            )
            .unwrap();
            let mut loss_counts = vec![1, 2.min(data).min(parity - 1), data.min(parity - 1)];
            loss_counts.sort_unstable();
            loss_counts.dedup();
            for len in [1, 15, 16, 17, 31, 32, 33, 65, 1024, 1025] {
                let payload: Vec<Vec<_>> = (0..data + parity)
                    .map(|shard| {
                        (0..len)
                            .map(|offset| Old::nth((shard * 257 + offset * 109 + 1) % Old::ORDER))
                            .collect()
                    })
                    .collect();
                let mut prior = payload.clone();
                let mut current = payload;
                old.encode(&mut prior).unwrap();
                new.encode(&mut current).unwrap();
                assert_eq!(prior, current);
                configurations += 1;
                let target = data + parity - 1;
                for &losses in &loss_counts {
                    for request_data in [false, true] {
                        let mut selected: Vec<_> = prior.iter().cloned().map(Some).collect();
                        for slot in &mut selected[..losses] {
                            *slot = None;
                        }
                        selected[target] = None;
                        let mut required = vec![false; data + parity];
                        required[target] = true;
                        required[0] = request_data;
                        new.reconstruct_some(&mut selected, &required).unwrap();
                        for index in 0..data + parity {
                            let erased = index < losses || index == target;
                            if !erased || required[index] {
                                assert_eq!(selected[index].as_ref(), Some(&prior[index]));
                            } else {
                                assert!(selected[index].is_none(), "unrequested data {index}");
                            }
                        }
                        // Published full recovery reads the candidate's repaired
                        // parity, while candidate full recovery reads prior bytes.
                        let mut for_old = selected.clone();
                        old.reconstruct(&mut for_old).unwrap();
                        new.reconstruct(&mut selected).unwrap();
                        assert!(
                            for_old
                                .iter()
                                .zip(&current)
                                .all(|(a, b)| a.as_ref() == Some(b))
                        );
                        assert!(
                            selected
                                .iter()
                                .zip(&prior)
                                .all(|(a, b)| a.as_ref() == Some(b))
                        );
                        recoveries += 2;
                    }
                }
            }
        }
    }
    (configurations, recoveries)
}

#[test]
fn selective_parity_matches_published_gf8_and_gf16_codewords() {
    let gf8 = compare::<codec_old::galois_8::Field, codec_new::galois_8::Field>();
    let gf16 = compare::<codec_old::galois_16::Field, codec_new::galois_16::Field>();
    assert_eq!(gf8, (120, 1200));
    assert_eq!(gf16, (150, 1440));
    println!("SELECTIVE_PARITY_PUBLISHED_COMPAT_EXECUTED configurations=270 cross_recoveries=2640");
}
