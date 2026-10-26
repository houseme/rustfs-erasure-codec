//! Retained inputs and result oracles for shape/plan diagnostics only.

#[path = "../../tests/common/reconstruct_shape_cases.rs"]
pub mod cases;

use cases::{Case, expected_error};
use rustfs_erasure_codec::{
    CodecOptions, Error, Field, MatrixMode, ReconstructWorkspace, ReedSolomon, ShardSlot,
};

pub const SHARD_ELEMENTS: usize = 65;
pub const ACTIONS: [&str; 3] = ["prepare_full", "prepare_none", "call_none"];

pub struct Fixture<F: Field> {
    pub(crate) codec: ReedSolomon<F>,
    pub(crate) shards: Vec<ShardSlot<Vec<F::Elem>>>,
    original: Vec<Vec<F::Elem>>,
    required: Vec<bool>,
    pub(crate) case: Case,
}

impl<F: Field> Fixture<F> {
    pub fn new(case: Case) -> Self {
        let codec = ReedSolomon::<F>::with_options(
            case.data,
            case.parity,
            CodecOptions::builder()
                .matrix_mode(MatrixMode::Cauchy)
                .inversion_cache(true)
                .build(),
        )
        .expect("construct planning fixture");
        let mut original: Vec<Vec<_>> = (0..case.data + case.parity)
            .map(|row| {
                (0..SHARD_ELEMENTS)
                    .map(|offset| F::nth((row * 379 + offset * 191 + 9281) % F::ORDER))
                    .collect()
            })
            .collect();
        codec
            .encode(&mut original)
            .expect("encode reference stripe");
        let mut shards: Vec<_> = original
            .iter()
            .cloned()
            .map(ShardSlot::new_present)
            .collect();
        for &index in &case.missing {
            shards[index].mark_missing();
            shards[index].as_inner_mut().fill(F::one());
        }
        if let Some(length) = case.bad_length {
            shards
                .last_mut()
                .expect("nonempty fixture")
                .as_inner_mut()
                .truncate(length);
        }
        let required = vec![false; case.data + case.parity];
        Self {
            codec,
            shards,
            original,
            required,
            case,
        }
    }

    pub fn prepare_full(&self) -> Result<ReconstructWorkspace<F>, Error> {
        self.codec.prepare_reconstruct_workspace(&self.shards)
    }

    pub fn prepare_none(&self) -> Result<ReconstructWorkspace<F>, Error> {
        self.codec
            .prepare_reconstruct_some_workspace(&self.shards, &self.required)
    }

    pub fn call_none(&mut self) -> Result<(), Error> {
        self.codec
            .reconstruct_some(&mut self.shards, &self.required)
    }

    /// Run outside measurement: warm the full inverse, verify prepared outputs,
    /// and check errors and no-output state against independent stripe bytes.
    pub fn validate(&mut self) {
        let initial = self.shards.clone();
        for full in [true, false] {
            let result = if full {
                self.prepare_full()
            } else {
                self.prepare_none()
            };
            match (result, expected_error(&self.case)) {
                (Err(actual), Some(expected)) => assert_eq!(actual, expected),
                (Ok(plan), None) => {
                    let expected_targets: Vec<_> = if full {
                        self.case.missing.clone()
                    } else {
                        vec![]
                    };
                    let actual_targets: Vec<_> = plan
                        .data_targets()
                        .iter()
                        .chain(plan.parity_targets())
                        .copied()
                        .collect();
                    assert_eq!(actual_targets, expected_targets);
                    let mut restored = self.shards.clone();
                    self.codec
                        .reconstruct_with_workspace(&mut restored, &plan)
                        .expect("execute prepared plan outside measurement");
                    if full {
                        for (slot, expected) in restored.iter().zip(&self.original) {
                            assert!(slot.is_present());
                            assert_eq!(slot.as_inner(), expected);
                        }
                    } else {
                        assert_eq!(restored, initial);
                    }
                }
                _ => panic!("unexpected plan result: {}", self.case.name),
            }
        }
        assert_eq!(
            self.call_none(),
            expected_error(&self.case).map_or(Ok(()), Err)
        );
        assert_eq!(self.shards, initial);
    }
}
