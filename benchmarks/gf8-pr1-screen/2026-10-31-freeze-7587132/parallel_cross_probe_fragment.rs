/// Temporary large parallel-recovery cross-version probe; not included in product code.
pub(super) fn parallel_gf8(peer: Option<&[super::Snapshot]>) -> Vec<super::Snapshot> {
    use rustfs_erasure_codec::galois_8::ReedSolomon;
    let mut snapshots = Vec::new();
    if let Some(peer) = peer {
        assert_eq!(peer.len(), 8);
    }
    for (mode_index, mode) in [
        MatrixMode::Vandermonde,
        MatrixMode::Cauchy,
        MatrixMode::JerasureLike,
        MatrixMode::Custom,
    ]
    .into_iter()
    .enumerate()
    {
        let options = CodecOptions::builder()
            .matrix_mode(mode)
            .max_parallel_jobs(4)
            .build();
        let codec = if mode == MatrixMode::Custom {
            let reference = ReedSolomon::new(10, 4).unwrap();
            let basis: Vec<Vec<u8>> = (0..10)
                .map(|r| (0..10).map(|c| u8::from(r == c)).collect())
                .collect();
            let mut rows = vec![vec![0; 10]; 4];
            reference.encode_sep(&basis, &mut rows).unwrap();
            rows.reverse();
            ReedSolomon::with_custom_matrix(10, 4, &rows, options).unwrap()
        } else {
            ReedSolomon::with_options(10, 4, options).unwrap()
        };
        for len in [1_048_576, 1_048_577] {
            let id = format!("gf8/Classic/10+4/parallel-matrix{mode_index}/{len}");
            let mut shards: Vec<Vec<u8>> = (0..14)
                .map(|r| (0..len).map(|i| (r * 73 + i * 19 + i / 17) as u8).collect())
                .collect();
            codec.encode(&mut shards).unwrap();
            if let Some(peer) = peer {
                let reference = &peer[snapshots.len()];
                assert_eq!(reference.id, id);
                for (dst, src) in shards[10..].iter_mut().zip(&reference.parity) {
                    dst.clone_from(src);
                }
            }
            assert!(codec.verify(&shards).unwrap(), "verification: {id}");
            let mut lost: Vec<Option<Vec<u8>>> = shards.iter().cloned().map(Some).collect();
            lost[3] = None;
            codec.reconstruct_data_opt(&mut lost).unwrap();
            assert!(
                lost.iter().zip(&shards).all(|(a, b)| a.as_ref() == Some(b)),
                "recovery: {id}"
            );
            snapshots.push(super::Snapshot {
                id,
                parity: shards[10..].to_vec(),
            });
        }
    }
    snapshots
}
