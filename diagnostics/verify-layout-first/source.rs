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
    let iterations = 262144usize;
    for len in [1024usize, 4096] {
        let mut blocks: Vec<_> = (0..6).map(|_| Block::new(len, 0)).collect();
        let mut scratch_storage: Vec<_> = (0..2).map(|_| Block::new(len, 0)).collect();
        for source_offset in [0usize, 16, 32, 48] {
            for scratch_offset in [0usize, 16, 32, 48] {
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
                        "\nALIGNMENT_SAMPLE {{\"kind\":\"{kind}\",\"index\":{index},\"len\":{len},\"source_offset\":{source_offset},\"scratch_offset\":{scratch_offset},\"iterations\":{iterations},\"wall_ns\":{wall_ns},\"ns_per_iter\":{ns_per_iter},\"input_page_offsets\":{input_page_offsets:?},\"scratch_page_offsets\":{scratch_page_offsets:?}}}"
                    );
                }
            }
        }
    }
}
