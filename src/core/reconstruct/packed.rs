extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use smallvec::SmallVec;

use super::{LEOPARD_INLINE_RECOVERY_BYTES, ReedSolomon};
use crate::errors::Error;
use crate::{Field, ReconstructShard};

impl<F: Field> ReedSolomon<F> {
    #[inline(never)]
    pub(super) fn reconstruct_leopard_packed<T: ReconstructShard<F>>(
        &self,
        slices: &mut [T],
        required: &impl Fn(usize) -> bool,
        reconstruct_fn: impl FnOnce(
            &[bool],
            &mut [&mut [u8]],
            &[Option<&[u8]>],
            usize,
            usize,
        ) -> Result<(), Error>,
        shard_len: usize,
    ) -> Result<(), Error> {
        // Stage only requested missing shards. The scratch remains private so
        // decoder errors and unwinds cannot modify a caller's missing buffers.
        let mut scratch = [0u8; LEOPARD_INLINE_RECOVERY_BYTES];
        let mut present: SmallVec<[bool; 32]> = SmallVec::new();
        let mut inputs: SmallVec<[Option<&[u8]>; 32]> = SmallVec::new();
        let mut requested: SmallVec<[bool; 32]> = SmallVec::new();
        let mut requested_count = 0usize;
        for (index, shard) in slices.iter_mut().enumerate() {
            let data = shard.get();
            present.push(data.is_some());
            if let Some(data) = data {
                let bytes = F::as_gf8_slice(data).ok_or(Error::UnsupportedCodecFamily)?;
                if data.len() != shard_len || bytes.len() != shard_len {
                    return Err(Error::IncorrectShardSize);
                }
                inputs.push(Some(bytes));
                requested.push(false);
            } else {
                inputs.push(None);
                let is_required = required(index);
                requested_count += usize::from(is_required);
                requested.push(is_required);
            }
        }

        // A user-defined shard can change its reported state between validation
        // and binding. Preserve that behavior even if more outputs are requested
        // than the fixed scratch can hold.
        let inline_count = LEOPARD_INLINE_RECOVERY_BYTES / shard_len;
        let mut overflow: SmallVec<[Vec<u8>; 4]> = SmallVec::new();
        overflow.resize_with(requested_count.saturating_sub(inline_count), || {
            vec![0; shard_len]
        });
        let mut staged = scratch.chunks_exact_mut(shard_len);
        let mut extra = overflow.iter_mut();
        let mut outputs: SmallVec<[&mut [u8]; 32]> = SmallVec::new();
        for &is_required in &requested {
            if !is_required {
                outputs.push(&mut []);
            } else if let Some(bytes) = staged.next() {
                outputs.push(bytes);
            } else {
                outputs.push(extra.next().expect("overflow buffers cover outputs"));
            }
        }
        drop(staged);
        drop(extra);
        reconstruct_fn(
            &present,
            &mut outputs,
            &inputs,
            self.data_shard_count(),
            self.parity_shard_count(),
        )?;
        drop(outputs);
        drop(inputs);

        // Publish from the original staging storage even if the decoder
        // reborrowed an output reference, and retain sequential error behavior.
        let mut staged = scratch.chunks_exact(shard_len);
        let mut extra = overflow.iter();
        for (shard, is_required) in slices.iter_mut().zip(requested) {
            if !is_required {
                continue;
            }
            let bytes = staged.next().unwrap_or_else(|| {
                extra
                    .next()
                    .expect("overflow buffers cover outputs")
                    .as_slice()
            });
            publish_leopard_staged_shard!(F, shard, bytes, shard_len);
        }
        Ok(())
    }
}
