#![cfg(all(feature = "std", rse_x86_avx2))]
use rustfs_erasure_codec::{Field, galois_8};
use std::cell::Cell;
thread_local! { static VIEWS: Cell<usize> = const { Cell::new(0) }; }
struct Counted;
impl Field for Counted {
    const ORDER: usize = 256; type Elem = u8;
    fn add(a: u8,b: u8)->u8 {galois_8::add(a,b)}
    fn mul(a: u8,b: u8)->u8 {galois_8::mul(a,b)}
    fn div(a: u8,b: u8)->u8 {galois_8::div(a,b)}
    fn exp(a: u8,n:usize)->u8 {galois_8::exp(a,n)}
    fn zero()->u8 {0} fn one()->u8 {1} fn nth_internal(n:usize)->u8 {n as u8}
    fn as_gf8_slice(s:&[u8])->Option<&[u8]> {VIEWS.with(|n|n.set(n.get()+1));Some(s)}
    fn as_gf8_slice_mut(s:&mut [u8])->Option<&mut [u8]> {VIEWS.with(|n|n.set(n.get()+1));Some(s)}
}
#[test]
fn one_missing_data_uses_row_only_at_threshold() {
    if !std::is_x86_feature_detected!("avx2") { return; }
    const CHILD: &str="RSE_NARROW_ROW_CHILD";
    if std::env::var_os(CHILD).is_none() {
       let ok=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","one_missing_data_uses_row_only_at_threshold"]).env(CHILD,"1").env("RSE_BACKEND_OVERRIDE","rust-avx2").status().unwrap();assert!(ok.success());return;
    }
    assert_eq!(galois_8::active_backend_id(),galois_8::BackendId::RustAvx2);
    let codec=rustfs_erasure_codec::ReedSolomon::<Counted>::new(10,4).unwrap();
    for (len,active) in [(1_048_575,false),(1_048_576,true),(1_048_577,true)] {
       let mut shards:Vec<Vec<u8>>=(0..14).map(|r|(0..len).map(|i|(r*73+i*19+i/17) as u8).collect()).collect();
       galois_8::ReedSolomon::new(10,4).unwrap().encode(&mut shards).unwrap();
       let mut broken:Vec<Option<Vec<u8>>>=shards.iter().cloned().map(Some).collect(); broken[4]=None;
       VIEWS.with(|n|n.set(0));codec.reconstruct_data(&mut broken).unwrap();
       assert_eq!(VIEWS.with(Cell::get)>0,active,"len={len}"); assert_eq!(broken[4].as_ref().unwrap(),&shards[4]);
    }
}
