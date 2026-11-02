//! Generate the targeted 72-case Leopard fuzz smoke-test seed corpus.
use std::error::Error;
use std::fs;
use std::path::PathBuf;

fn seed(family: u8, data: u8, parity: u8, length_index: u8, api: u8) -> [u8; 20] {
    let mut bytes = [0; 20];
    bytes[0] = family;
    bytes[3] = data - 1;
    bytes[4] = parity - 1;
    bytes[5] = length_index;
    bytes[7] = api;
    bytes[8] = parity.min(2);
    bytes[10..12].copy_from_slice(&[0xff, 0xff]);
    bytes[12] = 1;
    bytes[14] = data - 1;
    bytes[15] = 3;
    bytes[16..].copy_from_slice(&[0x11, 0x27, 0x5a, 0xc3]);
    bytes
}

fn main() -> Result<(), Box<dyn Error>> {
    let output = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("missing output directory")?,
    );
    fs::create_dir_all(&output)?;
    let mut index = 0;
    for family in [1, 2] {
        for data in [4, 8] {
            for parity in [2, 4] {
                for length_index in 0..3 {
                    for api in 0..3 {
                        fs::write(
                            output.join(format!("seed-{index:03}")),
                            seed(family, data, parity, length_index, api),
                        )?;
                        index += 1;
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::seed;
    use std::collections::HashSet;

    #[test]
    fn seed_catalog_has_expected_boundaries_and_unique_records() {
        let first = [
            1, 0, 0, 3, 1, 0, 0, 0, 2, 0, 255, 255, 1, 0, 3, 3, 17, 39, 90, 195,
        ];
        let last = [
            2, 0, 0, 7, 3, 2, 0, 2, 2, 0, 255, 255, 1, 0, 7, 3, 17, 39, 90, 195,
        ];
        assert_eq!(seed(1, 4, 2, 0, 0), first);
        assert_eq!(seed(2, 8, 4, 2, 2), last);
        let mut records = HashSet::new();
        for family in [1, 2] {
            for data in [4, 8] {
                for parity in [2, 4] {
                    for length_index in 0..3 {
                        for api in 0..3 {
                            records.insert(seed(family, data, parity, length_index, api));
                        }
                    }
                }
            }
        }
        assert_eq!(records.len(), 72);
    }
}
