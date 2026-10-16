//! Selection and checked execution of generated native GF8 kernels.

use crate::Error;

type EncodeFn = fn(&[&[u8]], &[&[u8]], &mut [&mut [u8]], usize);

/// A supported shape and an ISA-checked callback. The callback stays private so
/// generated unsafe kernels can only be invoked through the checked boundary.
#[derive(Clone, Copy)]
pub(crate) struct SelectedEncodeKernel {
    data_shards: usize,
    parity_shards: usize,
    encode: EncodeFn,
}

impl SelectedEncodeKernel {
    pub(super) fn new(data_shards: usize, parity_shards: usize, encode: EncodeFn) -> Self {
        Self {
            data_shards,
            parity_shards,
            encode,
        }
    }

    pub(crate) fn select(data_shards: usize, parity_shards: usize) -> Option<Self> {
        #[cfg(rse_x86_avx2)]
        return super::x86::codegen::select_encode_kernel_avx2(data_shards, parity_shards);
        #[cfg(rse_aarch64_neon)]
        return super::aarch64::codegen::select_encode_kernel_neon(data_shards, parity_shards);
        #[cfg(not(any(rse_x86_avx2, rse_aarch64_neon)))]
        {
            let _ = (data_shards, parity_shards);
            None
        }
    }

    /// Validate the complete invocation before the generated kernel can write.
    /// Selection is final: execution never falls back after modifying parity.
    pub(crate) fn execute(
        &self,
        parity_rows: &[&[u8]],
        data: &[&[u8]],
        parity: &mut [&mut [u8]],
        shard_len: usize,
    ) -> Result<(), Error> {
        if data.len() < self.data_shards {
            return Err(Error::TooFewDataShards);
        }
        if data.len() > self.data_shards {
            return Err(Error::TooManyDataShards);
        }
        if parity.len() < self.parity_shards {
            return Err(Error::TooFewParityShards);
        }
        if parity.len() > self.parity_shards {
            return Err(Error::TooManyParityShards);
        }
        if parity_rows.len() != self.parity_shards
            || parity_rows.iter().any(|row| row.len() != self.data_shards)
        {
            return Err(Error::InvalidCustomMatrix);
        }
        if data.iter().any(|shard| shard.len() != shard_len)
            || parity.iter().any(|shard| shard.len() != shard_len)
        {
            return Err(Error::IncorrectShardSize);
        }
        (self.encode)(parity_rows, data, parity, shard_len);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_kernel_checks_all_views_before_writing() {
        let kernel = SelectedEncodeKernel::new(2, 1, |_, _, parity, _| parity[0].fill(0));
        let row = [1, 1];
        let input = [1; 4];
        let short = [1; 3];
        let mut parity = [0xa5; 4];
        assert_eq!(
            kernel.execute(&[&row], &[&input, &short], &mut [&mut parity], 4),
            Err(Error::IncorrectShardSize)
        );
        assert_eq!(parity, [0xa5; 4]);
        assert_eq!(
            kernel.execute(&[&row[..1]], &[&input, &input], &mut [&mut parity], 4),
            Err(Error::InvalidCustomMatrix)
        );
        assert_eq!(parity, [0xa5; 4]);
        assert_eq!(
            kernel.execute(&[&row], &[&input], &mut [&mut parity], 4),
            Err(Error::TooFewDataShards)
        );
        assert_eq!(parity, [0xa5; 4]);
        kernel
            .execute(&[&row], &[&input, &input], &mut [&mut parity], 4)
            .unwrap();
        assert_eq!(parity, [0; 4]);
    }
}
