use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rustfs_erasure_codec::{galois_8, CodecFamily, CodecOptions, VerifyWorkspace};
use std::hint::black_box;
fn run(c: &mut Criterion) {
    if let Ok(name) = std::env::var("RSE_EXPECT_BACKEND") { assert_eq!(galois_8::active_backend_name(), name); }
    let mut group = c.benchmark_group("small_vec_verify");
    for (family_name,family) in [("classic",CodecFamily::Classic),("leopard",CodecFamily::LeopardGF8),("gf16",CodecFamily::LeopardGF16)] {
        for (k,p) in [(4usize,2usize),(10,4),(16,8)] {
            let codec=galois_8::ReedSolomon::with_options(k,p,CodecOptions::builder().codec_family(family).max_parallel_jobs(1).build()).unwrap();
            for len in [64usize,256,257,258,271,4096] {
                if family_name!="classic" && len%64!=0 { continue; }
                let mut shards:Vec<Vec<u8>>=(0..k+p).map(|r|(0..len).map(|i|(r*37+i*13+i/11) as u8).collect()).collect();
                codec.encode(&mut shards).unwrap();
                let mut encode_shards = shards.clone();
                let encode_id = format!("{family_name}/{k}x{p}/{len}");
                group.bench_function(BenchmarkId::new("encode", encode_id), |b| b.iter(|| codec.encode(black_box(&mut encode_shards)).unwrap()));
                for corrupt in [false,true] {
                    let mut input=shards.clone(); if corrupt { input[k][0]^=1; }
                    let mut ws=VerifyWorkspace::new(&codec,len);
                    assert_eq!(codec.verify_with_workspace_opt(&input,&mut ws),Ok(!corrupt));
                    let id=format!("{family_name}/{k}x{p}/{len}/{}",if corrupt {"bad"} else {"valid"});
                    group.bench_function(BenchmarkId::new("workspace",id), |b| b.iter(|| black_box(codec.verify_with_workspace_opt(black_box(&input),black_box(&mut ws)).unwrap())));
                }
            }
        }
    }
    group.finish();
}
criterion_group!(benches,run); criterion_main!(benches);
