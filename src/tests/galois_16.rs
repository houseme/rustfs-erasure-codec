extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use super::{fill_random, option_shards_into_shards, shards_into_option_shards};
use crate::galois_16::ReedSolomon;

const QUICKCHECK_MAX_SHARD_LEN: usize = 64;

fn quickcheck_shard_len(size: usize) -> usize {
    1 + size % QUICKCHECK_MAX_SHARD_LEN
}

fn qc_mix(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *seed;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn qc_seed(parts: &[usize]) -> u64 {
    let mut seed = 0x6a09_e667_f3bc_c909_u64;
    for &part in parts {
        seed ^= (part as u64).wrapping_add(0x9e37_79b9_7f4a_7c15);
        let _ = qc_mix(&mut seed);
    }
    seed
}

fn deterministic_shards_u16x2(per_shard: usize, size: usize, seed: u64) -> Vec<Vec<[u8; 2]>> {
    let mut seed = seed;
    let mut shards = Vec::with_capacity(size);
    for _ in 0..size {
        let mut shard = vec![[0; 2]; per_shard];
        for word in &mut shard {
            let value = qc_mix(&mut seed).to_le_bytes();
            word.copy_from_slice(&value[..2]);
        }
        shards.push(shard);
    }
    shards
}

fn deterministic_corrupt_positions(n: usize, total: usize, seed: u64) -> Vec<usize> {
    let n = n.min(total);
    let mut seed = seed;
    let mut positions: Vec<usize> = (0..total).collect();
    for i in 0..n {
        let remaining = total - i;
        let j = i + (qc_mix(&mut seed) as usize % remaining);
        positions.swap(i, j);
    }
    positions.truncate(n);
    positions
}

fn deterministic_fill_u16x2(slice: &mut [[u8; 2]], seed: u64) {
    let mut seed = seed;
    for word in slice {
        let value = qc_mix(&mut seed).to_le_bytes();
        word.copy_from_slice(&value[..2]);
    }
}

fn qc_params(data: usize, parity: usize) -> (usize, usize) {
    let data = 1 + data % 15;
    let mut parity = 1 + parity % 15;
    if data + parity > 16 {
        parity -= data + parity - 16;
    }
    (data, parity)
}

macro_rules! make_random_shards {
    ($per_shard:expr, $size:expr) => {{
        let mut shards = Vec::with_capacity($size);
        for _ in 0..$size {
            shards.push(vec![[0; 2]; $per_shard]);
        }

        for s in shards.iter_mut() {
            fill_random(s);
        }

        shards
    }};
}

#[test]
fn correct_field_order_restriction() {
    const ORDER: usize = 1 << 16;

    assert!(ReedSolomon::new(ORDER, 1).is_err());
    assert!(ReedSolomon::new(1, ORDER).is_err());

    // Disabled here because constructing the 65535 x 65535 Vandermonde matrix
    // is prohibitively expensive for the unit test suite.
    // assert!(ReedSolomon::new(ORDER - 1, 1).is_ok());
    assert!(ReedSolomon::new(1, ORDER - 1).is_ok());
}

quickcheck! {
    fn qc_encode_verify_reconstruct_verify(data: usize,
                                           parity: usize,
                                           corrupt: usize,
                                           size: usize) -> bool {
        let (data, parity) = qc_params(data, parity);
        let corrupt = corrupt % (parity + 1);
        let size = quickcheck_shard_len(size);
        let total = data + parity;
        let seed = qc_seed(&[data, parity, corrupt, size, 0]);
        let corrupt_pos_s =
            deterministic_corrupt_positions(corrupt, total, seed ^ 0x1000);

        let r = ReedSolomon::new(data, parity).unwrap();

        let mut expect = deterministic_shards_u16x2(size, total, seed);
        {
            let mut refs =
                convert_2D_slices!(expect =>to_mut_vec &mut [[u8; 2]]);

            r.encode(&mut refs).unwrap();
        }

        let expect = expect;

        let mut shards = expect.clone();

        // corrupt shards
        for (i, &p) in corrupt_pos_s.iter().enumerate() {
            deterministic_fill_u16x2(&mut shards[p], seed ^ 0x2000 ^ i as u64);
        }
        let mut slice_present = vec![true; total];
        for &p in corrupt_pos_s.iter() {
            slice_present[p] = false;
        }

        // reconstruct
        {
            let mut refs: Vec<_> = shards.iter_mut()
                .map(|i| &mut i[..])
                .zip(slice_present.iter().cloned())
                .collect();

            r.reconstruct(&mut refs[..]).unwrap();
        }

        ({
            let refs =
                convert_2D_slices!(expect =>to_vec &[[u8; 2]]);

            r.verify(&refs).unwrap()
        })
            &&
            expect == shards
            &&
            ({
                let refs =
                    convert_2D_slices!(shards =>to_vec &[[u8; 2]]);

                r.verify(&refs).unwrap()
            })
    }

    fn qc_encode_verify_reconstruct_verify_shards(data: usize,
                                                  parity: usize,
                                                  corrupt: usize,
                                                  size: usize) -> bool {
        let (data, parity) = qc_params(data, parity);
        let corrupt = corrupt % (parity + 1);
        let size = quickcheck_shard_len(size);
        let total = data + parity;
        let seed = qc_seed(&[data, parity, corrupt, size, 1]);
        let corrupt_pos_s =
            deterministic_corrupt_positions(corrupt, total, seed ^ 0x1000);

        let r = ReedSolomon::new(data, parity).unwrap();

        let mut expect = deterministic_shards_u16x2(size, total, seed);
        r.encode(&mut expect).unwrap();

        let expect = expect;

        let mut shards = shards_into_option_shards(expect.clone());

        // corrupt shards
        for &p in corrupt_pos_s.iter() {
            shards[p] = None;
        }

        // reconstruct
        r.reconstruct(&mut shards).unwrap();

        let shards = option_shards_into_shards(shards);

        r.verify(&expect).unwrap()
            && expect == shards
            && r.verify(&shards).unwrap()
    }

    fn qc_verify(data: usize,
                 parity: usize,
                 corrupt: usize,
                 size: usize) -> bool {
        let (data, parity) = qc_params(data, parity);
        let corrupt = corrupt % (parity + 1);
        let size = quickcheck_shard_len(size);
        let total = data + parity;
        let seed = qc_seed(&[data, parity, corrupt, size, 2]);
        let corrupt_pos_s =
            deterministic_corrupt_positions(corrupt, total, seed ^ 0x1000);

        let r = ReedSolomon::new(data, parity).unwrap();

        let mut expect = deterministic_shards_u16x2(size, total, seed);
        {
            let mut refs =
                convert_2D_slices!(expect =>to_mut_vec &mut [[u8; 2]]);

            r.encode(&mut refs).unwrap();
        }

        let expect = expect;

        let mut shards = expect.clone();

        // corrupt shards
        for (i, &p) in corrupt_pos_s.iter().enumerate() {
            deterministic_fill_u16x2(&mut shards[p], seed ^ 0x2000 ^ i as u64);
        }

        ({
            let refs =
                convert_2D_slices!(expect =>to_vec &[[u8; 2]]);

            r.verify(&refs).unwrap()
        })
            &&
            ((corrupt > 0 && expect != shards)
             || (corrupt == 0 && expect == shards))
            &&
            ({
                let refs =
                    convert_2D_slices!(shards =>to_vec &[[u8; 2]]);

                (corrupt > 0 && !r.verify(&refs).unwrap())
                    || (corrupt == 0 && r.verify(&refs).unwrap())
            })
    }

    fn qc_verify_shards(data: usize,
                        parity: usize,
                        corrupt: usize,
                        size: usize) -> bool {
        let (data, parity) = qc_params(data, parity);
        let corrupt = corrupt % (parity + 1);
        let size = quickcheck_shard_len(size);
        let total = data + parity;
        let seed = qc_seed(&[data, parity, corrupt, size, 3]);
        let corrupt_pos_s =
            deterministic_corrupt_positions(corrupt, total, seed ^ 0x1000);

        let r = ReedSolomon::new(data, parity).unwrap();

        let mut expect = deterministic_shards_u16x2(size, total, seed);
        r.encode(&mut expect).unwrap();

        let expect = expect;

        let mut shards = expect.clone();

        // corrupt shards
        for (i, &p) in corrupt_pos_s.iter().enumerate() {
            deterministic_fill_u16x2(&mut shards[p], seed ^ 0x2000 ^ i as u64);
        }

        r.verify(&expect).unwrap()
            &&
            ((corrupt > 0 && expect != shards)
             || (corrupt == 0 && expect == shards))
            &&
            ((corrupt > 0 && !r.verify(&shards).unwrap())
             || (corrupt == 0 && r.verify(&shards).unwrap()))
    }

    fn qc_encode_sep_same_as_encode(data: usize,
                                    parity: usize,
                                    size: usize) -> bool {
        let (data, parity) = qc_params(data, parity);
        let size = quickcheck_shard_len(size);

        let r = ReedSolomon::new(data, parity).unwrap();

        let mut expect = make_random_shards!(size, data + parity);
        let mut shards = expect.clone();

        {
            let mut refs =
                convert_2D_slices!(expect =>to_mut_vec &mut [[u8; 2]]);

            r.encode(&mut refs).unwrap();
        }

        let expect = expect;

        {
            let (data, parity) = shards.split_at_mut(data);

            let data_refs =
                convert_2D_slices!(data =>to_mut_vec &[[u8; 2]]);

            let mut parity_refs =
                convert_2D_slices!(parity =>to_mut_vec &mut [[u8; 2]]);

            r.encode_sep(&data_refs, &mut parity_refs).unwrap();
        }

        let shards = shards;

        expect == shards
    }

    fn qc_encode_sep_same_as_encode_shards(data: usize,
                                           parity: usize,
                                           size: usize) -> bool {
        let (data, parity) = qc_params(data, parity);
        let size = quickcheck_shard_len(size);

        let r = ReedSolomon::new(data, parity).unwrap();

        let mut expect = make_random_shards!(size, data + parity);
        let mut shards = expect.clone();

        r.encode(&mut expect).unwrap();

        let expect = expect;

        {
            let (data, parity) = shards.split_at_mut(data);

            r.encode_sep(data, parity).unwrap();
        }

        let shards = shards;

        expect == shards
    }

    fn qc_encode_single_same_as_encode(data: usize,
                                       parity: usize,
                                       size: usize) -> bool {
        let (data, parity) = qc_params(data, parity);
        let size = quickcheck_shard_len(size);

        let r = ReedSolomon::new(data, parity).unwrap();

        let mut expect = make_random_shards!(size, data + parity);
        let mut shards = expect.clone();

        {
            let mut refs =
                convert_2D_slices!(expect =>to_mut_vec &mut [[u8; 2]]);

            r.encode(&mut refs).unwrap();
        }

        let expect = expect;

        {
            let mut refs =
                convert_2D_slices!(shards =>to_mut_vec &mut [[u8; 2]]);

            for i in 0..data {
                r.encode_single(i, &mut refs).unwrap();
            }
        }

        let shards = shards;

        expect == shards
    }

    fn qc_encode_single_same_as_encode_shards(data: usize,
                                              parity: usize,
                                              size: usize) -> bool {
        let (data, parity) = qc_params(data, parity);
        let size = quickcheck_shard_len(size);

        let r = ReedSolomon::new(data, parity).unwrap();

        let mut expect = make_random_shards!(size, data + parity);
        let mut shards = expect.clone();

        r.encode(&mut expect).unwrap();

        let expect = expect;

        for i in 0..data {
            r.encode_single(i, &mut shards).unwrap();
        }

        let shards = shards;

        expect == shards
    }

    fn qc_encode_single_sep_same_as_encode(data: usize,
                                           parity: usize,
                                           size: usize) -> bool {
        let (data, parity) = qc_params(data, parity);
        let size = quickcheck_shard_len(size);

        let r = ReedSolomon::new(data, parity).unwrap();

        let mut expect = make_random_shards!(size, data + parity);
        let mut shards = expect.clone();

        {
            let mut refs =
                convert_2D_slices!(expect =>to_mut_vec &mut [[u8; 2]]);

            r.encode(&mut refs).unwrap();
        }

        let expect = expect;

        {
            let (data_shards, parity_shards) = shards.split_at_mut(data);

            let data_refs =
                convert_2D_slices!(data_shards =>to_mut_vec &[[u8; 2]]);

            let mut parity_refs =
                convert_2D_slices!(parity_shards =>to_mut_vec &mut [[u8; 2]]);

            for (i, shard) in data_refs.iter().enumerate().take(data) {
                r.encode_single_sep(i, shard, &mut parity_refs).unwrap();
            }
        }

        let shards = shards;

        expect == shards
    }

    fn qc_encode_single_sep_same_as_encode_shards(data: usize,
                                                  parity: usize,
                                                  size: usize) -> bool {
        let (data, parity) = qc_params(data, parity);
        let size = quickcheck_shard_len(size);

        let r = ReedSolomon::new(data, parity).unwrap();

        let mut expect = make_random_shards!(size, data + parity);
        let mut shards = expect.clone();

        r.encode(&mut expect).unwrap();

        let expect = expect;

        {
            let (data_shards, parity_shards) = shards.split_at_mut(data);

            for (i, shard) in data_shards.iter().enumerate().take(data) {
                r.encode_single_sep(i, shard, parity_shards).unwrap();
            }
        }

        let shards = shards;

        expect == shards
    }
}
