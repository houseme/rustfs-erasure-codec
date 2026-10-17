#![cfg(feature = "std")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::io::{Cursor, Read};

use rustfs_erasure_codec::Error;
use rustfs_erasure_codec::galois_8::ReedSolomon;
use rustfs_erasure_codec::stream::{StreamErrorKind, StreamIoMode, StreamOptions};

const BLOCK_SIZE: usize = 1024;

// Count heap requests and block-sized scratch on this test thread only. Input
// and output storage is prepared outside the measured scope, so extra blocks
// expose repeated allocation without interference from other tests.
thread_local! {
    static COUNT_ALLOCATIONS: Cell<bool> = const { Cell::new(false) };
    static BLOCK_ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
    static HEAP_REQUESTS: Cell<usize> = const { Cell::new(0) };
}

struct CountingAllocator;

fn record_allocation(size: usize) {
    let _ = COUNT_ALLOCATIONS.try_with(|enabled| {
        if enabled.get() {
            let _ = HEAP_REQUESTS.try_with(|count| count.set(count.get() + 1));
            if size == BLOCK_SIZE {
                let _ = BLOCK_ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
            }
        }
    });
}

// SAFETY: All allocation operations forward the original pointer and layout
// unchanged to System; bookkeeping neither allocates nor accesses their memory.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation(layout.size());
        // SAFETY: The caller supplies the layout required by GlobalAlloc.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation(layout.size());
        // SAFETY: The caller supplies the layout required by GlobalAlloc.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: Allocations originate from System and keep their layout.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_allocation(new_size);
        // SAFETY: The caller supplies a live allocation and valid new size.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn count_allocations(operation: impl FnOnce()) -> (usize, usize) {
    struct StopCounting;
    impl Drop for StopCounting {
        fn drop(&mut self) {
            COUNT_ALLOCATIONS.with(|enabled| enabled.set(false));
        }
    }

    BLOCK_ALLOCATIONS.with(|count| count.set(0));
    HEAP_REQUESTS.with(|count| count.set(0));
    COUNT_ALLOCATIONS.with(|enabled| enabled.set(true));
    let guard = StopCounting;
    operation();
    drop(guard);
    (
        BLOCK_ALLOCATIONS.with(Cell::get),
        HEAP_REQUESTS.with(Cell::get),
    )
}

fn count_block_allocations(operation: impl FnOnce()) -> usize {
    count_allocations(operation).0
}

fn options(mode: StreamIoMode) -> StreamOptions {
    StreamOptions::new()
        .with_block_size(BLOCK_SIZE)
        .with_io_mode(mode)
}

fn encoded_shards(codec: &ReedSolomon, len: usize) -> Vec<Vec<u8>> {
    let mut shards: Vec<Vec<u8>> = (0..codec.total_shard_count())
        .map(|shard| {
            (0..len)
                .map(|offset| {
                    (offset.wrapping_mul(73) ^ (offset / 251) ^ shard.wrapping_mul(137)) as u8
                })
                .collect()
        })
        .collect();
    codec.encode(&mut shards).unwrap();
    shards
}

struct FragmentedReader {
    data: Cursor<Vec<u8>>,
    interrupt_next: bool,
    fail_after: Option<u64>,
}

impl Read for FragmentedReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.interrupt_next {
            self.interrupt_next = false;
            return Err(std::io::ErrorKind::Interrupted.into());
        }
        if self
            .fail_after
            .is_some_and(|limit| self.data.position() >= limit)
        {
            return Err(std::io::Error::other("injected read error"));
        }
        self.interrupt_next = true;
        let chunk = buffer.len().min(31);
        self.data.read(&mut buffer[..chunk])
    }
}

fn fragmented_readers(shards: &[Vec<u8>]) -> Vec<FragmentedReader> {
    shards
        .iter()
        .cloned()
        .map(|data| FragmentedReader {
            data: Cursor::new(data),
            interrupt_next: true,
            fail_after: None,
        })
        .collect()
}

#[test]
fn streaming_encode_and_verify_match_whole_shards_with_partial_tail() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let shards = encoded_shards(&codec, BLOCK_SIZE * 5 + 37);

    for mode in [StreamIoMode::Serial, StreamIoMode::Parallel] {
        let mut readers: Vec<&[u8]> = shards[..4].iter().map(Vec::as_slice).collect();
        let mut parity = vec![Vec::new(); 2];
        codec
            .encode_stream(&mut readers, &mut parity, &options(mode))
            .unwrap();
        assert_eq!(parity, shards[4..]);

        let mut readers: Vec<&[u8]> = shards.iter().map(Vec::as_slice).collect();
        assert!(codec.verify_stream(&mut readers, &options(mode)).unwrap());

        let mut corrupted = shards.clone();
        *corrupted[5].last_mut().unwrap() ^= 1;
        let mut readers: Vec<&[u8]> = corrupted.iter().map(Vec::as_slice).collect();
        assert!(!codec.verify_stream(&mut readers, &options(mode)).unwrap());
    }
}

#[test]
fn streaming_reconstruct_reuses_missing_buffers_across_blocks_and_tail() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let len = BLOCK_SIZE * 5 + 37;
    let expected = encoded_shards(&codec, len);

    for mode in [StreamIoMode::Serial, StreamIoMode::Parallel] {
        for missing in [vec![1, 4], vec![0, 3], vec![4, 5], vec![2], vec![]] {
            let mut shards: Vec<_> = expected
                .iter()
                .enumerate()
                .map(|(index, data)| {
                    let mut cursor = Cursor::new(if missing.contains(&index) {
                        Vec::with_capacity(len)
                    } else {
                        data.clone()
                    });
                    cursor.set_position(17);
                    cursor
                })
                .collect();

            codec
                .reconstruct_stream(&mut shards, &options(mode))
                .unwrap();

            for (index, shard) in shards.iter().enumerate() {
                assert_eq!(shard.get_ref(), &expected[index]);
                // Recovered bytes extend the buffer without moving the missing
                // cursor; present cursors are rewound and consumed in full.
                let position = if missing.contains(&index) {
                    17
                } else {
                    len as u64
                };
                assert_eq!(shard.position(), position);
            }
        }
    }
}

#[test]
fn stream_views_support_more_than_inline_shard_capacity() {
    for (data, parity) in [(33, 2), (2, 33), (33, 33)] {
        let codec = ReedSolomon::new(data, parity).unwrap();
        let mut expected = encoded_shards(&codec, BLOCK_SIZE * 2 + 13);
        // Exercise an input that ends early within a block and remains at EOF
        // while other streams continue through a partial final block.
        let short_len = BLOCK_SIZE + 7;
        expected[0][short_len..].fill(0);
        codec.encode(&mut expected).unwrap();
        for mode in [StreamIoMode::Serial, StreamIoMode::Parallel] {
            let opts = options(mode);
            let mut readers: Vec<_> = expected[..data].iter().map(Vec::as_slice).collect();
            readers[0] = &readers[0][..short_len];
            let mut outputs = vec![Vec::new(); parity];
            codec
                .encode_stream(&mut readers, &mut outputs, &opts)
                .unwrap();
            assert_eq!(outputs, expected[data..]);

            let mut shards: Vec<_> = expected.iter().cloned().map(Cursor::new).collect();
            shards[data - 1] = Cursor::new(Vec::new());
            shards[data + parity - 1] = Cursor::new(Vec::new());
            codec.reconstruct_stream(&mut shards, &opts).unwrap();
            assert!(shards.iter().zip(&expected).all(|(a, b)| a.get_ref() == b));
            let mut readers: Vec<_> = shards
                .iter()
                .map(|shard| shard.get_ref().as_slice())
                .collect();
            assert!(codec.verify_stream(&mut readers, &opts).unwrap());
        }
    }
}

#[test]
fn verify_rejects_zero_filled_truncated_shard_in_later_block() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let len = BLOCK_SIZE * 2 + 37;
    let shards = vec![vec![0; len]; 6];
    for mode in [StreamIoMode::Serial, StreamIoMode::Parallel] {
        for short_len in [BLOCK_SIZE, len - 1] {
            let mut readers: Vec<&[u8]> = shards.iter().map(Vec::as_slice).collect();
            readers[3] = &readers[3][..short_len];
            let err = codec
                .verify_stream(&mut readers, &options(mode))
                .unwrap_err();
            assert_eq!(err.shard_index, 3);
            assert!(matches!(
                err.kind,
                StreamErrorKind::Codec(Error::IncorrectShardSize)
            ));
        }
    }
}

#[test]
fn encode_still_zero_pads_shorter_data_streams() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let len = BLOCK_SIZE * 2 + 37;
    let mut expected = encoded_shards(&codec, len);
    expected[1][BLOCK_SIZE + 11..].fill(0);
    codec.encode(&mut expected).unwrap();

    for mode in [StreamIoMode::Serial, StreamIoMode::Parallel] {
        let mut readers: Vec<&[u8]> = expected[..4].iter().map(Vec::as_slice).collect();
        readers[1] = &readers[1][..BLOCK_SIZE + 11];
        let mut parity = vec![Vec::new(); 2];
        codec
            .encode_stream(&mut readers, &mut parity, &options(mode))
            .unwrap();
        assert_eq!(parity, expected[4..]);
    }
}

#[test]
fn stream_reads_retry_interruptions_and_fill_short_reads() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let expected = encoded_shards(&codec, BLOCK_SIZE * 2 + 37);

    for mode in [StreamIoMode::Serial, StreamIoMode::Parallel] {
        let mut readers = fragmented_readers(&expected[..4]);
        let mut parity = vec![Vec::new(); 2];
        codec
            .encode_stream(&mut readers, &mut parity, &options(mode))
            .unwrap();
        assert_eq!(parity, expected[4..]);

        let mut readers = fragmented_readers(&expected);
        assert!(codec.verify_stream(&mut readers, &options(mode)).unwrap());
    }
}

#[test]
fn stream_read_errors_keep_the_failing_shard_index() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let expected = encoded_shards(&codec, BLOCK_SIZE * 2 + 37);

    for mode in [StreamIoMode::Serial, StreamIoMode::Parallel] {
        let mut readers = fragmented_readers(&expected[..4]);
        readers[2].fail_after = Some(BLOCK_SIZE as u64 + 31);
        let mut parity = vec![Vec::new(); 2];
        let err = codec
            .encode_stream(&mut readers, &mut parity, &options(mode))
            .unwrap_err();
        assert_eq!(err.shard_index, 2);
        assert!(matches!(err.kind, StreamErrorKind::Read(_)));

        let mut readers = fragmented_readers(&expected);
        readers[5].fail_after = Some(BLOCK_SIZE as u64 + 31);
        let err = codec
            .verify_stream(&mut readers, &options(mode))
            .unwrap_err();
        assert_eq!(err.shard_index, 5);
        assert!(matches!(err.kind, StreamErrorKind::Read(_)));
    }
}

#[test]
fn verify_scratch_allocations_do_not_grow_with_block_count() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let opts = options(StreamIoMode::Serial);
    let measure = |len| {
        let shards = encoded_shards(&codec, len);
        let mut readers: Vec<&[u8]> = shards.iter().map(Vec::as_slice).collect();
        count_block_allocations(|| {
            assert!(codec.verify_stream(&mut readers, &opts).unwrap());
        })
    };

    assert_eq!(measure(BLOCK_SIZE), measure(BLOCK_SIZE * 5));
}

#[test]
fn reconstruction_scratch_allocations_do_not_grow_with_block_count() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let opts = options(StreamIoMode::Serial);
    let measure = |len| {
        let expected = encoded_shards(&codec, len);
        let mut shards: Vec<_> = expected.iter().cloned().map(Cursor::new).collect();
        shards[1] = Cursor::new(Vec::with_capacity(len));
        shards[4] = Cursor::new(Vec::with_capacity(len));
        let count = count_block_allocations(|| {
            codec.reconstruct_stream(&mut shards, &opts).unwrap();
        });
        assert!(shards.iter().zip(&expected).all(|(a, b)| a.get_ref() == b));
        count
    };

    // Warm the decode cache before comparing the block buffer lifetimes.
    measure(BLOCK_SIZE);
    assert_eq!(measure(BLOCK_SIZE), measure(BLOCK_SIZE * 5));
}

#[test]
fn stream_encode_avoids_per_block_reference_allocations() {
    let opts = options(StreamIoMode::Serial);
    let mut counts = Vec::new();
    for (data, parity) in [(4, 2), (33, 2), (2, 33), (33, 33)] {
        let codec = ReedSolomon::new(data, parity).unwrap();
        let measure = |blocks| {
            let len = BLOCK_SIZE * blocks;
            let expected = encoded_shards(&codec, len);
            let mut readers: Vec<_> = expected[..data].iter().map(Vec::as_slice).collect();
            let mut writers: Vec<_> = (0..parity).map(|_| Vec::with_capacity(len)).collect();
            let (_, requests) = count_allocations(|| {
                codec
                    .encode_stream(&mut readers, &mut writers, &opts)
                    .unwrap();
            });
            assert_eq!(writers, expected[data..]);
            requests
        };
        measure(1); // Warm dispatch and hardware discovery outside the comparison.
        let single = measure(1);
        let five = measure(5);
        // The existing generic encoder still collects parity coefficient rows
        // above 32 outputs. This budget permits those required kernel views,
        // but never another data/parity reference array for each stream block.
        let coefficient_requests = if parity > 32 { 4 } else { 0 };
        counts.push((data, parity, single, five, coefficient_requests));
    }
    eprintln!("(data, parity, one block, five blocks, permitted coefficient growth): {counts:?}");
    assert!(
        counts
            .iter()
            .all(|&(_, _, single, five, growth)| five <= single + growth),
        "{counts:?}"
    );
}

#[test]
fn stream_verify_avoids_per_block_reference_allocations() {
    let opts = options(StreamIoMode::Serial);
    let mut counts = Vec::new();
    for (data, parity) in [(4, 2), (33, 2), (2, 33), (33, 33)] {
        let codec = ReedSolomon::new(data, parity).unwrap();
        let measure = |blocks| {
            let expected = encoded_shards(&codec, BLOCK_SIZE * blocks);
            let mut readers: Vec<_> = expected.iter().map(Vec::as_slice).collect();
            count_allocations(|| {
                assert!(codec.verify_stream(&mut readers, &opts).unwrap());
            })
            .1
        };
        measure(1);
        let single = measure(1);
        let five = measure(5);
        let coefficient_requests = if parity > 32 { 4 } else { 0 };
        counts.push((data, parity, single, five, coefficient_requests));
    }
    eprintln!("(data, parity, one block, five blocks, permitted coefficient growth): {counts:?}");
    assert!(
        counts
            .iter()
            .all(|&(_, _, single, five, growth)| five <= single + growth),
        "{counts:?}"
    );
}
