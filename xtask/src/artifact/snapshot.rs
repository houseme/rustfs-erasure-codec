//! Bounded original members, hashed from the same bytes that are decoded.
use crate::Result;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

pub(crate) const MEMBER_LIMIT: u64 = 32 * 1024 * 1024;
const SNAPSHOT_LIMIT: u64 = 256 * 1024 * 1024;

pub(crate) struct Snapshot {
    root: PathBuf,
    total: u64,
    limit: u64,
    members: BTreeMap<String, String>,
}
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
pub(crate) fn universal_newlines(text: String) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text
    }
}
/// Hash the bounded original UTF-8 bytes before applying text newline handling.
pub(crate) fn text_member(path: &Path, expected: &str) -> Result<String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .and_then(|p| p.to_str())
        .ok_or("invalid original member name")?;
    let bytes = Snapshot::new(parent)?.bytes(name)?;
    if digest(&bytes) != expected {
        return Err("original member digest mismatch".into());
    }
    Ok(universal_newlines(String::from_utf8(bytes)?))
}
impl Snapshot {
    pub(crate) fn new(root: &Path) -> Result<Self> {
        Self::with_budget(root, SNAPSHOT_LIMIT)
    }
    /// Fixed member catalogs can retain the original per-member acceptance budget.
    pub(crate) fn with_budget(root: &Path, limit: u64) -> Result<Self> {
        Ok(Self {
            root: fs::canonicalize(root)?,
            total: 0,
            limit,
            members: BTreeMap::new(),
        })
    }
    fn member(&self, name: &str) -> Result<PathBuf> {
        let mut path = self.root.clone();
        let parts: Vec<_> = Path::new(name).components().collect();
        if parts.is_empty() {
            return Err("regular original member required".into());
        }
        for (index, part) in parts.iter().enumerate() {
            let Component::Normal(part) = part else {
                return Err("invalid original member path".into());
            };
            path.push(part);
            let meta = fs::symlink_metadata(&path)?;
            if meta.file_type().is_symlink() {
                return Err(
                    "bounded regular original members required (symlinked original member)".into(),
                );
            }
            if if index + 1 == parts.len() {
                !meta.is_file()
            } else {
                !meta.is_dir()
            } {
                return Err("bounded regular original members required".into());
            }
        }
        Ok(path)
    }
    fn bytes(&self, name: &str) -> Result<Vec<u8>> {
        let path = self.member(name)?;
        let file = fs::File::open(path)?;
        let len = file.metadata()?.len();
        if len == 0 || len > MEMBER_LIMIT {
            return Err("bounded regular original members required".into());
        }
        let mut bytes = Vec::new();
        file.take(MEMBER_LIMIT + 1).read_to_end(&mut bytes)?;
        if bytes.is_empty() || bytes.len() as u64 > MEMBER_LIMIT {
            return Err("original member changed beyond byte budget".into());
        }
        Ok(bytes)
    }
    pub(crate) fn json(&mut self, name: &str) -> Result<Value> {
        self.json_bound(name, None)
    }
    pub(crate) fn json_bound(&mut self, name: &str, expected: Option<&str>) -> Result<Value> {
        self.bound(name, expected, |bytes| {
            let text = json_text(&bytes)?;
            let super::StrictJson(value) = serde_json::from_str(&text)?;
            Ok(value)
        })
    }
    pub(crate) fn python_json_bound(
        &mut self,
        name: &str,
        expected: Option<&str>,
    ) -> Result<Value> {
        self.bound(name, expected, |bytes| {
            super::python_json::parse(&universal_newlines(String::from_utf8(bytes)?))
        })
    }
    pub(crate) fn text_bound(&mut self, name: &str, expected: Option<&str>) -> Result<String> {
        self.bound(name, expected, |bytes| {
            Ok(universal_newlines(String::from_utf8(bytes)?))
        })
    }
    fn bound<T>(
        &mut self,
        name: &str,
        expected: Option<&str>,
        decode: impl FnOnce(Vec<u8>) -> Result<T>,
    ) -> Result<T> {
        if self.members.contains_key(name) {
            return Err("original member must be read once".into());
        }
        let bytes = self.bytes(name)?;
        self.total = self
            .total
            .checked_add(bytes.len() as u64)
            .ok_or("snapshot byte budget overflow")?;
        if self.total > self.limit {
            return Err("original snapshot exceeds byte budget".into());
        }
        let hash = digest(&bytes);
        if expected.is_some_and(|expected| expected != hash) {
            return Err("original member digest mismatch".into());
        }
        let value = decode(bytes).map_err(|e| format!("{name}: {e}"))?;
        self.members.insert(name.into(), hash);
        Ok(value)
    }
    pub(crate) fn finish(self) -> Result<BTreeMap<String, String>> {
        for (name, expected) in &self.members {
            if digest(&self.bytes(name)?) != *expected {
                return Err("original members changed during replay".into());
            }
        }
        Ok(self.members)
    }
}

// Match the UTF-8/16/32 encoding detection used by the former json.loads(bytes).
fn json_text(bytes: &[u8]) -> Result<std::borrow::Cow<'_, str>> {
    use std::borrow::Cow;
    let (width, little, skip) = if bytes.starts_with(&[0, 0, 254, 255]) {
        (4, false, 4)
    } else if bytes.starts_with(&[255, 254, 0, 0]) {
        (4, true, 4)
    } else if bytes.starts_with(&[254, 255]) {
        (2, false, 2)
    } else if bytes.starts_with(&[255, 254]) {
        (2, true, 2)
    } else if bytes.starts_with(&[239, 187, 191]) {
        (1, false, 3)
    } else if bytes.len() >= 4 && bytes[0] == 0 {
        if bytes[1] == 0 {
            (4, false, 0)
        } else {
            (2, false, 0)
        }
    } else if bytes.len() >= 4 && bytes[1] == 0 {
        if bytes[2] == 0 && bytes[3] == 0 {
            (4, true, 0)
        } else {
            (2, true, 0)
        }
    } else if bytes.len() == 2 && bytes[0] == 0 {
        (2, false, 0)
    } else if bytes.len() == 2 && bytes[1] == 0 {
        (2, true, 0)
    } else {
        (1, false, 0)
    };
    let raw = &bytes[skip..];
    if width == 1 {
        return Ok(Cow::Borrowed(std::str::from_utf8(raw)?));
    }
    if !raw.len().is_multiple_of(width) {
        return Err("incomplete JSON encoding unit".into());
    }
    let mut text = String::new();
    if width == 2 {
        let units = raw.as_chunks::<2>().0.iter().map(|v| {
            if little {
                u16::from_le_bytes(*v)
            } else {
                u16::from_be_bytes(*v)
            }
        });
        for c in char::decode_utf16(units) {
            text.push(c?);
        }
    } else {
        for v in raw.as_chunks::<4>().0 {
            let n = if little {
                u32::from_le_bytes(*v)
            } else {
                u32::from_be_bytes(*v)
            };
            text.push(char::from_u32(n).ok_or("invalid JSON Unicode scalar")?);
        }
    }
    Ok(Cow::Owned(text))
}

#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
mod text_member_tests {
    use super::*;
    #[test]
    fn original_text_hashes_precede_newline_decoding_and_reject_changed_or_unbounded_members() {
        let path = std::env::temp_dir().join(format!("rse-native-text-{}", std::process::id()));
        let bytes = b"first\r\nsecond\rthird\n";
        fs::write(&path, bytes).unwrap();
        assert_eq!(
            text_member(&path, &digest(bytes)).unwrap(),
            "first\nsecond\nthird\n"
        );
        assert!(text_member(&path, &"f".repeat(64)).is_err());
        fs::write(&path, b"changed").unwrap();
        assert!(text_member(&path, &digest(bytes)).is_err());
        fs::write(&path, []).unwrap();
        assert!(text_member(&path, &digest(&[])).is_err());
        fs::File::create(&path)
            .unwrap()
            .set_len(MEMBER_LIMIT + 1)
            .unwrap();
        assert!(text_member(&path, &"0".repeat(64)).is_err());
        fs::remove_file(&path).unwrap();
    }
}
