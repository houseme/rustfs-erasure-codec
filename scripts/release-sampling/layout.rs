//! Address-free metadata for caller-owned buffers. No allocator is replaced.
use std::fmt::{self, Write};

const MAX_BUFFERS: usize = 32;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Position {
    len: usize,
    capacity: usize,
    address: usize,
}

/// Fixed storage avoids observer allocations between retained-fixture samples.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) struct Fixture {
    positions: [Position; MAX_BUFFERS],
    source_count: usize,
    scratch_count: usize,
    page_size: usize,
}

impl Fixture {
    pub(super) fn vec_buffers(source: &[Vec<u8>], scratch: &[Vec<u8>], page_size: usize) -> Self {
        assert!(page_size.is_power_of_two() && (1024..=1048576).contains(&page_size));
        assert!(source.len() + scratch.len() <= MAX_BUFFERS);
        let mut fixture = Self {
            positions: [Position::default(); MAX_BUFFERS],
            source_count: source.len(),
            scratch_count: scratch.len(),
            page_size,
        };
        for (position, buffer) in fixture
            .positions
            .iter_mut()
            .zip(source.iter().chain(scratch))
        {
            assert!(!buffer.is_empty());
            *position = Position {
                len: buffer.len(),
                capacity: buffer.capacity(),
                address: buffer.as_ptr() as usize,
            };
        }
        fixture
    }
}

impl fmt::Debug for Fixture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for Fixture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{\"entry\":\"Vec<Vec<u8>>\",\"lifetime\":\"worker\",\"page_size\":{},\"source\":[",
            self.page_size
        )?;
        let mut positions = self.positions.iter();
        for (name, count) in [
            ("source", self.source_count),
            ("scratch", self.scratch_count),
        ] {
            if name == "scratch" {
                f.write_str("],\"scratch\":[")?;
            }
            for index in 0..count {
                if index != 0 {
                    f.write_char(',')?;
                }
                let p = positions.next().unwrap();
                write!(
                    f,
                    "{{\"len\":{},\"capacity\":{},\"offset_64\":{},\"offset_4096\":{},\"offset_page\":{}}}",
                    p.len,
                    p.capacity,
                    p.address % 64,
                    p.address % 4096,
                    p.address % self.page_size
                )?;
            }
        }
        f.write_str("]}")
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct OptionalFixture(pub(super) Option<Fixture>);

impl fmt::Display for OptionalFixture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(fixture) => fixture.fmt(f),
            None => f.write_str("null"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_vec_metadata_records_shape_and_detects_growth() {
        let source = vec![vec![1; 1024]; 6];
        let mut scratch = vec![vec![0; 1024]; 2];
        let original = Fixture::vec_buffers(&source, &scratch, 4096);
        scratch[0].fill(9);
        assert_eq!(Fixture::vec_buffers(&source, &scratch, 4096), original);
        scratch[0].reserve_exact(4096);
        assert_ne!(Fixture::vec_buffers(&source, &scratch, 4096), original);
        let json = original.to_string();
        assert!(json.contains("\"entry\":\"Vec<Vec<u8>>\""));
        assert!(json.contains("\"offset_4096\":"));
        assert!(!json.contains("address"));
    }
}
