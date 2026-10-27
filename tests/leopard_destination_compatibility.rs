use rustfs_erasure_codec::{CodecFamily, CodecOptions, Error, galois_8::ReedSolomon};

#[cfg(target_endian = "little")]
const FAMILIES: &[CodecFamily] = &[CodecFamily::LeopardGF8, CodecFamily::LeopardGF16];
#[cfg(target_endian = "big")]
const FAMILIES: &[CodecFamily] = &[CodecFamily::LeopardGF8];

fn fixture(family: CodecFamily) -> (ReedSolomon, Vec<Vec<u8>>) {
    let codec =
        ReedSolomon::with_options(4, 3, CodecOptions::builder().codec_family(family).build())
            .unwrap();
    let mut shards: Vec<Vec<u8>> = (0..7)
        .map(|i| (0..128).map(|j| (i * 31 + j * 17) as u8).collect())
        .collect();
    codec.encode(&mut shards).unwrap();
    (codec, shards)
}

// Both traits are independently implementable; a preflight must not assume
// the read-only length is the same as the destination returned by AsMut.
struct DifferingViews {
    declared: Vec<u8>,
    writable: Vec<u8>,
}

impl AsRef<[u8]> for DifferingViews {
    fn as_ref(&self) -> &[u8] {
        &self.declared
    }
}

impl AsMut<[u8]> for DifferingViews {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.writable
    }
}

#[test]
fn leopard_retains_partial_publication_when_mutable_destination_is_short() {
    for &family in FAMILIES {
        let (codec, original) = fixture(family);
        let mut shards: Vec<_> = original
            .iter()
            .map(|bytes| {
                (
                    DifferingViews {
                        declared: bytes.clone(),
                        writable: bytes.clone(),
                    },
                    true,
                )
            })
            .collect();
        shards[0].1 = false;
        shards[0].0.writable.fill(0xa5);
        shards[1].1 = false;
        shards[1].0.writable.truncate(64);
        assert_eq!(
            codec.reconstruct(&mut shards),
            Err(Error::IncorrectShardSize)
        );
        assert!(shards[0].1);
        assert_eq!(shards[0].0.writable, original[0]);
        assert!(!shards[1].1);
        assert_eq!(shards[1].0.writable, original[1][..64]);
    }
}

struct ShortCollection(Vec<u8>);

impl AsRef<[u8]> for ShortCollection {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl AsMut<[u8]> for ShortCollection {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}
impl FromIterator<u8> for ShortCollection {
    fn from_iter<I: IntoIterator<Item = u8>>(iter: I) -> Self {
        Self(iter.into_iter().take(64).collect())
    }
}

#[test]
fn leopard_checks_actual_option_storage_after_initialization() {
    for &family in FAMILIES {
        let (codec, original) = fixture(family);
        let mut shards: Vec<_> = original
            .into_iter()
            .map(|bytes| Some(ShortCollection(bytes)))
            .collect();
        shards[0] = None;
        assert_eq!(
            codec.reconstruct(&mut shards),
            Err(Error::IncorrectShardSize)
        );
        assert_eq!(shards[0].as_ref().unwrap().0.len(), 64);
    }
}
