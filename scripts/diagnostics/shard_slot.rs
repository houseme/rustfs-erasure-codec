//! Instrumented, bounded allocation-layout diagnosis. Never release timing evidence.
use rustfs_erasure_codec::ShardSlot;
use std::fmt::{self, Write};

const SHARDS: usize = 6;
const MAX_LAYOUTS: usize = 128;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Buffer {
    len: usize,
    capacity: usize,
    offset: usize,
}

type Layout = [Buffer; SHARDS];

fn layout<'a>(buffers: impl Iterator<Item = &'a Vec<u8>>, page_size: usize) -> Layout {
    let mut result = Layout::default();
    let mut count = 0;
    for (item, buffer) in result.iter_mut().zip(buffers) {
        *item = Buffer {
            len: buffer.len(),
            capacity: buffer.capacity(),
            offset: buffer.as_ptr() as usize % page_size,
        };
        count += 1;
    }
    assert_eq!(count, SHARDS);
    result
}

#[derive(Clone, Copy, Default)]
struct Bucket {
    layout: Layout,
    count: usize,
}

pub(super) struct Trace {
    source: Layout,
    buckets: [Bucket; MAX_LAYOUTS],
    used: usize,
    overflow: usize,
    observed: usize,
    transitions: usize,
    previous: Option<Layout>,
    addresses: [usize; SHARDS],
    page_size: usize,
    iterations: usize,
    phase_ns: [u128; 3],
}

impl Trace {
    pub(super) fn for_process(source: &[Vec<u8>], iterations: usize) -> Self {
        let page_size = std::env::var("RSE_SAMPLER_PAGE_SIZE")
            .unwrap()
            .parse()
            .unwrap();
        Self::new(source, iterations, page_size)
    }

    pub(super) fn new(source: &[Vec<u8>], iterations: usize, page_size: usize) -> Self {
        assert_eq!(source.len(), SHARDS);
        assert!(page_size.is_power_of_two() && (4096..=1048576).contains(&page_size));
        Self {
            source: layout(source.iter(), page_size),
            buckets: [Bucket::default(); MAX_LAYOUTS],
            used: 0,
            overflow: 0,
            observed: 0,
            transitions: 0,
            previous: None,
            addresses: [0; SHARDS],
            page_size,
            iterations,
            phase_ns: [0; 3],
        }
    }

    pub(super) fn before(&mut self, slots: &[ShardSlot<Vec<u8>>]) {
        assert_eq!(slots.len(), SHARDS);
        assert!(
            slots
                .iter()
                .enumerate()
                .all(|(i, slot)| slot.is_present() == (i != 0 && i != 4))
        );
        let current = layout(slots.iter().map(ShardSlot::as_inner), self.page_size);
        if self.previous.is_some_and(|last| last != current) {
            self.transitions += 1;
        }
        self.previous = Some(current);
        for (address, slot) in self.addresses.iter_mut().zip(slots) {
            *address = slot.as_inner().as_ptr() as usize;
        }
        self.observed += 1;
        if let Some(bucket) = self.buckets[..self.used]
            .iter_mut()
            .find(|b| b.layout == current)
        {
            bucket.count += 1;
        } else if self.used < MAX_LAYOUTS {
            self.buckets[self.used] = Bucket {
                layout: current,
                count: 1,
            };
            self.used += 1;
        } else {
            self.overflow += 1;
        }
    }

    pub(super) fn after(&self, slots: &[ShardSlot<Vec<u8>>], source: &[Vec<u8>]) {
        assert!(slots.iter().all(ShardSlot::is_present));
        assert_eq!(
            layout(slots.iter().map(ShardSlot::as_inner), self.page_size),
            self.previous.unwrap()
        );
        for (address, slot) in self.addresses.iter().zip(slots) {
            assert_eq!(*address, slot.as_inner().as_ptr() as usize);
        }
        // Check full bytes at both ends; do not claim the observer is cost-free.
        if self.observed == 1 || self.observed == self.iterations {
            assert!(
                slots
                    .iter()
                    .zip(source)
                    .all(|(slot, original)| slot.as_inner() == original)
            );
        }
    }

    pub(super) fn phases(&mut self, clone_ns: u128, reconstruct_ns: u128, drop_ns: u128) {
        for (total, value) in self
            .phase_ns
            .iter_mut()
            .zip([clone_ns, reconstruct_ns, drop_ns])
        {
            *total += value;
        }
    }

    pub(super) fn finish(&self) {
        assert_eq!(self.observed, self.iterations);
        println!("\nRSE_SLOT_LAYOUT {self}");
    }
}

struct JsonLayout<'a>(&'a Layout);
impl fmt::Display for JsonLayout<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_char('[')?;
        for (i, b) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_char(',')?;
            }
            write!(
                f,
                "{{\"len\":{},\"capacity\":{},\"offset_64\":{},\"offset_4096\":{},\"offset_page\":{}}}",
                b.len,
                b.capacity,
                b.offset % 64,
                b.offset % 4096,
                b.offset
            )?;
        }
        f.write_char(']')
    }
}
impl fmt::Display for Trace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{\"entry\":\"Vec<ShardSlot<Vec<u8>>>\",\"source_lifetime\":\"worker\",\"clone_lifetime\":\"iteration\",\"page_size\":{},\"iterations\":{},\"transitions\":{},\"overflow\":{},\"source\":{},\"layouts\":[",
            self.page_size,
            self.iterations,
            self.transitions,
            self.overflow,
            JsonLayout(&self.source)
        )?;
        for (i, bucket) in self.buckets[..self.used].iter().enumerate() {
            if i > 0 {
                f.write_char(',')?;
            }
            write!(
                f,
                "{{\"count\":{},\"buffers\":{}}}",
                bucket.count,
                JsonLayout(&bucket.layout)
            )?;
        }
        write!(
            f,
            "],\"phase_ns\":{{\"clone\":{},\"reconstruct\":{},\"drop\":{}}}}}",
            self.phase_ns[0], self.phase_ns[1], self.phase_ns[2]
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustfs_erasure_codec::galois_8::ReedSolomon;

    #[test]
    fn trace_checks_recovery_identity_and_counts_every_clone() {
        let codec = ReedSolomon::new(4, 2).unwrap();
        let mut source = vec![vec![0x53; 1061]; SHARDS];
        codec.encode(&mut source).unwrap();
        let mut trace = Trace::new(&source, 3, 16384);
        for _ in 0..3 {
            let mut slots: Vec<_> = source.iter().cloned().map(ShardSlot::new_present).collect();
            slots[0].mark_missing();
            slots[4].mark_missing();
            trace.before(&slots);
            codec.reconstruct(&mut slots).unwrap();
            trace.after(&slots, &source);
        }
        assert_eq!(
            trace.overflow
                + trace.buckets[..trace.used]
                    .iter()
                    .map(|b| b.count)
                    .sum::<usize>(),
            3
        );
        trace.phases(7, 11, 13);
        trace.phases(1, 2, 3);
        assert_eq!(trace.phase_ns, [8, 13, 16]);
        let json = trace.to_string();
        assert!(!json.contains("address"));
        assert!(json.contains("\"offset_page\":"));
    }

    #[test]
    fn bounded_histogram_reports_overflow_instead_of_discarding_it() {
        let source = vec![vec![0; 1]; SHARDS];
        let mut trace = Trace::new(&source, 2, 4096);
        trace.used = MAX_LAYOUTS;
        for b in &mut trace.buckets {
            b.layout[0].len = 999;
        }
        let mut slots: Vec<_> = source.into_iter().map(ShardSlot::new_present).collect();
        slots[0].mark_missing();
        slots[4].mark_missing();
        trace.before(&slots);
        trace.before(&slots);
        assert_eq!(trace.overflow, 2);
        assert_eq!(trace.observed, 2);
    }
}
