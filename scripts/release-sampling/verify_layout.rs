//! Complete counterbalanced slice diagnostic; never a replacement Vec release case.
use rustfs_erasure_codec::galois_8::ReedSolomon;
use std::{hint::black_box, time::Instant};

struct Block {
    storage: Vec<u8>,
    start: usize,
    len: usize,
}
impl Block {
    fn new(len: usize, offset: usize) -> Self {
        let storage = vec![0u8; len + 127];
        let start = (64 - (storage.as_ptr() as usize % 64)) % 64 + offset;
        Self {
            storage,
            start,
            len,
        }
    }
    fn bytes(&self) -> &[u8] {
        &self.storage[self.start..self.start + self.len]
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.storage[self.start..self.start + self.len]
    }
}

#[test]
#[ignore]
fn verify_alignment_control() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let iterations = std::env::var("RSE_LAYOUT_ITERATIONS")
        .ok()
        .map(|value| value.parse::<usize>().unwrap())
        .unwrap_or(262144);
    assert!((1..=1_000_000_000).contains(&iterations));
    for len in [1024usize, 4096] {
        let mut blocks: Vec<_> = (0..6).map(|_| Block::new(len, 0)).collect();
        let mut scratch_storage: Vec<_> = (0..2).map(|_| Block::new(len, 0)).collect();
        let layouts: Vec<_> = [0usize, 16, 32, 48]
            .into_iter()
            .flat_map(|a| [0usize, 16, 32, 48].into_iter().map(move |b| (a, b)))
            .collect();
        for pass in 0..2 {
            let order: Vec<_> = if pass == 0 {
                layouts.clone()
            } else {
                layouts.iter().rev().copied().collect()
            };
            for (source_offset, scratch_offset) in order {
                for block in &mut blocks {
                    block.start =
                        (64 - (block.storage.as_ptr() as usize % 64)) % 64 + source_offset;
                }
                for block in &mut scratch_storage {
                    block.start =
                        (64 - (block.storage.as_ptr() as usize % 64)) % 64 + scratch_offset;
                }
                for (row, block) in blocks[..4].iter_mut().enumerate() {
                    for (offset, value) in block.bytes_mut().iter_mut().enumerate() {
                        *value = ((offset * 7 + offset / 251 + row * 31) % 251) as u8;
                    }
                }
                {
                    let (data, parity) = blocks.split_at_mut(4);
                    let input: Vec<_> = data.iter().map(Block::bytes).collect();
                    let mut output: Vec<_> = parity.iter_mut().map(Block::bytes_mut).collect();
                    codec.encode_sep(&input, &mut output).unwrap();
                }
                let inputs: Vec<_> = blocks.iter().map(Block::bytes).collect();
                let mut scratch: Vec<_> =
                    scratch_storage.iter_mut().map(Block::bytes_mut).collect();
                assert!(
                    inputs
                        .iter()
                        .all(|s| s.as_ptr() as usize % 64 == source_offset)
                );
                assert!(
                    scratch
                        .iter()
                        .all(|s| s.as_ptr() as usize % 64 == scratch_offset)
                );
                let input_page_offsets: Vec<_> =
                    inputs.iter().map(|s| s.as_ptr() as usize % 4096).collect();
                let scratch_page_offsets: Vec<_> =
                    scratch.iter().map(|s| s.as_ptr() as usize % 4096).collect();
                for index in 0..12 {
                    let start = Instant::now();
                    for _ in 0..iterations {
                        assert!(black_box(
                            codec
                                .verify_with_buffer(
                                    black_box(inputs.as_slice()),
                                    black_box(scratch.as_mut_slice())
                                )
                                .unwrap()
                        ));
                    }
                    let wall_ns = start.elapsed().as_nanos();
                    let ns_per_iter = wall_ns as f64 / iterations as f64;
                    let kind = if index < 3 { "warmup" } else { "sample" };
                    println!(
                        "\nRSE_LAYOUT_SAMPLE {{\"entry\":\"slice_views\",\"fixture_lifetime\":\"same_backing_per_length\",\"kind\":\"{kind}\",\"pass\":{pass},\"index\":{index},\"len\":{len},\"source_offset\":{source_offset},\"scratch_offset\":{scratch_offset},\"iterations\":{iterations},\"wall_ns\":{wall_ns},\"ns_per_iter\":{ns_per_iter},\"input_page_offsets\":{input_page_offsets:?},\"scratch_page_offsets\":{scratch_page_offsets:?}}}"
                    );
                }
            }
        }
    }
}

#[test]
fn layout_views_keep_guards_and_detect_corrupt_parity() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    for source in [0, 16, 32, 48] {
        for output in [0, 16, 32, 48] {
            let mut blocks: Vec<_> = (0..6).map(|_| Block::new(1061, source)).collect();
            let mut scratch: Vec<_> = (0..2).map(|_| Block::new(1061, output)).collect();
            for block in blocks.iter_mut().chain(&mut scratch) {
                block.storage.fill(0xa5);
            }
            {
                let (data, parity) = blocks.split_at_mut(4);
                let input: Vec<_> = data.iter().map(Block::bytes).collect();
                let mut output: Vec<_> = parity.iter_mut().map(Block::bytes_mut).collect();
                codec.encode_sep(&input, &mut output).unwrap();
            }
            let check = |blocks: &[Block], scratch: &mut [Block]| {
                let input: Vec<_> = blocks.iter().map(Block::bytes).collect();
                let mut output: Vec<_> = scratch.iter_mut().map(Block::bytes_mut).collect();
                codec.verify_with_buffer(&input, &mut output).unwrap()
            };
            assert!(check(&blocks, &mut scratch));
            blocks[5].bytes_mut()[1060] ^= 1;
            assert!(!check(&blocks, &mut scratch));
            for block in blocks.iter().chain(&scratch) {
                assert!(block.storage[..block.start].iter().all(|&v| v == 0xa5));
                assert!(
                    block.storage[block.start + block.len..]
                        .iter()
                        .all(|&v| v == 0xa5)
                );
            }
        }
    }
}
