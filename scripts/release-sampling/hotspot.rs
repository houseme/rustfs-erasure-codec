pub(super) fn release_profile() {
    let cases = [
        (10, 4, "reconstruct_data_missing_1_data", &[0][..], None),
        (10, 4, "reconstruct_data_missing_2_data", &[0, 2][..], None),
        (
            10,
            4,
            "reconstruct_data_missing_data_plus_parity",
            &[1, 10][..],
            None,
        ),
        (
            32,
            16,
            "reconstruct_data_32x16_missing_2_data",
            &[0, 2][..],
            None,
        ),
        (
            10,
            4,
            "reconstruct_some_required_1_of_2_missing_data",
            &[0, 2][..],
            Some(&[2][..]),
        ),
        (
            10,
            4,
            "reconstruct_some_required_2_of_3_missing_data",
            &[0, 2, 4][..],
            Some(&[0, 4][..]),
        ),
        (
            10,
            4,
            "reconstruct_some_required_data_and_skip_parity",
            &[1, 10][..],
            Some(&[1][..]),
        ),
        (
            32,
            16,
            "reconstruct_some_32x16_required_2_of_4_missing_data",
            &[0, 2, 4, 6][..],
            Some(&[2, 6][..]),
        ),
    ];
    for (data, parity, name, missing, required) in cases {
        super::sampling::run(
            name,
            "reconstruction-hotspot-results.json",
            |_| match required {
                None => bench_reconstruct_data_hotspot(data, parity, 1048576, name, missing),
                Some(required) => {
                    bench_reconstruct_some_hotspot(data, parity, 1048576, name, missing, required)
                }
            },
            |r| r.useful_shards as f64 * r.shard_size as f64 * 1e9 / (1048576.0 * r.candidate_mb_s),
            |r| write_reconstruction_hotspot_bench_results(std::slice::from_ref(r)),
        );
    }
}
