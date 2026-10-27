//! Shared streaming SHA input. Callers own limits, framing and scratch lifetime.
use crate::Result;
use sha2::{Digest, Sha256};
use std::io::Read;

pub(crate) const BUFFER_LEN: usize = 64 * 1024;

pub(crate) fn update(mut reader: impl Read, hash: &mut Sha256, buffer: &mut [u8]) -> Result<u64> {
    if buffer.is_empty() {
        return Err("nonempty verification buffer required".into());
    }
    let mut total = 0_u64;
    loop {
        let count = match reader.read(buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        };
        total = total
            .checked_add(count as u64)
            .ok_or("fingerprint byte count overflow")?;
        hash.update(&buffer[..count]);
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn short_and_interrupted_reads_preserve_bytes_and_partial_errors_stay_errors() {
        struct Short {
            bytes: std::io::Cursor<Vec<u8>>,
            calls: usize,
            largest_request: usize,
        }
        impl Read for Short {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                self.calls += 1;
                self.largest_request = self.largest_request.max(buffer.len());
                if self.calls % 3 == 1 {
                    return Err(std::io::ErrorKind::Interrupted.into());
                }
                let limit = buffer.len().min(3);
                self.bytes.read(&mut buffer[..limit])
            }
        }
        let original = b"original bytes\r\n\0\xff";
        let mut reader = Short {
            bytes: std::io::Cursor::new(original.to_vec()),
            calls: 0,
            largest_request: 0,
        };
        let mut hash = Sha256::new();
        let mut buffer = [0; 7];
        assert_eq!(
            update(&mut reader, &mut hash, &mut buffer).unwrap(),
            original.len() as u64
        );
        assert_eq!(hash.finalize(), Sha256::digest(original));
        assert_eq!(reader.largest_request, buffer.len());
        struct PartialFailure(bool);
        impl Read for PartialFailure {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                if self.0 {
                    return Err(std::io::ErrorKind::Other.into());
                }
                self.0 = true;
                buffer[0] = 7;
                Ok(1)
            }
        }
        assert!(update(PartialFailure(false), &mut Sha256::new(), &mut buffer).is_err());
        assert!(update(std::io::empty(), &mut Sha256::new(), &mut []).is_err());
    }
}
