use rustfs_erasure_codec::{CodecFamily, CodecOptions, Field, ReedSolomon, galois_8};
use std::cell::Cell;
thread_local! { static STACK_MARK: Cell<Option<usize>> = const { Cell::new(None) }; }
struct AddressAwareField;
impl Field for AddressAwareField {
    const ORDER: usize = 256;
    type Elem = u8;
    fn as_gf8_slice(s: &[u8]) -> Option<&[u8]> {
        Some(s)
    }
    fn as_gf8_slice_mut(s: &mut [u8]) -> Option<&mut [u8]> {
        if s.is_empty() {
            return Some(s);
        }
        let address = s.as_ptr() as usize;
        if STACK_MARK
            .with(Cell::get)
            .is_some_and(|marker| marker.abs_diff(address) < 1_048_576)
        {
            None
        } else {
            Some(s)
        }
    }
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
fn address_aware_output_view() {
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        let codec = ReedSolomon::<AddressAwareField>::with_options(
            10,
            4,
            CodecOptions::builder()
                .codec_family(family)
                .max_parallel_jobs(1)
                .build(),
        )
        .unwrap();
        let mut shards: Vec<Vec<u8>> = (0..14)
            .map(|i| (0..64).map(|j| (i * 31 + j * 17) as u8).collect())
            .collect();
        STACK_MARK.with(|p| p.set(None));
        codec.encode(&mut shards).unwrap();
        let marker = 0u8;
        STACK_MARK.with(|p| p.set(Some(&marker as *const u8 as usize)));
        println!("{family:?} {:?}", codec.verify(&shards));
        STACK_MARK.with(|p| p.set(None));
    }
}
