use rustfs_erasure_codec::{Field, ReedSolomon, galois_8};
use std::sync::atomic::{AtomicUsize, Ordering};

static ADDS: AtomicUsize = AtomicUsize::new(0);
static MULTIPLIED: AtomicUsize = AtomicUsize::new(0);
struct Counted;
impl Field for Counted {
    const ORDER: usize = 256;
    type Elem = u8;
    fn add(a: u8, b: u8) -> u8 {
        ADDS.fetch_add(1, Ordering::Relaxed);
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
        0
    }
    fn one() -> u8 {
        1
    }
    fn nth_internal(n: usize) -> u8 {
        n as u8
    }
    fn mul_slice_add(c: u8, input: &[u8], output: &mut [u8]) {
        MULTIPLIED.fetch_add(input.len(), Ordering::Relaxed);
        galois_8::Field::mul_slice_add(c, input, output);
    }
}

#[test]
fn record_sparse_update_work_and_check_full_encode() {
    let codec = ReedSolomon::<Counted>::new(4, 2).unwrap();
    let original: Vec<Vec<u8>> = (0..4)
        .map(|r| (0..256).map(|i| (r * 37 + i * 13) as u8).collect())
        .collect();
    let refs: Vec<_> = original.iter().map(Vec::as_slice).collect();
    let mut old_parity = vec![vec![0; 256]; 2];
    codec.encode_sep(&refs, &mut old_parity).unwrap();
    for (label, positions) in [
        ("first", vec![0]),
        ("middle", vec![128]),
        ("last", vec![255]),
        ("ends", vec![0, 255]),
        ("dense", (0..256).collect()),
        ("identical", vec![]),
    ] {
        let mut changed = original.clone();
        for pos in positions {
            changed[0][pos] ^= 0x5a;
        }
        let mut expected = vec![vec![0; 256]; 2];
        codec.encode_sep(&changed, &mut expected).unwrap();
        let mut changes = vec![None; 4];
        changes[0] = Some(changed[0].as_slice());
        let mut parity = old_parity.clone();
        ADDS.store(0, Ordering::Relaxed);
        MULTIPLIED.store(0, Ordering::Relaxed);
        codec.update(&refs, &changes, &mut parity).unwrap();
        let adds = ADDS.load(Ordering::Relaxed);
        let multiplied = MULTIPLIED.load(Ordering::Relaxed);
        assert_eq!(parity, expected, "{label}");
        println!("SPARSE_WORK case={label} delta_adds={adds} multiplied_elements={multiplied}");
    }
}
