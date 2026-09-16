extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use spin::Once;

use crate::errors::Error;
#[cfg(feature = "std")]
use std::sync::atomic::AtomicUsize;
#[cfg(feature = "std")]
use std::sync::atomic::Ordering;

pub(crate) mod decode;
mod driver;
mod encode;
mod ops;
mod tables;
#[cfg(test)]
mod tests;
mod work;

const BITWIDTH8: usize = 8;
const ORDER8: usize = 1 << BITWIDTH8;
const MODULUS8: usize = ORDER8 - 1;
const POLYNOMIAL8: usize = 0x11D;
pub(crate) const WORK_SIZE8: usize = 32 << 10;
const WORK_SIZE8_HIGH_FANOUT: usize = 128 << 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Mul8Lut {
    pub(super) value: [u8; 256],
    /// Pre-split low nibble table: `low[i] = value[i]` for `i in 0..16`.
    /// Used by SIMD nibble-lookup to avoid per-call table construction.
    pub(super) low: [u8; 16],
    /// Pre-split high nibble table: `high[i] = value[i * 16]` for `i in 0..16`.
    /// Used by SIMD nibble-lookup to avoid per-call table construction.
    pub(super) high: [u8; 16],
}

#[derive(Debug)]
pub(crate) struct LeopardGf8Tables {
    pub(crate) fft_skew: Box<[u8; MODULUS8]>,
    pub(crate) log_walsh: Box<[u8; ORDER8]>,
    pub(crate) log_lut: Box<[u8; ORDER8]>,
    pub(crate) exp_lut: Box<[u8; ORDER8]>,
    pub(crate) mul_luts: Box<[Mul8Lut; ORDER8]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LeopardGf8EncodeDriver {
    pub(crate) shard_size: usize,
    pub(crate) m: usize,
    pub(crate) mtrunc: usize,
    pub(crate) last_count: usize,
    pub(crate) chunk_size: usize,
    pub(crate) work_slices: usize,
    pub(crate) skew_offset: usize,
}

#[derive(Debug, Clone, Copy)]
struct Stage4Block {
    r: usize,
    dist: usize,
    log_m01: u8,
    log_m23: u8,
    log_m02: u8,
}

#[derive(Debug, Clone, Copy)]
struct Stage2Block {
    r: usize,
    dist: usize,
    log_m: u8,
}

#[derive(Debug, Clone)]
struct FftDit8Plan {
    mtrunc: usize,
    stage4_blocks: Vec<Stage4Block>,
    final_stage: Vec<Stage2Block>,
}

#[derive(Debug, Clone)]
struct IfftDit8Plan {
    mtrunc: usize,
    m: usize,
    initial_blocks: Vec<Stage4Block>,
    later_blocks: Vec<Stage4Block>,
    clear_start: usize,
    final_stage: Option<Stage2Block>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
enum IfftProfilePhase {
    FirstGroup,
    LaterGroup,
    RemainderGroup,
}

#[cfg(feature = "std")]
#[derive(Debug, Default)]
pub(crate) struct LeopardGf8ProfileMetrics {
    encode_calls: AtomicUsize,
    encode_chunks: AtomicUsize,
    encode_full_groups: AtomicUsize,
    encode_remainder_groups: AtomicUsize,
    encode_later_group_calls: AtomicUsize,
    fft_stage_calls: AtomicUsize,
    ifft_stage_calls: AtomicUsize,
    first_group_ifft_calls: AtomicUsize,
    later_group_ifft_calls: AtomicUsize,
    remainder_group_ifft_calls: AtomicUsize,
    first_group_input_copy_bytes: AtomicUsize,
    later_group_input_copy_bytes: AtomicUsize,
    remainder_group_input_copy_bytes: AtomicUsize,
    first_group_zero_fill_bytes: AtomicUsize,
    later_group_zero_fill_bytes: AtomicUsize,
    remainder_group_zero_fill_bytes: AtomicUsize,
    later_group_xor_bytes: AtomicUsize,
    remainder_group_xor_bytes: AtomicUsize,
    output_writeback_calls: AtomicUsize,
    input_copy_bytes: AtomicUsize,
    zero_fill_bytes: AtomicUsize,
    xor_bytes: AtomicUsize,
    output_writeback_bytes: AtomicUsize,
}

#[cfg(feature = "std")]
impl LeopardGf8ProfileMetrics {
    fn add_ifft_calls(&self, phase: IfftProfilePhase) {
        self.ifft_stage_calls.fetch_add(1, Ordering::Relaxed);
        match phase {
            IfftProfilePhase::FirstGroup => &self.first_group_ifft_calls,
            IfftProfilePhase::LaterGroup => &self.later_group_ifft_calls,
            IfftProfilePhase::RemainderGroup => &self.remainder_group_ifft_calls,
        }
        .fetch_add(1, Ordering::Relaxed);
    }

    fn add_input_copy_bytes(&self, phase: IfftProfilePhase, bytes: usize) {
        self.input_copy_bytes.fetch_add(bytes, Ordering::Relaxed);
        match phase {
            IfftProfilePhase::FirstGroup => &self.first_group_input_copy_bytes,
            IfftProfilePhase::LaterGroup => &self.later_group_input_copy_bytes,
            IfftProfilePhase::RemainderGroup => &self.remainder_group_input_copy_bytes,
        }
        .fetch_add(bytes, Ordering::Relaxed);
    }

    fn add_zero_fill_bytes(&self, phase: IfftProfilePhase, bytes: usize) {
        self.zero_fill_bytes.fetch_add(bytes, Ordering::Relaxed);
        match phase {
            IfftProfilePhase::FirstGroup => &self.first_group_zero_fill_bytes,
            IfftProfilePhase::LaterGroup => &self.later_group_zero_fill_bytes,
            IfftProfilePhase::RemainderGroup => &self.remainder_group_zero_fill_bytes,
        }
        .fetch_add(bytes, Ordering::Relaxed);
    }

    fn add_xor_bytes(&self, phase: IfftProfilePhase, bytes: usize) {
        self.xor_bytes.fetch_add(bytes, Ordering::Relaxed);
        match phase {
            IfftProfilePhase::LaterGroup => &self.later_group_xor_bytes,
            IfftProfilePhase::RemainderGroup => &self.remainder_group_xor_bytes,
            IfftProfilePhase::FirstGroup => return,
        }
        .fetch_add(bytes, Ordering::Relaxed);
    }

    fn add_output_writeback(&self, bytes: usize) {
        self.output_writeback_calls.fetch_add(1, Ordering::Relaxed);
        self.output_writeback_bytes
            .fetch_add(bytes, Ordering::Relaxed);
    }
}

#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeopardGf8ProfileStats {
    pub encode_calls: usize,
    pub encode_chunks: usize,
    pub encode_full_groups: usize,
    pub encode_remainder_groups: usize,
    pub encode_later_group_calls: usize,
    pub fft_stage_calls: usize,
    pub ifft_stage_calls: usize,
    pub first_group_ifft_calls: usize,
    pub later_group_ifft_calls: usize,
    pub remainder_group_ifft_calls: usize,
    pub first_group_input_copy_bytes: usize,
    pub later_group_input_copy_bytes: usize,
    pub remainder_group_input_copy_bytes: usize,
    pub first_group_zero_fill_bytes: usize,
    pub later_group_zero_fill_bytes: usize,
    pub remainder_group_zero_fill_bytes: usize,
    pub later_group_xor_bytes: usize,
    pub remainder_group_xor_bytes: usize,
    pub output_writeback_calls: usize,
    pub input_copy_bytes: usize,
    pub zero_fill_bytes: usize,
    pub xor_bytes: usize,
    pub output_writeback_bytes: usize,
}

static TABLES8: Once<LeopardGf8Tables> = Once::new();

#[cfg(feature = "std")]
static PROFILE8: LeopardGf8ProfileMetrics = LeopardGf8ProfileMetrics {
    encode_calls: AtomicUsize::new(0),
    encode_chunks: AtomicUsize::new(0),
    encode_full_groups: AtomicUsize::new(0),
    encode_remainder_groups: AtomicUsize::new(0),
    encode_later_group_calls: AtomicUsize::new(0),
    fft_stage_calls: AtomicUsize::new(0),
    ifft_stage_calls: AtomicUsize::new(0),
    first_group_ifft_calls: AtomicUsize::new(0),
    later_group_ifft_calls: AtomicUsize::new(0),
    remainder_group_ifft_calls: AtomicUsize::new(0),
    first_group_input_copy_bytes: AtomicUsize::new(0),
    later_group_input_copy_bytes: AtomicUsize::new(0),
    remainder_group_input_copy_bytes: AtomicUsize::new(0),
    first_group_zero_fill_bytes: AtomicUsize::new(0),
    later_group_zero_fill_bytes: AtomicUsize::new(0),
    remainder_group_zero_fill_bytes: AtomicUsize::new(0),
    later_group_xor_bytes: AtomicUsize::new(0),
    remainder_group_xor_bytes: AtomicUsize::new(0),
    output_writeback_calls: AtomicUsize::new(0),
    input_copy_bytes: AtomicUsize::new(0),
    zero_fill_bytes: AtomicUsize::new(0),
    xor_bytes: AtomicUsize::new(0),
    output_writeback_bytes: AtomicUsize::new(0),
};

pub(crate) fn init_leopard_gf8_tables() -> &'static LeopardGf8Tables {
    TABLES8.call_once(tables::build_tables8)
}

#[cfg(feature = "std")]
pub(crate) fn leopard_gf8_profile_stats() -> LeopardGf8ProfileStats {
    LeopardGf8ProfileStats {
        encode_calls: PROFILE8.encode_calls.load(Ordering::Relaxed),
        encode_chunks: PROFILE8.encode_chunks.load(Ordering::Relaxed),
        encode_full_groups: PROFILE8.encode_full_groups.load(Ordering::Relaxed),
        encode_remainder_groups: PROFILE8.encode_remainder_groups.load(Ordering::Relaxed),
        encode_later_group_calls: PROFILE8.encode_later_group_calls.load(Ordering::Relaxed),
        fft_stage_calls: PROFILE8.fft_stage_calls.load(Ordering::Relaxed),
        ifft_stage_calls: PROFILE8.ifft_stage_calls.load(Ordering::Relaxed),
        first_group_ifft_calls: PROFILE8.first_group_ifft_calls.load(Ordering::Relaxed),
        later_group_ifft_calls: PROFILE8.later_group_ifft_calls.load(Ordering::Relaxed),
        remainder_group_ifft_calls: PROFILE8.remainder_group_ifft_calls.load(Ordering::Relaxed),
        first_group_input_copy_bytes: PROFILE8
            .first_group_input_copy_bytes
            .load(Ordering::Relaxed),
        later_group_input_copy_bytes: PROFILE8
            .later_group_input_copy_bytes
            .load(Ordering::Relaxed),
        remainder_group_input_copy_bytes: PROFILE8
            .remainder_group_input_copy_bytes
            .load(Ordering::Relaxed),
        first_group_zero_fill_bytes: PROFILE8.first_group_zero_fill_bytes.load(Ordering::Relaxed),
        later_group_zero_fill_bytes: PROFILE8.later_group_zero_fill_bytes.load(Ordering::Relaxed),
        remainder_group_zero_fill_bytes: PROFILE8
            .remainder_group_zero_fill_bytes
            .load(Ordering::Relaxed),
        later_group_xor_bytes: PROFILE8.later_group_xor_bytes.load(Ordering::Relaxed),
        remainder_group_xor_bytes: PROFILE8.remainder_group_xor_bytes.load(Ordering::Relaxed),
        output_writeback_calls: PROFILE8.output_writeback_calls.load(Ordering::Relaxed),
        input_copy_bytes: PROFILE8.input_copy_bytes.load(Ordering::Relaxed),
        zero_fill_bytes: PROFILE8.zero_fill_bytes.load(Ordering::Relaxed),
        xor_bytes: PROFILE8.xor_bytes.load(Ordering::Relaxed),
        output_writeback_bytes: PROFILE8.output_writeback_bytes.load(Ordering::Relaxed),
    }
}

#[cfg(feature = "std")]
pub(crate) fn reset_leopard_gf8_profile_stats() {
    PROFILE8.encode_calls.store(0, Ordering::Relaxed);
    PROFILE8.encode_chunks.store(0, Ordering::Relaxed);
    PROFILE8.encode_full_groups.store(0, Ordering::Relaxed);
    PROFILE8.encode_remainder_groups.store(0, Ordering::Relaxed);
    PROFILE8
        .encode_later_group_calls
        .store(0, Ordering::Relaxed);
    PROFILE8.fft_stage_calls.store(0, Ordering::Relaxed);
    PROFILE8.ifft_stage_calls.store(0, Ordering::Relaxed);
    PROFILE8.first_group_ifft_calls.store(0, Ordering::Relaxed);
    PROFILE8.later_group_ifft_calls.store(0, Ordering::Relaxed);
    PROFILE8
        .remainder_group_ifft_calls
        .store(0, Ordering::Relaxed);
    PROFILE8
        .first_group_input_copy_bytes
        .store(0, Ordering::Relaxed);
    PROFILE8
        .later_group_input_copy_bytes
        .store(0, Ordering::Relaxed);
    PROFILE8
        .remainder_group_input_copy_bytes
        .store(0, Ordering::Relaxed);
    PROFILE8
        .first_group_zero_fill_bytes
        .store(0, Ordering::Relaxed);
    PROFILE8
        .later_group_zero_fill_bytes
        .store(0, Ordering::Relaxed);
    PROFILE8
        .remainder_group_zero_fill_bytes
        .store(0, Ordering::Relaxed);
    PROFILE8.later_group_xor_bytes.store(0, Ordering::Relaxed);
    PROFILE8
        .remainder_group_xor_bytes
        .store(0, Ordering::Relaxed);
    PROFILE8.output_writeback_calls.store(0, Ordering::Relaxed);
    PROFILE8.input_copy_bytes.store(0, Ordering::Relaxed);
    PROFILE8.zero_fill_bytes.store(0, Ordering::Relaxed);
    PROFILE8.xor_bytes.store(0, Ordering::Relaxed);
    PROFILE8.output_writeback_bytes.store(0, Ordering::Relaxed);
}

pub(crate) fn build_leopard_gf8_encode_driver(
    data_shards: usize,
    parity_shards: usize,
    shard_size: usize,
) -> Result<LeopardGf8EncodeDriver, Error> {
    driver::build_leopard_gf8_encode_driver(data_shards, parity_shards, shard_size)
}

fn build_fft_dit8_plan(mtrunc: usize, m: usize, skew_lut: &[u8; MODULUS8]) -> FftDit8Plan {
    let mut stage4_blocks = Vec::new();
    let mut dist4 = m;
    let mut dist = m >> 2;
    while dist != 0 {
        let mut r = 0usize;
        while r < mtrunc {
            let i_end = r + dist;
            stage4_blocks.push(Stage4Block {
                r,
                dist,
                log_m01: skew_lut[i_end - 1],
                log_m02: skew_lut[i_end + dist - 1],
                log_m23: skew_lut[i_end + dist * 2 - 1],
            });
            r += dist4;
        }
        dist4 = dist;
        dist >>= 2;
    }

    let final_stage = if dist4 == 2 {
        let mut blocks = Vec::new();
        let mut r = 0usize;
        while r < mtrunc {
            blocks.push(Stage2Block {
                r,
                dist: 1,
                log_m: skew_lut[r],
            });
            r += 2;
        }
        blocks
    } else {
        Vec::new()
    };

    FftDit8Plan {
        mtrunc,
        stage4_blocks,
        final_stage,
    }
}

fn build_ifft_dit8_plan(mtrunc: usize, m: usize, skew_lut: &[u8]) -> IfftDit8Plan {
    let mut initial_blocks = Vec::new();
    let mut later_blocks = Vec::new();
    let mut dist = 1usize;
    let mut dist4 = 4usize;

    if dist4 <= m {
        let full_groups = mtrunc & !3usize;
        let mut r = 0usize;
        while r < full_groups {
            let i_end = r + dist;
            initial_blocks.push(Stage4Block {
                r,
                dist,
                log_m01: skew_lut[i_end],
                log_m02: skew_lut[i_end + dist],
                log_m23: skew_lut[i_end + dist * 2],
            });
            r += dist4;
        }

        if full_groups < mtrunc {
            let r = full_groups;
            let i_end = r + dist;
            initial_blocks.push(Stage4Block {
                r,
                dist,
                log_m01: skew_lut[i_end],
                log_m02: skew_lut[i_end + dist],
                log_m23: skew_lut[i_end + dist * 2],
            });
        }

        dist = dist4;
        dist4 <<= 2;
        while dist4 <= m {
            let mut r = 0usize;
            while r < mtrunc {
                let i_end = r + dist;
                later_blocks.push(Stage4Block {
                    r,
                    dist,
                    log_m01: skew_lut[i_end],
                    log_m02: skew_lut[i_end + dist],
                    log_m23: skew_lut[i_end + dist * 2],
                });
                r += dist4;
            }
            dist = dist4;
            dist4 <<= 2;
        }
    }

    let final_stage = if dist < m {
        Some(Stage2Block {
            r: 0,
            dist,
            log_m: skew_lut[dist],
        })
    } else {
        None
    };

    IfftDit8Plan {
        mtrunc,
        m,
        initial_blocks,
        later_blocks,
        clear_start: (mtrunc + 3) & !3usize,
        final_stage,
    }
}

/// IFFT plan builder for the decode path.
///
/// The decode path passes the full `fft_skew` array (no offset), so skew
/// indices use `i_end - 1` style indexing (matching the FFT plan builder).
/// The encoder uses `build_ifft_dit8_plan` with `fft_skew[m-1..]`, which
/// effectively achieves the same result with `i_end` indexing.
fn build_ifft_decode_dit8_plan(mtrunc: usize, m: usize, skew_lut: &[u8; MODULUS8]) -> IfftDit8Plan {
    let mut initial_blocks = Vec::new();
    let mut later_blocks = Vec::new();
    let mut dist = 1usize;
    let mut dist4 = 4usize;

    if dist4 <= m {
        let full_groups = mtrunc & !3usize;
        let mut r = 0usize;
        while r < full_groups {
            let i_end = r + dist;
            initial_blocks.push(Stage4Block {
                r,
                dist,
                log_m01: skew_lut[i_end - 1],
                log_m02: skew_lut[i_end + dist - 1],
                log_m23: skew_lut[i_end + dist * 2 - 1],
            });
            r += dist4;
        }

        if full_groups < mtrunc {
            let r = full_groups;
            let i_end = r + dist;
            initial_blocks.push(Stage4Block {
                r,
                dist,
                log_m01: skew_lut[i_end - 1],
                log_m02: skew_lut[i_end + dist - 1],
                log_m23: skew_lut[i_end + dist * 2 - 1],
            });
        }

        dist = dist4;
        dist4 <<= 2;
        while dist4 <= m {
            let mut r = 0usize;
            while r < mtrunc {
                let i_end = r + dist;
                later_blocks.push(Stage4Block {
                    r,
                    dist,
                    log_m01: skew_lut[i_end - 1],
                    log_m02: skew_lut[i_end + dist - 1],
                    log_m23: skew_lut[i_end + dist * 2 - 1],
                });
                r += dist4;
            }
            dist = dist4;
            dist4 <<= 2;
        }
    }

    let final_stage = if dist < m {
        Some(Stage2Block {
            r: 0,
            dist,
            log_m: skew_lut[dist - 1],
        })
    } else {
        None
    };

    IfftDit8Plan {
        mtrunc,
        m,
        initial_blocks,
        later_blocks,
        clear_start: (mtrunc + 3) & !3usize,
        final_stage,
    }
}

pub(crate) fn encode_skeleton<T: AsRef<[u8]>, U: AsRef<[u8]> + AsMut<[u8]>>(
    data_shards: usize,
    parity_shards: usize,
    data: &[T],
    parity: &mut [U],
) -> Result<LeopardGf8EncodeDriver, Error> {
    encode::encode_skeleton(data_shards, parity_shards, data, parity)
}

pub(crate) fn encode_with_tables<T: AsRef<[u8]>, U: AsRef<[u8]> + AsMut<[u8]>>(
    data_shards: usize,
    parity_shards: usize,
    data: &[T],
    parity: &mut [U],
) -> Result<LeopardGf8EncodeDriver, Error> {
    encode::encode_with_tables(data_shards, parity_shards, data, parity)
}

pub(crate) fn reconstruct_with_tables(
    present: &[bool],
    outputs: &mut [&mut [u8]],
    input_data: &[Option<&[u8]>],
    data_shards: usize,
    parity_shards: usize,
) -> Result<(), Error> {
    let tables = init_leopard_gf8_tables();
    decode::reconstruct_with_tables(
        present,
        outputs,
        input_data,
        data_shards,
        parity_shards,
        tables,
    )
}

fn ceil_pow2(n: usize) -> usize {
    n.next_power_of_two()
}
