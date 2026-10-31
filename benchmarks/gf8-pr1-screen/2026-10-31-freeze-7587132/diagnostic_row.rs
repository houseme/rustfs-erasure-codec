//! Temporary benchmark bridge. This module is not part of the production crate.
use crate::core::ReconstructionGoal;
use crate::matrix::MatrixRows;
use crate::{Error, galois_8::ReedSolomon};

/// Extract the actual one-missing-data recovery row and survivor order.
pub fn recovery_row(
    codec: &ReedSolomon,
    shards: &mut [Option<Vec<u8>>],
) -> Result<(Vec<usize>, Vec<u8>), Error> {
    let mut result = None;
    codec.with_classic_reconstruct_plan(shards, ReconstructionGoal::Data, |_, plan| {
        assert_eq!(plan.data_targets().len(), 1);
        let rows = plan.row_views(codec);
        result = Some((plan.shape.valid_indices.to_vec(), rows.row(0).to_vec()));
        Ok(())
    })?;
    Ok(result.expect("diagnostic fixture must contain one missing shard"))
}

/// Code a prevalidated row into a retained output buffer with the existing scheduler.
pub fn run_row(
    codec: &ReedSolomon,
    row: &[u8],
    inputs: &[&[u8]],
    output: &mut [u8],
    chunk_len: usize,
    jobs: usize,
) {
    assert_eq!(inputs.len(), codec.data_shard_count());
    assert_eq!(row.len(), inputs.len());
    assert!(inputs.iter().all(|input| input.len() == output.len()));
    assert!(chunk_len > 0 && jobs > 0);
    let rows = [row];
    let mut outputs = [output];
    codec.code_some_slices_par_chunked_with_jobs(&rows[..], inputs, &mut outputs, chunk_len, jobs);
}
