#[inline(never)]
#[unsafe(no_mangle)]
pub fn rse_stack_gf8(codec: &codec::galois_8::ReedSolomon, shards: &[Vec<u8>]) -> bool {
    codec.verify(shards).unwrap()
}
#[inline(never)]
#[unsafe(no_mangle)]
pub fn rse_stack_gf16(codec: &codec::galois_16::ReedSolomon, shards: &[Vec<[u8; 2]>]) -> bool {
    codec.verify(shards).unwrap()
}

pub struct ThreeByteField;
impl ThreeByteField {
    fn wrap(value: u8) -> [u8; 3] {
        [value, value.rotate_left(1), value.rotate_left(3)]
    }
}
impl codec::Field for ThreeByteField {
    const ORDER: usize = 256;
    type Elem = [u8; 3];

    fn add(a: Self::Elem, b: Self::Elem) -> Self::Elem {
        Self::wrap(codec::galois_8::Field::add(a[0], b[0]))
    }
    fn mul(a: Self::Elem, b: Self::Elem) -> Self::Elem {
        Self::wrap(codec::galois_8::Field::mul(a[0], b[0]))
    }
    fn div(a: Self::Elem, b: Self::Elem) -> Self::Elem {
        Self::wrap(codec::galois_8::Field::div(a[0], b[0]))
    }
    fn exp(value: Self::Elem, n: usize) -> Self::Elem {
        Self::wrap(codec::galois_8::Field::exp(value[0], n))
    }
    fn zero() -> Self::Elem {
        Self::wrap(codec::galois_8::Field::zero())
    }
    fn one() -> Self::Elem {
        Self::wrap(codec::galois_8::Field::one())
    }
    fn nth_internal(n: usize) -> Self::Elem {
        Self::wrap(codec::galois_8::Field::nth_internal(n))
    }
}

#[inline(never)]
#[unsafe(no_mangle)]
pub fn rse_stack_gf24(codec: &codec::ReedSolomon<ThreeByteField>, shards: &[Vec<[u8; 3]>]) -> bool {
    codec.verify(shards).unwrap()
}
