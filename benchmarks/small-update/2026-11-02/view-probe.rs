use std::cell::Cell;

use rustfs_erasure_codec::galois_8::ReedSolomon;

struct View {
    bytes: Vec<u8>,
    reads: Cell<usize>,
    writes: Cell<usize>,
    valid_reads: usize,
}

impl View {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            reads: Cell::new(0),
            writes: Cell::new(0),
            valid_reads: usize::MAX,
        }
    }
}

impl AsRef<[u8]> for View {
    fn as_ref(&self) -> &[u8] {
        let reads = self.reads.get();
        self.reads.set(reads.saturating_add(1));
        if reads >= self.valid_reads {
            &[]
        } else {
            &self.bytes
        }
    }
}

impl AsMut<[u8]> for View {
    fn as_mut(&mut self) -> &mut [u8] {
        self.writes.set(self.writes.get().saturating_add(1));
        &mut self.bytes
    }
}

#[test]
fn observe_update_views_and_results() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    for len in [64, 256, 257] {
        let original: Vec<Vec<u8>> = (0..4)
            .map(|row| (0..len).map(|i| (row * 37 + i * 13 + 1) as u8).collect())
            .collect();
        let mut original_parity = vec![vec![0; len]; 2];
        codec.encode_sep(&original, &mut original_parity).unwrap();
        for condition in [
            "none",
            "identical",
            "one",
            "two",
            "shrink_old",
            "shrink_new",
        ] {
            let mut changed = original.clone();
            if !matches!(condition, "none" | "identical") {
                changed[0][0] ^= 0x5a;
            }
            if condition == "two" {
                changed[3][len - 1] ^= 0xa5;
            }
            let mut expected = original_parity.clone();
            codec.encode_sep(&changed, &mut expected).unwrap();
            let mut old: Vec<_> = original.iter().cloned().map(View::new).collect();
            if condition == "shrink_old" {
                old[0].valid_reads = 2;
            }
            let mut changes: Vec<Option<View>> = (0..4).map(|_| None).collect();
            if condition != "none" {
                changes[0] = Some(View::new(changed[0].clone()));
                if condition == "shrink_new" {
                    changes[0].as_mut().unwrap().valid_reads = 1;
                }
            }
            if condition == "two" {
                changes[3] = Some(View::new(changed[3].clone()));
            }
            let mut parity: Vec<_> = original_parity.iter().cloned().map(View::new).collect();
            let result = codec.update(&old, &changes, &mut parity);
            let actual: Vec<_> = parity.iter().map(|shard| shard.bytes.clone()).collect();
            if !condition.starts_with("shrink_") {
                assert_eq!(result, Ok(()));
                assert_eq!(actual, expected);
            }
            let old_reads: Vec<_> = old.iter().map(|v| v.reads.get()).collect();
            let new_reads: Vec<_> = changes
                .iter()
                .map(|v| v.as_ref().map(|v| v.reads.get()))
                .collect();
            let parity_reads: Vec<_> = parity.iter().map(|v| v.reads.get()).collect();
            let parity_writes: Vec<_> = parity.iter().map(|v| v.writes.get()).collect();
            println!(
                "VIEW_OBSERVATION len={len} condition={condition} result={result:?} old={old_reads:?} new={new_reads:?} parity_reads={parity_reads:?} parity_writes={parity_writes:?} parity={actual:?}"
            );
        }
    }
}
