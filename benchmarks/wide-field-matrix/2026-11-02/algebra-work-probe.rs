use rustfs_erasure_codec::{Field, ReedSolomon, galois_8};
use std::cell::Cell;

// Counts and an order-sensitive rolling witness are diagnostic only.
thread_local! {
    static WORK: Cell<([u64; 10], u64)> = const { Cell::new(([0; 10], 0xcbf29ce484222325)) };
}
fn record(index: usize) {
    WORK.with(|work| {
        let (mut counts, hash) = work.get();
        counts[index] += 1;
        work.set((counts, hash.wrapping_mul(0x100000001b3) ^ index as u64));
    });
}

struct ScalarField;
struct ByteViewField;
macro_rules! field_impl {
    ($field:ty $(, $extra:item)* $(,)?) => {
        impl Field for $field {
            const ORDER: usize = 256;
            type Elem = u8;
            fn add(a:u8,b:u8)->u8 { record(0); galois_8::add(a,b) }
            fn mul(a:u8,b:u8)->u8 { record(1); galois_8::mul(a,b) }
            fn div(a:u8,b:u8)->u8 { record(2); galois_8::div(a,b) }
            fn exp(a:u8,n:usize)->u8 { record(3); galois_8::exp(a,n) }
            fn zero()->u8 { record(4); 0 }
            fn one()->u8 { record(5); 1 }
            fn nth_internal(n:usize)->u8 { record(6); n as u8 }
            $($extra)*
        }
    };
}
field_impl!(ScalarField);
field_impl!(
    ByteViewField,
    fn as_gf8_slice(slice: &[u8]) -> Option<&[u8]> {
        record(7);
        Some(slice)
    },
    fn as_gf8_slice_mut(slice: &mut [u8]) -> Option<&mut [u8]> {
        record(8);
        Some(slice)
    }
);

fn check<F: Field<Elem = u8>>(name: &str) {
    for data in [22, 23, 24, 32] {
        for parity in [2, 16] {
            WORK.with(|work| work.set(([0; 10], 0xcbf29ce484222325)));
            let codec = ReedSolomon::<F>::new(data, parity).unwrap();
            let (counts, hash) = WORK.with(Cell::get);
            let reference = galois_8::ReedSolomon::new(data, parity).unwrap();
            for row in 0..data + parity {
                assert_eq!(
                    codec.encoding_format().matrix_row(row),
                    reference.encoding_format().matrix_row(row),
                    "field={name}, data={data}, parity={parity}, row={row}"
                );
            }
            println!(
                "ALGEBRA_WORK field={name} data={data} parity={parity} counts={counts:?} sequence={hash:016x}"
            );
        }
    }
}

#[derive(Copy, Debug, Default, PartialEq)]
struct CloneElem(u8);
impl Clone for CloneElem {
    fn clone(&self) -> Self {
        record(9);
        *self
    }
}
struct CloneField;
impl Field for CloneField {
    const ORDER: usize = 256;
    type Elem = CloneElem;
    fn add(a: CloneElem, b: CloneElem) -> CloneElem {
        record(0);
        CloneElem(galois_8::add(a.0, b.0))
    }
    fn mul(a: CloneElem, b: CloneElem) -> CloneElem {
        record(1);
        CloneElem(galois_8::mul(a.0, b.0))
    }
    fn div(a: CloneElem, b: CloneElem) -> CloneElem {
        record(2);
        CloneElem(galois_8::div(a.0, b.0))
    }
    fn exp(a: CloneElem, n: usize) -> CloneElem {
        record(3);
        CloneElem(galois_8::exp(a.0, n))
    }
    fn zero() -> CloneElem {
        record(4);
        CloneElem(0)
    }
    fn one() -> CloneElem {
        record(5);
        CloneElem(1)
    }
    fn nth_internal(n: usize) -> CloneElem {
        record(6);
        CloneElem(n as u8)
    }
}
fn check_clone() {
    for data in [22, 23, 24, 32] {
        for parity in [2, 16] {
            WORK.with(|work| work.set(([0; 10], 0xcbf29ce484222325)));
            let codec = ReedSolomon::<CloneField>::new(data, parity).unwrap();
            let (counts, hash) = WORK.with(Cell::get);
            let reference = galois_8::ReedSolomon::new(data, parity).unwrap();
            for row in 0..data + parity {
                let actual: Vec<_> = codec
                    .encoding_format()
                    .matrix_row(row)
                    .unwrap()
                    .iter()
                    .map(|value| value.0)
                    .collect();
                assert_eq!(actual, reference.encoding_format().matrix_row(row).unwrap());
            }
            println!(
                "ALGEBRA_WORK field=clone data={data} parity={parity} counts={counts:?} sequence={hash:016x}"
            );
        }
    }
}

#[test]
fn construction_preserves_matrix_and_observes_field_operation_order() {
    check::<ScalarField>("scalar");
    check::<ByteViewField>("byte-view");
    check_clone();
}
