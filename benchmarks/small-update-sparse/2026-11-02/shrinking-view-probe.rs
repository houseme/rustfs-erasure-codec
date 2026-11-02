use rustfs_erasure_codec::galois_8::ReedSolomon;
use std::panic::{AssertUnwindSafe, catch_unwind};

struct ShrinkingView(Vec<u8>);
impl AsRef<[u8]> for ShrinkingView {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl AsMut<[u8]> for ShrinkingView {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.0[..1]
    }
}
#[test]
fn observe_a_short_mutable_view_after_successful_validation() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let old = vec![vec![0; 256]; 4];
    let mut replacement = old[0].clone();
    replacement[0] = 1;
    let old_refs: Vec<_> = old.iter().map(Vec::as_slice).collect();
    let changes = [Some(replacement.as_slice()), None, None, None];
    let mut parity = [ShrinkingView(vec![0; 256]), ShrinkingView(vec![0; 256])];
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        codec.update(&old_refs, &changes, &mut parity)
    }));
    println!(
        "SHRINKING_VIEW panicked={} result={:?} first_parity_byte={}",
        outcome.is_err(),
        outcome.as_ref().ok(),
        parity[0].0[0]
    );
}
