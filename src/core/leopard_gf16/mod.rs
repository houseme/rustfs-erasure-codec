extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use spin::Once;

use crate::errors::Error;

pub(crate) mod decode;
mod driver;
pub(crate) mod encode;
mod fwht_simd;
mod mul_simd;
pub(crate) mod ops;
pub(crate) mod tables;
#[cfg(test)]
mod tests;
pub(crate) mod work;

pub(crate) const BITWIDTH16: usize = 16;
pub(crate) const ORDER16: usize = 1 << BITWIDTH16;
pub(crate) const MODULUS16: usize = ORDER16 - 1;
pub(crate) const POLYNOMIAL16: u32 = 0x1002D;
pub(crate) const WORK_SIZE16: usize = 32 << 10;

#[derive(Debug)]
pub(crate) struct LeopardGf16Tables {
    pub(crate) log_lut: Box<[u16; ORDER16]>,
    pub(crate) exp_lut: Box<[u16; ORDER16 * 2]>,
    pub(crate) fft_skew: Box<[u16; MODULUS16]>,
    pub(crate) log_walsh: Box<[u16; ORDER16]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LeopardGf16EncodeDriver {
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
    log_m01: u16,
    log_m23: u16,
    log_m02: u16,
}

#[derive(Debug, Clone, Copy)]
struct Stage2Block {
    r: usize,
    dist: usize,
    log_m: u16,
}

#[derive(Debug, Clone)]
pub(crate) struct FftDit16Plan {
    mtrunc: usize,
    stage4_blocks: Vec<Stage4Block>,
    final_stage: Vec<Stage2Block>,
}

#[derive(Debug, Clone)]
pub(crate) struct IfftDit16Plan {
    mtrunc: usize,
    m: usize,
    initial_blocks: Vec<Stage4Block>,
    later_blocks: Vec<Stage4Block>,
    clear_start: usize,
    final_stage: Option<Stage2Block>,
}

static TABLES16: Once<LeopardGf16Tables> = Once::new();

pub(crate) fn init_leopard_gf16_tables() -> &'static LeopardGf16Tables {
    TABLES16.call_once(tables::build_tables16)
}

pub(crate) fn build_leopard_gf16_encode_driver(
    data_shards: usize,
    parity_shards: usize,
    shard_size: usize,
) -> Result<LeopardGf16EncodeDriver, Error> {
    driver::build_leopard_gf16_encode_driver(data_shards, parity_shards, shard_size)
}

fn build_fft_dit16_plan(mtrunc: usize, m: usize, skew_lut: &[u16; MODULUS16]) -> FftDit16Plan {
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

    FftDit16Plan {
        mtrunc,
        stage4_blocks,
        final_stage,
    }
}

/// Build FFT plan for the decode path.
///
/// Matches Go's `fftDIT` loop structure exactly:
///   dist4 = n; dist = n >> 2;
///   for dist != 0:
///     for r = 0; r < mtrunc; r += dist4:
///       for i = r; i < r+dist; i++:
///         stage4 block at (i, dist)
///     dist4 = dist; dist >>= 2
///   final stage: for r = 0; r < mtrunc; r += 2:
///     stage2 block at (r, 1)
pub(crate) fn build_fft_decode_dit16_plan(
    mtrunc: usize,
    n: usize,
    skew_lut: &[u16; MODULUS16],
) -> FftDit16Plan {
    // Decode follows Go's `fftDIT(work, outputCount, n, fftSkew[:], ...)`.
    // Unlike the encoder path, which passes `fftSkew[m - 1..]`, decode uses
    // the full skew table with no base offset. Stage4 therefore reads
    // `skew_lut[i_end - 1]` and the final stage reads `skew_lut[r]` directly.

    let mut stage4_blocks = Vec::new();
    let mut dist4 = n;
    let mut dist = n >> 2;
    while dist != 0 {
        let mut r = 0usize;
        while r < mtrunc {
            let i_end = r + dist;
            for i in r..i_end {
                stage4_blocks.push(Stage4Block {
                    r: i,
                    dist,
                    log_m01: skew_lut[i_end - 1],
                    log_m02: skew_lut[i_end + dist - 1],
                    log_m23: skew_lut[i_end + dist * 2 - 1],
                });
            }
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

    FftDit16Plan {
        mtrunc,
        stage4_blocks,
        final_stage,
    }
}

fn build_ifft_dit16_plan(mtrunc: usize, m: usize, skew_lut: &[u16]) -> IfftDit16Plan {
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

    IfftDit16Plan {
        mtrunc,
        m,
        initial_blocks,
        later_blocks,
        clear_start: (mtrunc + 3) & !3usize,
        final_stage,
    }
}

fn build_ifft_decode_dit16_plan(
    mtrunc: usize,
    m: usize,
    skew_lut: &[u16; MODULUS16],
) -> IfftDit16Plan {
    // Go: ifftDITDecoder(mtrunc, work, n, fftSkew[:], &r.o)
    // skewLUT = fftSkew[:] (full table, no base offset)
    // skewLUT[iend-1] = fftSkew[iend-1]
    // Final stage: skewLUT[dist-1] = fftSkew[dist-1]
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

    IfftDit16Plan {
        mtrunc,
        m,
        initial_blocks,
        later_blocks,
        clear_start: (mtrunc + 3) & !3usize,
        final_stage,
    }
}

fn ceil_pow2(n: usize) -> usize {
    n.next_power_of_two()
}
