use codec_new as new;
use codec_old as old;

fn main() {
    let expected_backend = std::env::var("RSE_EXPECT_BACKEND").unwrap();
    assert_eq!(new::galois_8::active_backend_name(), expected_backend);
    let mut tested = 0usize;
    for family in ["classic", "gf8", "gf16"] {
        for (k, p) in [(4usize, 2usize), (10, 4), (16, 8)] {
            let old_family = match family { "classic" => old::CodecFamily::Classic, "gf8" => old::CodecFamily::LeopardGF8, _ => old::CodecFamily::LeopardGF16 };
            let new_family = match family { "classic" => new::CodecFamily::Classic, "gf8" => new::CodecFamily::LeopardGF8, _ => new::CodecFamily::LeopardGF16 };
            let a = old::galois_8::ReedSolomon::with_options(k, p, old::CodecOptions::builder().codec_family(old_family).max_parallel_jobs(1).build()).unwrap();
            let b = new::galois_8::ReedSolomon::with_options(k, p, new::CodecOptions::builder().codec_family(new_family).max_parallel_jobs(1).build()).unwrap();
            for len in [65_535usize, 65_536, 65_537, 1_048_575, 1_048_576, 1_048_577] {
                if family != "classic" && len % 64 != 0 { continue; }
                let mut original: Vec<Vec<u8>> = (0..k+p).map(|r| (0..len).map(|i| (r*37+i*13+i/17) as u8).collect()).collect();
                let mut candidate = original.clone();
                a.encode(&mut original).unwrap();
                b.encode(&mut candidate).unwrap();
                assert_eq!(original, candidate, "parity {family} {k}+{p} {len}");
                assert!(a.verify(&candidate).unwrap());
                assert!(b.verify(&original).unwrap());
                let mut recover_old: Vec<_> = candidate.iter().cloned().map(Some).collect();
                let mut recover_new: Vec<_> = original.iter().cloned().map(Some).collect();
                for i in [0, k+p-1] { recover_old[i]=None; recover_new[i]=None; }
                a.reconstruct(&mut recover_old).unwrap();
                b.reconstruct(&mut recover_new).unwrap();
                assert!(recover_old.iter().zip(&original).all(|(x,y)| x.as_ref()==Some(y)), "old decode {family} {k}+{p} {len}");
                assert!(recover_new.iter().zip(&original).all(|(x,y)| x.as_ref()==Some(y)), "new decode {family} {k}+{p} {len}");
                candidate[k][len-1] ^= 1;
                assert!(!a.verify(&candidate).unwrap());
                assert!(!b.verify(&candidate).unwrap());
                tested += 1;
                println!("PASS {family} {k}+{p} {len}");
            }
        }
    }
    assert_eq!(tested, 30);
    println!("30 large boundary cross-version cases passed");
}
