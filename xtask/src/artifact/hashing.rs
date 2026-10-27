//! Shared SHA input and compact JSON hashing. Callers own limits and framing.
use crate::Result;
use sha2::{Digest, Sha256};
use std::io::{BufWriter, Read, Write};

pub(crate) const BUFFER_LEN: usize = 64 * 1024;
const JSON_BUFFER_LEN: usize = 8 * 1024;

pub(crate) fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        text.push(HEX[(b >> 4) as usize] as char);
        text.push(HEX[(b & 15) as usize] as char);
    }
    text
}
pub(crate) fn digest(bytes: &[u8]) -> String {
    hex(Sha256::digest(bytes).as_slice())
}
struct DigestWriter<'a>(&'a mut Sha256);
impl Write for DigestWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn write_compact<T: serde::Serialize + ?Sized>(value: &T, writer: impl Write) -> Result<()> {
    let mut writer = BufWriter::with_capacity(JSON_BUFFER_LEN, writer);
    serde_json::to_writer(&mut writer, value)?;
    writer.flush()?;
    Ok(())
}
/// Hash exactly to_vec's compact bytes without retaining the complete encoding.
pub(crate) fn compact_digest<T: serde::Serialize + ?Sized>(value: &T) -> Result<String> {
    let mut hash = Sha256::new();
    write_compact(value, DigestWriter(&mut hash))?;
    Ok(hex(hash.finalize().as_slice()))
}

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
    fn compact_digests_match_legacy_bytes_across_boundaries_and_number_encodings() {
        for length in [0, 1, 8191, 8192, 8193, 3 * 8192 + 17] {
            let value = serde_json::json!({"payload":"x".repeat(length),"unicode":"雪\r\n\"quoted\"", "nested":[null,true,-0.0,{"z":1,"a":2}]});
            assert_eq!(
                compact_digest(&value).unwrap(),
                digest(&serde_json::to_vec(&value).unwrap())
            );
        }
        for value in [0.0, -0.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                compact_digest(&value).unwrap(),
                digest(&serde_json::to_vec(&value).unwrap())
            );
        }
    }
    #[test]
    fn compact_writer_emits_before_serialization_finishes_and_flushes_the_tail() {
        use serde::ser::SerializeSeq;
        use std::cell::Cell;
        struct Rows<'a>(&'a Cell<usize>);
        impl serde::Serialize for Rows<'_> {
            fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                let mut sequence = serializer.serialize_seq(Some(10_000))?;
                for index in 0..10_000 {
                    sequence.serialize_element(&index)?;
                    self.0.set(index + 1);
                }
                sequence.end()
            }
        }
        struct Recorder<'a> {
            progress: &'a Cell<usize>,
            first_write: Option<usize>,
            bytes: Vec<u8>,
            flushed: bool,
        }
        impl Write for Recorder<'_> {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.first_write.get_or_insert(self.progress.get());
                self.bytes.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.flushed = true;
                Ok(())
            }
        }
        let progress = Cell::new(0);
        let rows = Rows(&progress);
        let expected = serde_json::to_vec(&rows).unwrap();
        progress.set(0);
        let mut recorder = Recorder {
            progress: &progress,
            first_write: None,
            bytes: Vec::new(),
            flushed: false,
        };
        write_compact(&rows, &mut recorder).unwrap();
        assert!(recorder.first_write.unwrap() < 10_000);
        assert!(recorder.flushed);
        assert_eq!(recorder.bytes, expected);
    }
    #[test]
    fn compact_serializer_and_flush_failures_do_not_return_partial_pins() {
        use serde::ser::SerializeSeq;
        struct Failed;
        impl serde::Serialize for Failed {
            fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                let mut sequence = serializer.serialize_seq(None)?;
                for index in 0..5_000 {
                    sequence.serialize_element(&index)?;
                }
                Err(<S::Error as serde::ser::Error>::custom(
                    "injected serialization failure",
                ))
            }
        }
        let expected = serde_json::to_vec(&Failed).unwrap_err();
        assert_eq!(
            compact_digest(&Failed).unwrap_err().to_string(),
            expected.to_string()
        );
        struct FlushFailure;
        impl Write for FlushFailure {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Err(std::io::ErrorKind::Other.into())
            }
        }
        assert!(write_compact(&1_u8, FlushFailure).is_err());
        assert_eq!(
            compact_digest(&"next").unwrap(),
            digest(&serde_json::to_vec("next").unwrap())
        );
    }
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
