#![no_main]

use libfuzzer_sys::fuzz_target;
use rustfs_erasure_codec::galois_8::ReedSolomon;

fuzz_target!(|data: &[u8]| {
    if data.len() >= 7 {
        let data_shards = data[0] as usize;
        let parity_shards = data[1] as usize;
        let shard_size = data[2] as usize;
        let run_count = data[3] as usize;
        let interval = data[4] as usize;
        let corrupt_count = data[5] as usize;
        let corrupt_index = data[6] as usize;

        let data = &data[7..];

        if data_shards > 0
            && parity_shards > 0
            && shard_size > 0
            && interval > 0
            && corrupt_count <= parity_shards
            && data_shards + parity_shards <= 256
            && data.len() == data_shards * shard_size
        {
            let codec = ReedSolomon::new(data_shards, parity_shards).unwrap();

            for _ in 0..run_count {
                assert_eq!(codec.data_shard_count(), data_shards);
                assert_eq!(codec.parity_shard_count(), parity_shards);
                assert_eq!(codec.total_shard_count(), data_shards + parity_shards);

                let mut data_buffer: Vec<u8> = vec![0u8; data_shards * shard_size];
                data_buffer.copy_from_slice(data);
                let mut data = data_buffer;
                let mut parity_buffer: Vec<u8> = vec![0u8; shard_size * parity_shards];
                {
                    let data_slices: Vec<&[u8]> = data.chunks(shard_size).collect();
                    let mut parity_slices: Vec<&mut [u8]> =
                        parity_buffer.chunks_mut(shard_size).collect();

                    codec.encode_sep(&data_slices, &mut parity_slices).unwrap();
                }

                let expected_data = data.clone();
                let expected_parity = parity_buffer.clone();

                {
                    let mut data_slices: Vec<_> = data
                        .chunks_mut(shard_size)
                        .map(|shard| (shard, true))
                        .collect();
                    let mut parity_slices: Vec<_> = parity_buffer
                        .chunks_mut(shard_size)
                        .map(|shard| (shard, true))
                        .collect();

                    let mut slices = Vec::with_capacity(data_shards + parity_shards);
                    slices.append(&mut data_slices);
                    slices.append(&mut parity_slices);

                    for i in 0..corrupt_count {
                        let corrupt =
                            (corrupt_index + i * interval) % (data_shards + parity_shards);
                        slices[corrupt].1 = false;
                        slices[corrupt].0.fill(0);
                    }
                }

                {
                    let mut data_slices: Vec<_> = data
                        .chunks_mut(shard_size)
                        .map(|shard| (shard, true))
                        .collect();
                    let mut parity_slices: Vec<_> = parity_buffer
                        .chunks_mut(shard_size)
                        .map(|shard| (shard, true))
                        .collect();

                    let mut slices = Vec::with_capacity(data_shards + parity_shards);
                    slices.append(&mut data_slices);
                    slices.append(&mut parity_slices);

                    for i in 0..corrupt_count {
                        let corrupt =
                            (corrupt_index + i * interval) % (data_shards + parity_shards);
                        slices[corrupt].1 = false;
                    }

                    codec.reconstruct(&mut slices).unwrap();
                }

                assert_eq!(data, expected_data);
                assert_eq!(parity_buffer, expected_parity);

                {
                    let data_slices: Vec<&[u8]> = data.chunks(shard_size).collect();
                    let parity_slices: Vec<&[u8]> = parity_buffer.chunks(shard_size).collect();

                    let mut slices = Vec::with_capacity(data_shards + parity_shards);
                    for &d in data_slices.iter() {
                        slices.push(d);
                    }
                    for &p in parity_slices.iter() {
                        slices.push(p);
                    }

                    assert!(codec.verify(&slices).unwrap());
                }
            }
        }
    }
});
