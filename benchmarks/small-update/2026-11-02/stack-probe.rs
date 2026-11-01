use codec::{Field, ReedSolomon};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wide([u8; 256]);
impl Default for Wide {
    fn default() -> Self {
        Self([0; 256])
    }
}

pub struct WideField;
impl WideField {
    fn wrap(x: u8) -> Wide {
        Wide([x; 256])
    }
}
impl Field for WideField {
    const ORDER: usize = 256;
    type Elem = Wide;
    fn add(a: Wide, b: Wide) -> Wide {
        Self::wrap(codec::galois_8::add(a.0[0], b.0[0]))
    }
    fn mul(a: Wide, b: Wide) -> Wide {
        Self::wrap(codec::galois_8::mul(a.0[0], b.0[0]))
    }
    fn div(a: Wide, b: Wide) -> Wide {
        Self::wrap(codec::galois_8::div(a.0[0], b.0[0]))
    }
    fn exp(a: Wide, n: usize) -> Wide {
        Self::wrap(codec::galois_8::exp(a.0[0], n))
    }
    fn zero() -> Wide {
        Self::wrap(0)
    }
    fn one() -> Wide {
        Self::wrap(1)
    }
    fn nth_internal(n: usize) -> Wide {
        Self::wrap(n as u8)
    }
}

#[inline(never)]
#[unsafe(no_mangle)]
pub fn update_wide(
    codec: &ReedSolomon<WideField>,
    data: &[&[Wide]],
    changes: &[Option<&[Wide]>],
    parity: &mut [Vec<Wide>],
) -> Result<(), codec::Error> {
    codec.update(data, changes, parity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_field_updates_match_full_encode() {
        let codec = ReedSolomon::<WideField>::new(4, 2).unwrap();
        for len in [1, 64, 256, 257] {
            let original: Vec<Vec<Wide>> = (0..4)
                .map(|row| {
                    (0..len)
                        .map(|i| WideField::nth((row * 37 + i * 13) % 256))
                        .collect()
                })
                .collect();
            let mut changed = original.clone();
            changed[3][len - 1] = WideField::add(changed[3][len - 1], WideField::one());
            let mut parity = vec![vec![WideField::zero(); len]; 2];
            let mut expected = parity.clone();
            codec.encode_sep(&original, &mut parity).unwrap();
            codec.encode_sep(&changed, &mut expected).unwrap();
            let refs: Vec<_> = original.iter().map(Vec::as_slice).collect();
            let changes = [None, None, None, Some(changed[3].as_slice())];
            assert_eq!(update_wide(&codec, &refs, &changes, &mut parity), Ok(()));
            assert_eq!(parity, expected, "len={len}");
        }
    }
}
