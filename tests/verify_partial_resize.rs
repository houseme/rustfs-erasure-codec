use rustfs_erasure_codec::{Field, ReedSolomon, VerifyWorkspace, galois_8};
use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
};
thread_local! { static FAIL_ON_ZERO: Cell<Option<usize>> = const { Cell::new(None) }; }
struct FailField;
impl Field for FailField {
    const ORDER: usize = 256;
    type Elem = u8;
    fn add(a: u8, b: u8) -> u8 {
        galois_8::add(a, b)
    }
    fn mul(a: u8, b: u8) -> u8 {
        galois_8::mul(a, b)
    }
    fn div(a: u8, b: u8) -> u8 {
        galois_8::div(a, b)
    }
    fn exp(a: u8, n: usize) -> u8 {
        galois_8::exp(a, n)
    }
    fn zero() -> u8 {
        FAIL_ON_ZERO.with(|counter| match counter.get() {
            Some(0) => {
                counter.set(None);
                panic!("injected zero failure");
            }
            Some(n) => counter.set(Some(n - 1)),
            None => {}
        });
        0
    }
    fn one() -> u8 {
        1
    }
    fn nth_internal(n: usize) -> u8 {
        n as u8
    }
}
#[test]
fn verification_repairs_partial_workspace_resize_after_unwinding() {
    let codec = ReedSolomon::<FailField>::new(4, 4).unwrap();
    let mut workspace = VerifyWorkspace::new(&codec, 64);
    let mut shards = vec![vec![0; 128]; 8];
    for (row, shard) in shards[..4].iter_mut().enumerate() {
        for (i, byte) in shard.iter_mut().enumerate() {
            *byte = (row * 19 + i * 7) as u8;
        }
    }
    codec.encode(&mut shards).unwrap();
    FAIL_ON_ZERO.with(|counter| counter.set(Some(1)));
    assert!(catch_unwind(AssertUnwindSafe(|| workspace.resize(&codec, 128))).is_err());
    assert_eq!(workspace.shard_len(), Some(128));
    assert!(
        codec
            .verify_with_workspace(&shards, &mut workspace)
            .unwrap()
    );
    shards[7][127] ^= 1;
    assert!(
        !codec
            .verify_with_workspace(&shards, &mut workspace)
            .unwrap()
    );
}
