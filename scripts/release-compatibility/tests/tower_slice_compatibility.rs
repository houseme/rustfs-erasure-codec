//! General tower coefficients must preserve the published custom-matrix format.
use codec_new::{Field as _, galois_16::Field as NewField};
use codec_old::{Field as _, galois_16::Field as OldField};

#[test]
fn general_tower_matrices_keep_published_parity_and_cross_recovery() {
    let mut configurations = 0;
    let mut cross_recoveries = 0;
    for data in [2, 4, 10] {
        let alpha = [1, 0];
        let powers: Vec<_> = (0..data).map(|i| OldField::exp(alpha, i)).collect();
        assert!(
            powers
                .iter()
                .enumerate()
                .all(|(i, v)| *v != [0, 0] && !powers[..i].contains(v))
        );
        assert_eq!(
            powers,
            (0..data)
                .map(|i| NewField::exp(alpha, i))
                .collect::<Vec<_>>()
        );
        let rows = vec![vec![[0, 1]; data], powers];
        let baseline = codec_old::ReedSolomon::<OldField>::with_custom_matrix(
            data,
            2,
            &rows,
            codec_old::CodecOptions::default(),
        )
        .unwrap();
        let candidate = codec_new::ReedSolomon::<NewField>::with_custom_matrix(
            data,
            2,
            &rows,
            codec_new::CodecOptions::default(),
        )
        .unwrap();
        for len in [1, 15, 16, 17, 31, 32, 33, 1024, 1025] {
            let original: Vec<Vec<[u8; 2]>> = (0..data + 2)
                .map(|shard| {
                    (0..len)
                        .map(|offset| {
                            [
                                (shard * 47 + offset * 13) as u8,
                                (shard * 71 + offset * 37) as u8,
                            ]
                        })
                        .collect()
                })
                .collect();
            let mut old_codeword = original.clone();
            let mut new_codeword = original;
            baseline.encode(&mut old_codeword).unwrap();
            candidate.encode(&mut new_codeword).unwrap();
            assert_eq!(old_codeword, new_codeword, "data={data}, len={len}");
            assert!(baseline.verify(&new_codeword).unwrap());
            assert!(candidate.verify(&old_codeword).unwrap());
            for missing in [vec![0], vec![0, data - 1], vec![1, data]] {
                let mut for_old: Vec<_> = new_codeword.iter().cloned().map(Some).collect();
                let mut for_new: Vec<_> = old_codeword.iter().cloned().map(Some).collect();
                for index in missing {
                    for_old[index] = None;
                    for_new[index] = None;
                }
                baseline.reconstruct(&mut for_old).unwrap();
                candidate.reconstruct(&mut for_new).unwrap();
                assert!(
                    for_old
                        .iter()
                        .zip(&old_codeword)
                        .all(|(got, expected)| got.as_ref() == Some(expected))
                );
                assert!(
                    for_new
                        .iter()
                        .zip(&new_codeword)
                        .all(|(got, expected)| got.as_ref() == Some(expected))
                );
                cross_recoveries += 2;
            }
            configurations += 1;
        }
    }
    assert_eq!(configurations, 27);
    assert_eq!(cross_recoveries, 162);
    println!(
        "TOWER_PUBLISHED_COMPAT_EXECUTED configurations={configurations} cross_recoveries={cross_recoveries} baseline_backend={} candidate_backend={}",
        codec_old::galois_8::active_backend_name(),
        codec_new::galois_8::active_backend_name()
    );
}
