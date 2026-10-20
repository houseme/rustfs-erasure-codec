use rustfs_erasure_codec::{Field, galois_8, galois_16};

// Independent polynomial arithmetic: no library log tables or power routine.
fn polynomial_mul8(a: u8, b: u8) -> u8 {
    let mut a = u16::from(a);
    let mut b = u16::from(b);
    let mut product = 0;
    for _ in 0..8 {
        if b & 1 != 0 {
            product ^= a;
        }
        b >>= 1;
        a <<= 1;
        if a & 0x100 != 0 {
            a ^= 0x11d;
        }
    }
    product as u8
}

fn polynomial_mul16(a: [u8; 2], b: [u8; 2]) -> [u8; 2] {
    let high = polynomial_mul8(a[0], b[0]);
    [
        polynomial_mul8(a[0], b[1]) ^ polynomial_mul8(a[1], b[0]) ^ polynomial_mul8(2, high),
        polynomial_mul8(a[1], b[1]) ^ polynomial_mul8(128, high),
    ]
}

fn reference_power<T: Copy>(mut base: T, mut n: usize, one: T, mul: fn(T, T) -> T) -> T {
    let mut result = one;
    while n != 0 {
        if n & 1 != 0 {
            result = mul(result, base);
        }
        base = mul(base, base);
        n >>= 1;
    }
    result
}

#[test]
fn gf8_full_usize_exponents_match_polynomial_reference() {
    // log(4) = 2. Old debug code overflows; old release wraps the product to
    // zero and returns 1 immediately, so this reproducer never waits on a huge loop.
    let high_bit = usize::MAX / 2 + 1;
    assert_eq!(galois_8::exp(4, high_bit), 2);
    for a in 0..=255u8 {
        for n in [
            0,
            1,
            254,
            255,
            256,
            high_bit - 1,
            high_bit,
            usize::MAX - 1,
            usize::MAX,
        ] {
            let expected = reference_power(a, n, 1, polynomial_mul8);
            assert_eq!(galois_8::exp(a, n), expected, "base={a}, exponent={n}");
            assert_eq!(galois_8::Field::exp(a, n), expected);
        }
    }
}

#[test]
fn gf8_every_base_preserves_small_powers_and_period_boundaries() {
    for a in 0..=255u8 {
        let mut expected = 1;
        for n in 0..=512 {
            assert_eq!(galois_8::exp(a, n), expected, "base={a}, exponent={n}");
            expected = polynomial_mul8(expected, a);
        }
    }
}

const GF16_BASES: [[u8; 2]; 16] = [
    [0, 0],
    [0, 1],
    [0, 2],
    [0, 255],
    [1, 0],
    [1, 1],
    [1, 2],
    [2, 0],
    [2, 128],
    [3, 5],
    [7, 19],
    [128, 1],
    [128, 128],
    [255, 0],
    [255, 1],
    [255, 255],
];

#[test]
fn gf16_small_powers_preserve_tower_polynomial_arithmetic() {
    for a in GF16_BASES {
        let mut expected = [0, 1];
        for n in 0..=256 {
            assert_eq!(
                galois_16::Field::exp(a, n),
                expected,
                "base={a:?}, exponent={n}"
            );
            expected = polynomial_mul16(expected, a);
        }
    }
}

#[test]
fn gf16_full_usize_exponents_match_polynomial_reference() {
    let high_bit = usize::MAX / 2 + 1;
    for a in GF16_BASES {
        for n in [
            0,
            1,
            2,
            65534,
            65535,
            65536,
            high_bit - 1,
            high_bit,
            usize::MAX - 1,
            usize::MAX,
        ] {
            let expected = reference_power(a, n, [0, 1], polynomial_mul16);
            assert_eq!(
                galois_16::Field::exp(a, n),
                expected,
                "base={a:?}, exponent={n}"
            );
        }
    }
}
