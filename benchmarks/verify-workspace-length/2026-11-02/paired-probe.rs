use std::hint::black_box;
use std::time::{Duration,Instant};
fn measure(mut f: impl FnMut()) -> f64 { let warm=Instant::now();while warm.elapsed()<Duration::from_millis(100){f()};let start=Instant::now();let mut n=0u64;while start.elapsed()<Duration::from_millis(300){for _ in 0..32 {f();} n+=32;} start.elapsed().as_nanos() as f64/n as f64 }
fn main(){for (data,parity,len) in [(4,2,64),(4,2,256),(10,4,64),(10,4,256),(2,33,64),(4,2,4096)] {
 let a=base_codec::galois_8::ReedSolomon::new(data,parity).unwrap();let b=next_codec::galois_8::ReedSolomon::new(data,parity).unwrap();
 let mut shards=vec![vec![0u8;len];data+parity];for (r,s) in shards.iter_mut().take(data).enumerate(){for(i,v) in s.iter_mut().enumerate(){*v=(r*37+i*13) as u8;}} a.encode(&mut shards).unwrap();let mut wa=base_codec::VerifyWorkspace::new(&a,len);let mut wb=next_codec::VerifyWorkspace::new(&b,len);assert!(a.verify_with_workspace(&shards,&mut wa).unwrap());assert!(b.verify_with_workspace(&shards,&mut wb).unwrap());
 let aa=measure(||{black_box(a.verify_with_workspace(black_box(&shards),black_box(&mut wa)).unwrap());});let b1=measure(||{black_box(b.verify_with_workspace(black_box(&shards),black_box(&mut wb)).unwrap());});let b2=measure(||{black_box(b.verify_with_workspace(black_box(&shards),black_box(&mut wb)).unwrap());});let a2=measure(||{black_box(a.verify_with_workspace(black_box(&shards),black_box(&mut wa)).unwrap());});
 println!("{data}+{parity}/{len} a1={aa:.2} b1={b1:.2} b2={b2:.2} a2={a2:.2} A_drift={:.2}% B_drift={:.2}% diff={:.2}%",(a2/aa-1.)*100.,(b2/b1-1.)*100.,((b1+b2)/(aa+a2)-1.)*100.);
}}
