#[cfg(feature = "std")]
use crate::core::{ClassicReconstructPlan, ReconstructionGoal, RuntimeParallelPolicyCache};

#[cfg(feature = "std")]
const RECONSTRUCT_DATA_MIN_PARALLEL_SHARD_BYTES: usize = 512 * 1024;
#[cfg(feature = "std")]
const RECONSTRUCT_FULL_MIN_PARALLEL_SHARD_BYTES: usize = 256 * 1024;
#[cfg(feature = "std")]
const RS_RECONSTRUCT_DATA_MIN_PARALLEL_SHARD_BYTES_ENV: &str =
    "RS_RECONSTRUCT_DATA_MIN_PARALLEL_SHARD_BYTES";
#[cfg(feature = "std")]
const RS_RECONSTRUCT_FULL_MIN_PARALLEL_SHARD_BYTES_ENV: &str =
    "RS_RECONSTRUCT_FULL_MIN_PARALLEL_SHARD_BYTES";
#[cfg(feature = "std")]
const RS_RECONSTRUCT_MIN_BYTES_PER_JOB_ENV: &str = "RS_RECONSTRUCT_MIN_BYTES_PER_JOB";
#[cfg(all(feature = "std", target_arch = "aarch64"))]
const RS_AARCH64_RECONSTRUCT_MIN_PARALLEL_SHARD_BYTES_ENV: &str =
    "RS_AARCH64_RECONSTRUCT_MIN_PARALLEL_SHARD_BYTES";
#[cfg(all(feature = "std", target_arch = "aarch64"))]
const RS_AARCH64_RECONSTRUCT_MIN_BYTES_PER_JOB_ENV: &str =
    "RS_AARCH64_RECONSTRUCT_MIN_BYTES_PER_JOB";
#[cfg(all(feature = "std", target_arch = "aarch64"))]
const RS_AARCH64_RECONSTRUCT_MAX_JOBS_ENV: &str = "RS_AARCH64_RECONSTRUCT_MAX_JOBS";
#[cfg(all(feature = "std", target_arch = "aarch64"))]
const RS_AARCH64_RECONSTRUCT_DATA_MIN_BYTES_PER_JOB_ENV: &str =
    "RS_AARCH64_RECONSTRUCT_DATA_MIN_BYTES_PER_JOB";
#[cfg(all(feature = "std", target_arch = "aarch64"))]
const RS_AARCH64_RECONSTRUCT_PARITY_MIN_BYTES_PER_JOB_ENV: &str =
    "RS_AARCH64_RECONSTRUCT_PARITY_MIN_BYTES_PER_JOB";

/// GF8 compatibility name for the generic Classic reconstruction plan.
///
/// Existing option-vector APIs retain their bulk serial executor. The same
/// plan can also be used by the generic `ReconstructShard` workspace API.
#[cfg(feature = "std")]
pub type OptionVecReconstructWorkspace = crate::ReconstructWorkspace<super::Field>;

impl crate::ReedSolomon<super::Field> {
    #[cfg(feature = "std")]
    fn validate_opt_shard_count(&self, count: usize) -> Result<(), crate::Error> {
        if count < self.total_shard_count() {
            return Err(crate::Error::TooFewShards);
        }
        if count > self.total_shard_count() {
            return Err(crate::Error::TooManyShards);
        }
        Ok(())
    }

    #[cfg(feature = "std")]
    fn validate_opt_shards<T: AsRef<[u8]>>(&self, shards: &[T]) -> Result<usize, crate::Error> {
        self.validate_opt_shard_count(shards.len())?;
        let shard_len = shards[0].as_ref().len();
        if shard_len == 0 {
            return Err(crate::Error::EmptyShard);
        }
        if shards.iter().any(|shard| shard.as_ref().len() != shard_len) {
            return Err(crate::Error::IncorrectShardSize);
        }
        Ok(shard_len)
    }

    #[cfg(feature = "std")]
    fn encode_leopard_gf8_opt<T, U>(&self, data: &[T], parity: &mut [U]) -> Result<(), crate::Error>
    where
        T: AsRef<[u8]> + Sync,
        U: AsRef<[u8]> + AsMut<[u8]> + Send,
    {
        crate::core::leopard_gf8::encode_with_tables(
            self.data_shard_count(),
            self.parity_shard_count(),
            data,
            parity,
        )
        .map(|_| ())
    }

    #[cfg(feature = "std")]
    fn decode_idx_accumulate_reduced_outputs(
        &self,
        matrix_rows: &[smallvec::SmallVec<[u8; 32]>],
        inputs: &[&[u8]],
        outputs: &mut [&mut [u8]],
    ) {
        debug_assert!(!outputs.is_empty());

        let shard_len = inputs.first().map(|input| input.len()).unwrap_or(0);
        if shard_len == 0 {
            return;
        }

        let chunk_len = self.code_chunk_len(shard_len);
        if outputs.len() == 1 {
            let matrix_row = matrix_rows[0].as_slice();
            outputs[0]
                .chunks_mut(chunk_len)
                .enumerate()
                .for_each(|(chunk_idx, output_chunk)| {
                    let start = chunk_idx * chunk_len;
                    let end = start + output_chunk.len();
                    for i_input in 0..inputs.len() {
                        super::mul_slice_xor(
                            matrix_row[i_input],
                            &inputs[i_input][start..end],
                            output_chunk,
                        );
                    }
                });
            return;
        }

        if outputs.len() == 2 {
            let (first, second) = outputs.split_at_mut(1);
            let output0 = &mut first[0];
            let output1 = &mut second[0];
            let row0 = matrix_rows[0].as_slice();
            let row1 = matrix_rows[1].as_slice();
            output0
                .chunks_mut(chunk_len)
                .zip(output1.chunks_mut(chunk_len))
                .enumerate()
                .for_each(|(chunk_idx, (output0_chunk, output1_chunk))| {
                    let start = chunk_idx * chunk_len;
                    let end = start + output0_chunk.len();
                    let input0 = &inputs[0][start..end];
                    super::mul_slice_xor(row0[0], input0, output0_chunk);
                    super::mul_slice_xor(row1[0], input0, output1_chunk);
                    for i_input in 1..inputs.len() {
                        let input_chunk = &inputs[i_input][start..end];
                        super::mul_slice_xor(row0[i_input], input_chunk, output0_chunk);
                        super::mul_slice_xor(row1[i_input], input_chunk, output1_chunk);
                    }
                });
            return;
        }

        for (row, output) in matrix_rows.iter().zip(outputs.iter_mut()) {
            output
                .chunks_mut(chunk_len)
                .enumerate()
                .for_each(|(chunk_idx, output_chunk)| {
                    let start = chunk_idx * chunk_len;
                    let end = start + output_chunk.len();
                    for (coefficient, input) in row.iter().copied().zip(inputs.iter().copied()) {
                        super::mul_slice_xor(coefficient, &input[start..end], output_chunk);
                    }
                });
        }
    }

    #[cfg(feature = "std")]
    fn plan_option_vec_reconstruct(
        &self,
        shards: &[Option<Vec<u8>>],
        required: Option<&[bool]>,
    ) -> Result<ClassicReconstructPlan<super::Field>, crate::Error> {
        self.validate_opt_shard_count(shards.len())?;
        if self.is_leopard_family() {
            return Err(crate::Error::UnsupportedCodecFamily);
        }
        if let Some(required) = required
            && required.len() != self.data_shard_count()
            && required.len() != self.total_shard_count()
        {
            return Err(crate::Error::InvalidShardFlags);
        }
        let shape = self
            .classic_reconstruct_shape(shards.iter().map(|shard| shard.as_ref().map(Vec::len)))?;
        self.prepare_classic_reconstruct_plan(
            shape,
            required.map_or(ReconstructionGoal::All, ReconstructionGoal::Required),
        )
    }

    #[cfg(feature = "std")]
    fn first_shard_len<T: AsRef<[u8]>>(slices: &[T]) -> usize {
        slices
            .first()
            .map(|shard| shard.as_ref().len())
            .unwrap_or(0)
    }

    #[cfg(feature = "std")]
    fn first_present_shard_len(shards: &[Option<Vec<u8>>]) -> usize {
        shards
            .iter()
            .find_map(|shard| shard.as_ref().map(|shard| shard.len()))
            .unwrap_or(0)
    }

    #[cfg(feature = "std")]
    pub(crate) fn reconstruct_parallel_decision_with(
        &self,
        shard_len: usize,
        missing_data: usize,
        missing_total: usize,
        data_only: bool,
        available_parallelism: usize,
    ) -> crate::ParallelDecision {
        let output_shards = if data_only {
            missing_data
        } else {
            missing_total
        };
        let tuned = self.policy_cache.reconstruct_policy(data_only);
        tuned.decide(
            shard_len,
            self.data_shard_count(),
            output_shards,
            available_parallelism,
        )
    }

    #[cfg(feature = "std")]
    fn reconstruct_parallel_decision(
        &self,
        shard_len: usize,
        missing_data: usize,
        missing_total: usize,
        data_only: bool,
    ) -> crate::ParallelDecision {
        self.reconstruct_parallel_decision_with(
            shard_len,
            missing_data,
            missing_total,
            data_only,
            self.policy_cache.available_parallelism,
        )
    }

    #[cfg(feature = "std")]
    fn reconstruct_stage_policies(
        &self,
        data_only: bool,
    ) -> (crate::ParallelPolicy, crate::ParallelPolicy) {
        self.policy_cache.reconstruct_stage_policies(data_only)
    }

    #[cfg(feature = "std")]
    #[doc(hidden)]
    pub fn reconstruct_stage_policies_for_bench(
        &self,
        data_only: bool,
    ) -> (crate::ParallelPolicy, crate::ParallelPolicy) {
        self.reconstruct_stage_policies(data_only)
    }

    #[cfg(feature = "std")]
    #[doc(hidden)]
    pub fn reconstruct_execution_context_for_bench(
        &self,
        shard_len: usize,
        missing_data: usize,
        missing_total: usize,
        data_only: bool,
        available_parallelism: usize,
    ) -> (
        crate::ParallelDecision,
        crate::ParallelPolicy,
        crate::ParallelPolicy,
    ) {
        let decision = self.reconstruct_parallel_decision_with(
            shard_len,
            missing_data,
            missing_total,
            data_only,
            available_parallelism,
        );
        let (data_policy, parity_policy) = self.reconstruct_stage_policies(data_only);
        (decision, data_policy, parity_policy)
    }

    #[cfg(feature = "std")]
    #[doc(hidden)]
    pub fn plan_option_vec_reconstruct_for_bench(
        &self,
        shards: &[Option<Vec<u8>>],
        required: Option<&[bool]>,
    ) -> Result<(usize, usize, usize, usize), crate::Error> {
        let plan = self.plan_option_vec_reconstruct(shards, required)?;
        Ok((
            plan.shape.shard_len,
            plan.shape.number_present,
            if plan.shape.invalid_indices.is_empty() {
                0
            } else {
                plan.shape.valid_indices.len()
            },
            plan.shape.invalid_indices.len(),
        ))
    }

    #[cfg(feature = "std")]
    /// Prepare a reusable reconstruct workspace for an `Option<Vec<u8>>` shard layout.
    ///
    /// The workspace is only valid for subsequent calls that keep the same
    /// missing-shard pattern, shard length, shard counts, and encoding matrix.
    /// Leopard families return `UnsupportedCodecFamily`.
    pub fn prepare_reconstruct_opt_workspace(
        &self,
        shards: &[Option<Vec<u8>>],
    ) -> Result<OptionVecReconstructWorkspace, crate::Error> {
        self.prepare_reconstruct_workspace(shards)
    }

    #[cfg(feature = "std")]
    /// Execute reconstruct using a previously prepared workspace.
    ///
    /// Executes the workspace's prepared targets through the bulk executor.
    /// The legacy preparer requests all missing shards; generic data/selected
    /// workspace preparers may also be used to retain only those outputs.
    /// This skips the per-call option-vec planning pass, but the caller must
    /// provide shards with the same missing-index layout and shard length as the
    /// one used to prepare `workspace`. Codec shard counts must also match;
    /// an incompatible encoding matrix returns `InvalidCustomMatrix`.
    pub fn reconstruct_opt_with_workspace(
        &self,
        shards: &mut [Option<Vec<u8>>],
        workspace: &OptionVecReconstructWorkspace,
    ) -> Result<(), crate::Error> {
        self.validate_reconstruct_workspace(shards, workspace)?;
        self.execute_classic_option_plan_serial(shards, &workspace.plan)
    }

    #[cfg(feature = "std")]
    #[doc(hidden)]
    pub fn execute_option_vec_reconstruct_plan_serial_for_bench(
        &self,
        shards: &mut [Option<Vec<u8>>],
        data_only: bool,
    ) -> Result<(), crate::Error> {
        if self.is_leopard_family() {
            return Err(crate::Error::UnsupportedCodecFamily);
        }
        let goal = if data_only {
            ReconstructionGoal::Data
        } else {
            ReconstructionGoal::All
        };
        self.with_classic_reconstruct_plan(shards, goal, |shards, plan| {
            self.execute_classic_option_plan_serial(shards, plan)
        })
    }

    #[cfg(feature = "std")]
    pub fn encode_opt<T, U>(&self, mut shards: T) -> Result<(), crate::Error>
    where
        T: AsRef<[U]> + AsMut<[U]>,
        U: AsRef<[u8]> + AsMut<[u8]> + Send + Sync,
    {
        if self.is_leopard_gf8_family() {
            let slices = shards.as_mut();
            self.validate_opt_shards(slices)?;
            let (data, parity) = slices.split_at_mut(self.data_shard_count());
            return self.encode_leopard_gf8_opt(&*data, parity);
        }

        self.encode_par(shards)
    }

    #[cfg(feature = "std")]
    pub fn encode_sep_opt<T, U>(&self, data: &[T], parity: &mut [U]) -> Result<(), crate::Error>
    where
        T: AsRef<[u8]> + Sync,
        U: AsRef<[u8]> + AsMut<[u8]> + Send,
    {
        if self.is_leopard_gf8_family() {
            if data.len() < self.data_shard_count() {
                return Err(crate::Error::TooFewDataShards);
            }
            if data.len() > self.data_shard_count() {
                return Err(crate::Error::TooManyDataShards);
            }
            if parity.len() < self.parity_shard_count() {
                return Err(crate::Error::TooFewParityShards);
            }
            if parity.len() > self.parity_shard_count() {
                return Err(crate::Error::TooManyParityShards);
            }
            let shard_len = Self::first_shard_len(data);
            if shard_len == 0 {
                return Err(crate::Error::EmptyShard);
            }
            if data.iter().any(|shard| shard.as_ref().len() != shard_len)
                || parity.iter().any(|shard| shard.as_ref().len() != shard_len)
            {
                return Err(crate::Error::IncorrectShardSize);
            }
            return self.encode_leopard_gf8_opt(data, parity);
        }

        self.encode_sep_par(data, parity)
    }

    #[cfg(feature = "std")]
    pub fn verify_opt<T>(&self, slices: &[T]) -> Result<bool, crate::Error>
    where
        T: AsRef<[u8]> + Sync,
    {
        if self.is_leopard_gf8_family() {
            return self.verify(slices);
        }
        // Encoding selects the execution policy and generated kernels together.
        // An outer serial branch would bypass that shared native dispatch.
        self.verify_par(slices)
    }

    #[cfg(feature = "std")]
    pub fn verify_with_buffer_opt<T, U>(
        &self,
        slices: &[T],
        buffer: &mut [U],
    ) -> Result<bool, crate::Error>
    where
        T: AsRef<[u8]> + Sync,
        U: AsRef<[u8]> + AsMut<[u8]> + Send,
    {
        if self.is_leopard_gf8_family() {
            return self.verify_with_buffer(slices, buffer);
        }
        self.verify_with_buffer_par(slices, buffer)
    }

    #[cfg(feature = "std")]
    pub fn verify_with_workspace_opt(
        &self,
        slices: &[Vec<u8>],
        workspace: &mut crate::VerifyWorkspace<crate::galois_8::Field>,
    ) -> Result<bool, crate::Error> {
        self.verify_with_workspace_par(slices, workspace)
    }

    #[cfg(feature = "std")]
    pub fn reconstruct_opt(&self, shards: &mut [Option<Vec<u8>>]) -> Result<(), crate::Error> {
        if self.is_leopard_family() {
            return self.reconstruct_leopard_option_direct(shards, |_| true);
        }
        self.reconstruct_classic_opt(shards, false)
    }

    #[cfg(feature = "std")]
    pub fn reconstruct_data_opt(&self, shards: &mut [Option<Vec<u8>>]) -> Result<(), crate::Error> {
        if self.is_leopard_family() {
            return self.reconstruct_leopard_option_direct(shards, |i| i < self.data_shard_count());
        }
        self.reconstruct_classic_opt(shards, true)
    }

    #[cfg(feature = "std")]
    fn reconstruct_classic_opt(
        &self,
        shards: &mut [Option<Vec<u8>>],
        data_only: bool,
    ) -> Result<(), crate::Error> {
        let goal = if data_only {
            ReconstructionGoal::Data
        } else {
            ReconstructionGoal::All
        };
        self.with_classic_reconstruct_plan(shards, goal, |shards, plan| {
            let decision = self.reconstruct_parallel_decision(
                plan.shape.shard_len,
                plan.shape.missing_data_count(self.data_shard_count()),
                plan.shape.invalid_indices.len(),
                data_only,
            );
            self.record_reconstruct_entry_path(decision.use_parallel);
            if decision.use_parallel {
                let (data_policy, parity_policy) = self.reconstruct_stage_policies(data_only);
                self.execute_classic_plan_parallel(shards, plan, data_policy, parity_policy)
            } else {
                if !data_only && plan.has_outputs() {
                    self.record_reconstruct_opt_fallback_serial_path();
                }
                self.execute_classic_option_plan_serial(shards, plan)
            }
        })
    }

    #[cfg(feature = "std")]
    pub fn reconstruct_some_opt(
        &self,
        shards: &mut [Option<Vec<u8>>],
        required: &[bool],
    ) -> Result<(), crate::Error> {
        if required.len() != self.total_shard_count() && required.len() != self.data_shard_count() {
            return Err(crate::Error::InvalidShardFlags);
        }
        if self.is_leopard_family() {
            return self.reconstruct_leopard_option_direct(shards, |i| {
                required.get(i).copied().unwrap_or(false)
            });
        }
        self.with_classic_reconstruct_plan(
            shards,
            ReconstructionGoal::Required(required),
            |shards, plan| {
                let policy = self.effective_parallel_policy();
                self.execute_classic_plan_parallel(shards, plan, policy, policy)
            },
        )
    }

    /// Accumulate Classic progressive recovery contributions into prepared destinations.
    ///
    /// With `Some(expected)`, the mask selects the decoding inputs and each call
    /// may supply a subset of those inputs. Initialize output buffers to zero
    /// before the first contribution, and apply each successful contribution once.
    ///
    /// With `None`, merge partial results by XOR at matching shard indices. Each
    /// present input requires an equally sized destination. Lengths may differ
    /// between indices, and empty pairs are allowed. The complete merge layout
    /// is validated before writing, so an invalid batch leaves destinations
    /// unchanged and can be retried after its layout is repaired.
    ///
    /// Counts must match the codec's total shard count. Leopard is unsupported.
    #[cfg(feature = "std")]
    #[allow(clippy::needless_range_loop)]
    pub fn decode_idx(
        &self,
        dst: &mut [Option<Vec<u8>>],
        expect_input: Option<&[bool]>,
        input: &[Option<Vec<u8>>],
    ) -> Result<(), crate::Error> {
        if self.is_leopard_family() {
            return Err(crate::Error::UnsupportedCodecFamily);
        }
        for count in [dst.len(), input.len()] {
            self.validate_opt_shard_count(count)?;
        }

        if let Some(expect_input) = expect_input {
            if expect_input.len() != self.total_shard_count() {
                return Err(crate::Error::InvalidShardFlags);
            }

            let mut valid_indices =
                smallvec::SmallVec::<[usize; 32]>::with_capacity(self.data_shard_count());
            let mut invalid_indices =
                smallvec::SmallVec::<[usize; 32]>::with_capacity(self.parity_shard_count());

            for (idx, expected) in expect_input.iter().copied().enumerate() {
                if expected {
                    valid_indices.push(idx);
                } else {
                    invalid_indices.push(idx);
                }
            }

            if valid_indices.len() < self.data_shard_count() {
                return Err(crate::Error::TooFewShardsPresent);
            }

            let shard_len = input
                .iter()
                .chain(dst.iter())
                .find_map(|shard| shard.as_ref().map(|shard| shard.len()))
                .ok_or(crate::Error::TooFewShardsPresent)?;

            for shard in input.iter().flatten() {
                if shard.len() != shard_len {
                    return Err(crate::Error::IncorrectShardSize);
                }
            }
            for shard in dst.iter().flatten() {
                if shard.len() != shard_len {
                    return Err(crate::Error::IncorrectShardSize);
                }
            }

            let mut output_indices: smallvec::SmallVec<[usize; 32]> = smallvec::SmallVec::new();

            for (idx, shard) in dst.iter().enumerate() {
                let Some(_dst_shard) = shard.as_ref() else {
                    continue;
                };
                output_indices.push(idx);
            }

            if output_indices.is_empty() {
                return Ok(());
            }

            let mut input_positions = smallvec::SmallVec::<[usize; 32]>::new();
            let mut input_refs = smallvec::SmallVec::<[&[u8]; 32]>::new();
            for (col, &idx) in valid_indices
                .iter()
                .take(self.data_shard_count())
                .enumerate()
            {
                if let Some(shard) = input[idx].as_deref() {
                    input_positions.push(col);
                    input_refs.push(shard);
                }
            }

            if input_refs.is_empty() {
                return Ok(());
            }

            let data_decode_matrix =
                self.get_data_decode_matrix(&valid_indices, &invalid_indices)?;
            let parity_rows = self.get_parity_rows();
            let mut reduced_rows: smallvec::SmallVec<[smallvec::SmallVec<[u8; 32]>; 32]> =
                smallvec::SmallVec::with_capacity(output_indices.len());
            for &idx in &output_indices {
                let mut row = smallvec::SmallVec::<[u8; 32]>::with_capacity(input_positions.len());
                if idx < self.data_shard_count() {
                    for &col in &input_positions {
                        row.push(data_decode_matrix.get(idx, col));
                    }
                } else {
                    let parity_row = parity_rows.get_row(idx - self.data_shard_count());
                    for &col in &input_positions {
                        let mut acc = 0u8;
                        for i in 0..self.data_shard_count() {
                            acc ^= super::mul(parity_row[i], data_decode_matrix.get(i, col));
                        }
                        row.push(acc);
                    }
                }
                reduced_rows.push(row);
            }

            let mut output_ptrs: smallvec::SmallVec<[(*mut u8, usize); 32]> =
                smallvec::SmallVec::with_capacity(output_indices.len());
            for &idx in &output_indices {
                let dst_shard = dst[idx]
                    .as_deref_mut()
                    .expect("output index was collected only for present destinations");
                if dst_shard.len() != shard_len {
                    return Err(crate::Error::IncorrectShardSize);
                }
                output_ptrs.push((dst_shard.as_mut_ptr(), dst_shard.len()));
            }

            {
                let mut output_refs: smallvec::SmallVec<[&mut [u8]; 32]> = output_ptrs
                    .iter()
                    .map(|&(ptr, len)| {
                        // SAFETY: each pointer/length pair was captured from a distinct
                        // destination shard collected in `output_indices`. Those shards
                        // remain alive for this scope and are only mutated through these
                        // temporary slices.
                        unsafe { core::slice::from_raw_parts_mut(ptr, len) }
                    })
                    .collect();

                self.decode_idx_accumulate_reduced_outputs(
                    &reduced_rows,
                    &input_refs,
                    &mut output_refs,
                );
            }

            return Ok(());
        }

        // Validate every active pair before applying any XOR. A later layout
        // error must not leave earlier contributions applied, since retrying
        // that batch would apply them twice and cancel their recovered bytes.
        for (dst_shard, input_shard) in dst.iter().zip(input) {
            if let Some(input) = input_shard {
                let dst = dst_shard.as_ref().ok_or(crate::Error::TooFewShards)?;
                if dst.len() != input.len() {
                    return Err(crate::Error::IncorrectShardSize);
                }
            }
        }
        for (dst_shard, input_shard) in dst.iter_mut().zip(input) {
            if let (Some(dst), Some(input)) = (dst_shard.as_deref_mut(), input_shard.as_deref()) {
                for (dst_byte, input_byte) in dst.iter_mut().zip(input) {
                    *dst_byte ^= *input_byte;
                }
            }
        }

        Ok(())
    }
}

#[cfg(feature = "std")]
fn reconstruct_parallel_policy_default(
    base: crate::ParallelPolicy,
    data_only: bool,
) -> crate::ParallelPolicy {
    let data_only_min = parse_positive_env_usize(RS_RECONSTRUCT_DATA_MIN_PARALLEL_SHARD_BYTES_ENV)
        .unwrap_or(RECONSTRUCT_DATA_MIN_PARALLEL_SHARD_BYTES);
    let full_min = parse_positive_env_usize(RS_RECONSTRUCT_FULL_MIN_PARALLEL_SHARD_BYTES_ENV)
        .unwrap_or(RECONSTRUCT_FULL_MIN_PARALLEL_SHARD_BYTES);
    let min_bytes_per_job = parse_positive_env_usize(RS_RECONSTRUCT_MIN_BYTES_PER_JOB_ENV)
        .unwrap_or(base.min_bytes_per_job);
    if data_only {
        crate::ParallelPolicy {
            min_parallel_shard_bytes: core::cmp::max(base.min_parallel_shard_bytes, data_only_min),
            min_bytes_per_job,
            max_jobs: base.max_jobs,
            l2_cache_bytes: base.l2_cache_bytes,
        }
    } else {
        crate::ParallelPolicy {
            min_parallel_shard_bytes: core::cmp::max(base.min_parallel_shard_bytes / 2, full_min),
            min_bytes_per_job,
            max_jobs: base.max_jobs,
            l2_cache_bytes: base.l2_cache_bytes,
        }
    }
}

#[cfg(feature = "std")]
fn parse_positive_env_usize(name: &str) -> Option<usize> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
}

#[cfg(all(feature = "std", target_arch = "aarch64"))]
fn reconstruct_policy_cache_aarch64(base: crate::ParallelPolicy) -> RuntimeParallelPolicyCache {
    let mut reconstruct_full_data = reconstruct_parallel_policy_default(base, false);
    if let Some(value) =
        parse_positive_env_usize(RS_AARCH64_RECONSTRUCT_MIN_PARALLEL_SHARD_BYTES_ENV)
    {
        reconstruct_full_data.min_parallel_shard_bytes = value;
    }
    if let Some(value) = parse_positive_env_usize(RS_AARCH64_RECONSTRUCT_MIN_BYTES_PER_JOB_ENV) {
        reconstruct_full_data.min_bytes_per_job = value;
    }
    if let Some(value) = std::env::var(RS_AARCH64_RECONSTRUCT_MAX_JOBS_ENV)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
    {
        reconstruct_full_data.max_jobs = value;
    }

    let mut reconstruct_data = reconstruct_parallel_policy_default(base, true);
    reconstruct_data.min_parallel_shard_bytes = reconstruct_full_data.min_parallel_shard_bytes;
    reconstruct_data.min_bytes_per_job = reconstruct_full_data.min_bytes_per_job;
    reconstruct_data.max_jobs = reconstruct_full_data.max_jobs;

    let mut reconstruct_full_parity = reconstruct_parallel_policy_default(base, false);
    if let Some(value) =
        parse_positive_env_usize(RS_AARCH64_RECONSTRUCT_PARITY_MIN_BYTES_PER_JOB_ENV)
    {
        reconstruct_full_parity.min_bytes_per_job = value;
    }

    if let Some(value) = parse_positive_env_usize(RS_AARCH64_RECONSTRUCT_DATA_MIN_BYTES_PER_JOB_ENV)
    {
        reconstruct_data.min_bytes_per_job = value;
    }

    RuntimeParallelPolicyCache {
        available_parallelism: std::thread::available_parallelism()
            .map(|parallelism| parallelism.get())
            .unwrap_or(1),
        data: base,
        reconstruct_data,
        reconstruct_full_data,
        reconstruct_full_parity,
    }
}

#[cfg(all(feature = "std", not(target_arch = "aarch64")))]
pub(crate) fn resolve_runtime_parallel_policy_cache(
    base: crate::ParallelPolicy,
) -> RuntimeParallelPolicyCache {
    let reconstruct_data = reconstruct_parallel_policy_default(base, true);
    let reconstruct_full = reconstruct_parallel_policy_default(base, false);
    RuntimeParallelPolicyCache {
        available_parallelism: std::thread::available_parallelism()
            .map(|parallelism| parallelism.get())
            .unwrap_or(1),
        data: base,
        reconstruct_data,
        reconstruct_full_data: reconstruct_full,
        reconstruct_full_parity: reconstruct_full,
    }
}

#[cfg(all(feature = "std", target_arch = "aarch64"))]
pub(crate) fn resolve_runtime_parallel_policy_cache(
    base: crate::ParallelPolicy,
) -> RuntimeParallelPolicyCache {
    reconstruct_policy_cache_aarch64(base)
}
