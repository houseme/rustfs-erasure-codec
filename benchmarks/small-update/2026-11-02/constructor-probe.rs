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
pub fn construct_wide() -> Result<ReedSolomon<WideField>, codec::Error> {
    ReedSolomon::<WideField>::new(4, 2)
}
