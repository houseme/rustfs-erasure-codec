//! Bounded result metadata; formatting is deferred until all observations finish.
use std::fmt;

const CAPACITY: usize = 8; // Current complete hotspot catalog needs at most four.

#[derive(Clone, Copy)]
pub(super) struct ShardPattern {
    indices: [usize; CAPACITY],
    len: usize,
    data_shards: usize,
}
impl ShardPattern {
    pub(super) fn new(data_shards: usize, indices: &[usize]) -> Self {
        assert!(
            indices.len() <= CAPACITY,
            "hotspot pattern capacity exceeded"
        );
        let mut pattern = Self {
            indices: [0; CAPACITY],
            len: indices.len(),
            data_shards,
        };
        pattern.indices[..indices.len()].copy_from_slice(indices);
        pattern
    }
}
impl fmt::Display for ShardPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.len == 0 {
            return f.write_str("-");
        }
        for (position, &index) in self.indices[..self.len].iter().enumerate() {
            if position != 0 {
                f.write_str("|")?;
            }
            if index < self.data_shards {
                write!(f, "d{index}")?;
            } else {
                write!(f, "p{}", index - self.data_shards)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn patterns_preserve_empty_order_duplicates_and_parity_offsets() {
        assert_eq!(ShardPattern::new(10, &[]).to_string(), "-");
        assert_eq!(ShardPattern::new(10, &[1, 10]).to_string(), "d1|p0");
        assert_eq!(
            ShardPattern::new(4, &[6, 0, 6, 3]).to_string(),
            "p2|d0|p2|d3"
        );
        assert_eq!(
            ShardPattern::new(32, &[0, 2, 4, 6]).to_string(),
            "d0|d2|d4|d6"
        );
    }
    #[test]
    #[should_panic(expected = "capacity exceeded")]
    fn oversized_patterns_fail_instead_of_being_truncated() {
        ShardPattern::new(10, &[0; CAPACITY + 1]);
    }
}
