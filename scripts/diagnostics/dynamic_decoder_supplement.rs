#[test]
#[ignore]
fn release_dynamic_decoder_profile() {
    use rustfs_erasure_codec::__rse_dynamic_buffers as observer;
    use rustfs_erasure_codec::{CodecFamily, CodecOptions, Field, ReedSolomon};
    fn run<F: Field>(key: &str, size: usize, family: CodecFamily, cache: bool, page: usize) {
        let _root = observer::Root::start(page);
        let budget = std::thread::available_parallelism().unwrap().get();
        let codec = ReedSolomon::<F>::with_options(
            10,
            4,
            CodecOptions::builder()
                .codec_family(family)
                .fast_one_parity(false)
                .inversion_cache(cache)
                .build(),
        )
        .unwrap();
        let mut source: Vec<Vec<F::Elem>> = (0..14)
            .map(|row| {
                (0..size)
                    .map(|index| F::nth((row * 29 + index * 17 + 13) % F::ORDER))
                    .collect()
            })
            .collect();
        codec.encode(&mut source).unwrap();
        let placement = sampling::rse_dynamic_placement(budget);
        for observation in 0..12 {
            observer::observation(observation);
            for _ in 0..2 {
                placement();
                let _epoch = observer::Epoch::begin();
                observer::elements("source", "retained", &source);
                let mut shards: Vec<_> = source.iter().cloned().map(Some).collect();
                observer::optional_elements("dynamic", "cloned", &shards);
                shards[0] = None;
                shards[10] = None;
                observer::optional_elements("dynamic", "missing", &shards);
                codec.reconstruct(&mut shards).unwrap();
                observer::optional_elements("dynamic", "recovered", &shards);
                assert!(
                    shards
                        .iter()
                        .zip(&source)
                        .all(|(a, b)| a.as_ref() == Some(b))
                );
                placement();
            }
        }
        println!(
            "\nRSE_DYNAMIC_SUPPLEMENT {{\"schema\":\"decoder-owner-supplement-v1\",\"observer\":\"dynamic-source-copy-v1\",\"case\":\"{key}\",\"data_shards\":10,\"parity_shards\":4,\"shard_size_elements\":{size},\"element_size\":{},\"family\":\"{family:?}\",\"inversion_cache\":{cache},\"fast_one_parity\":false,\"source_lifetime\":\"worker\",\"construction\":\"codec-then-source-then-explicit-pool-preparation\",\"output_policy\":\"iteration-cloned-options-drop-missing-reconstruct-all\",\"missing_mask\":1025,\"required_mask\":16383,\"constructor_budget\":{budget},\"pool_threads\":{},\"native_placement_checked\":{},\"normal_outputs_match_source\":true,\"release_approval\":false}}",
            std::mem::size_of::<F::Elem>(),
            rayon::current_num_threads(),
            cfg!(target_os = "linux")
        );
    }
    let key = std::env::var("RSE_DYNAMIC_DECODER_CASE").unwrap();
    let page = std::env::var("RSE_SAMPLER_PAGE_SIZE")
        .unwrap()
        .parse()
        .unwrap();
    match key.as_str() {
        "classic_uncached:10:4:16384" => run::<rustfs_erasure_codec::galois_8::Field>(
            &key,
            16384,
            CodecFamily::Classic,
            false,
            page,
        ),
        "tower_gf16:10:4:65" => run::<rustfs_erasure_codec::galois_16::Field>(
            &key,
            65,
            CodecFamily::Classic,
            false,
            page,
        ),
        "leopard_gf8:10:4:32768" => run::<rustfs_erasure_codec::galois_8::Field>(
            &key,
            32768,
            CodecFamily::LeopardGF8,
            true,
            page,
        ),
        "leopard_gf8:10:4:65600" => run::<rustfs_erasure_codec::galois_8::Field>(
            &key,
            65600,
            CodecFamily::LeopardGF8,
            true,
            page,
        ),
        "leopard_gf16:10:4:32768" => run::<rustfs_erasure_codec::galois_8::Field>(
            &key,
            32768,
            CodecFamily::LeopardGF16,
            true,
            page,
        ),
        "leopard_gf16:10:4:65600" => run::<rustfs_erasure_codec::galois_8::Field>(
            &key,
            65600,
            CodecFamily::LeopardGF16,
            true,
            page,
        ),
        _ => panic!("unknown fixed decoder-owner supplement"),
    }
}
