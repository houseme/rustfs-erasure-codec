//! Complete counterbalanced slice diagnostic; never a replacement Vec release case.
use rustfs_erasure_codec::galois_8::{
    ReedSolomon, active_backend_id, active_backend_kind, active_backend_name,
};
use std::{hint::black_box, time::Instant};

struct Block {
    storage: Vec<u8>,
    start: usize,
    len: usize,
    alignment: usize,
}
impl Block {
    fn new(len: usize, offset: usize, page_size: usize) -> Self {
        assert!(page_size.is_power_of_two() && (1024..=1048576).contains(&page_size));
        let alignment = page_size.max(4096);
        // Leave real guards on both sides even if malloc returns an aligned base.
        let storage = vec![0xa5; len + alignment + 64];
        let mut block = Self {
            storage,
            start: 0,
            len,
            alignment,
        };
        block.select(offset);
        block
    }
    fn select(&mut self, offset: usize) {
        assert!([0, 16, 32, 48].contains(&offset));
        self.start = self.alignment - self.storage.as_ptr() as usize % self.alignment + offset;
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
    let page_size = std::env::var("RSE_LAYOUT_PAGE_SIZE")
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let measuring_cpu = std::env::var("RSE_LAYOUT_CPU")
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let check_affinity = || {
        let status = std::fs::read_to_string("/proc/thread-self/status").unwrap();
        let allowed = status
            .lines()
            .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
            .unwrap();
        assert_eq!(allowed.trim(), measuring_cpu.to_string());
        assert_eq!(std::thread::available_parallelism().unwrap().get(), 1);
    };
    check_affinity();
    let codec = ReedSolomon::new(4, 2).unwrap();
    let backend = active_backend_name();
    let backend_id = active_backend_id();
    let backend_kind = active_backend_kind();
    const {
        assert!(cfg!(all(feature = "std", feature = "simd-accel")));
        assert!(!cfg!(feature = "benchmark-metrics"));
    }
    let target_arch = std::env::consts::ARCH;
    let target_os = std::env::consts::OS;
    let iterations = std::env::var("RSE_LAYOUT_ITERATIONS")
        .ok()
        .map(|value| value.parse::<usize>().unwrap())
        .unwrap_or(262144);
    assert!((1..=1_000_000_000).contains(&iterations));
    for len in [1024usize, 4096] {
        let mut blocks: Vec<_> = (0..6).map(|_| Block::new(len, 0, page_size)).collect();
        let mut scratch_storage: Vec<_> = (0..2).map(|_| Block::new(len, 0, page_size)).collect();
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
                    block.select(source_offset);
                }
                for block in &mut scratch_storage {
                    block.select(scratch_offset);
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
                let input_page_offsets: [usize; 6] =
                    std::array::from_fn(|i| inputs[i].as_ptr() as usize % page_size);
                let scratch_page_offsets: [usize; 2] =
                    std::array::from_fn(|i| scratch[i].as_ptr() as usize % page_size);
                let input_4096_offsets: [usize; 6] =
                    std::array::from_fn(|i| inputs[i].as_ptr() as usize % 4096);
                let scratch_4096_offsets: [usize; 2] =
                    std::array::from_fn(|i| scratch[i].as_ptr() as usize % 4096);
                assert_eq!(input_4096_offsets, [source_offset; 6]);
                assert_eq!(scratch_4096_offsets, [scratch_offset; 2]);
                assert_eq!(input_page_offsets, [source_offset; 6]);
                assert_eq!(scratch_page_offsets, [scratch_offset; 2]);
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
                        "\nRSE_LAYOUT_SAMPLE {{\"page_size\":{page_size},\"cpu\":{measuring_cpu},\"backend\":\"{backend}\",\"backend_id\":\"{backend_id:?}\",\"backend_kind\":\"{backend_kind:?}\",\"features\":\"simd-accel|std\",\"target_arch\":\"{target_arch}\",\"target_os\":\"{target_os}\",\"entry\":\"slice_views\",\"fixture_lifetime\":\"same_backing_per_length\",\"kind\":\"{kind}\",\"pass\":{pass},\"index\":{index},\"len\":{len},\"source_offset\":{source_offset},\"scratch_offset\":{scratch_offset},\"iterations\":{iterations},\"wall_ns\":{wall_ns},\"ns_per_iter\":{ns_per_iter},\"input_4096_offsets\":{input_4096_offsets:?},\"scratch_4096_offsets\":{scratch_4096_offsets:?},\"input_page_offsets\":{input_page_offsets:?},\"scratch_page_offsets\":{scratch_page_offsets:?}}}"
                    );
                }
            }
        }
    }
    check_affinity();
}

#[test]
fn layout_views_keep_guards_and_detect_corrupt_parity() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    for source in [0, 16, 32, 48] {
        for output in [0, 16, 32, 48] {
            let mut blocks: Vec<_> = (0..6).map(|_| Block::new(1061, source, 4096)).collect();
            let mut scratch: Vec<_> = (0..2).map(|_| Block::new(1061, output, 4096)).collect();
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

#[test]
fn layout_selection_retains_storage_and_controls_real_page_offsets() {
    for page_size in [1024, 4096, 16384, 65536] {
        for len in [1024, 4096] {
            let mut block = Block::new(len, 0, page_size);
            let base = block.storage.as_ptr();
            for offset in [0, 16, 32, 48, 48, 32, 16, 0] {
                block.select(offset);
                assert_eq!(block.storage.as_ptr(), base);
                assert_eq!(block.bytes().len(), len);
                let address = block.bytes().as_ptr() as usize;
                assert_eq!(address % page_size, offset);
                assert_eq!(address % 4096, offset);
                assert_eq!(address % 64, offset);
                assert!(block.start > 0);
                assert!(block.start + len < block.storage.len());
            }
        }
    }
}
