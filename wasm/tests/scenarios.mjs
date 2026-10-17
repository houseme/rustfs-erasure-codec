export function check(condition, message) {
    if (!condition) throw new Error(message);
}

export function equalBytes(actual, expected, label) {
    check(actual.length === expected.length, `${label}: length mismatch`);
    for (let index = 0; index < actual.length; index += 1) {
        if (actual[index] !== expected[index]) throw new Error(`${label}: byte ${index}`);
    }
}

function gf8Multiply(left, right) {
    let product = 0;
    while (right !== 0) {
        if (right & 1) product ^= left;
        right >>>= 1;
        left <<= 1;
        if (left & 0x100) left ^= 0x11d;
    }
    return product;
}

// Interpolating systematic Vandermonde data at x=0,1,2 and parity at
// x=3,4 gives these rows. This scalar oracle uses no Rust/WASM encoder.
const parityTables = [[1, 1, 1], [15, 8, 6]].map((row) =>
    row.map((coefficient) => Uint8Array.from({ length: 256 }, (_, value) => gf8Multiply(coefficient, value))));

function checkAllParity(shards, data, shardSize) {
    for (let row = 0; row < parityTables.length; row += 1) {
        for (let offset = 0; offset < shardSize; offset += 1) {
            let expected = 0;
            for (let column = 0; column < 3; column += 1) {
                expected ^= parityTables[row][column][data[column * shardSize + offset]];
            }
            if (shards[(3 + row) * shardSize + offset] !== expected) {
                throw new Error(`independent parity oracle: row ${row}, byte ${offset}`);
            }
        }
    }
}

export function exerciseGoldenVector(codec) {
    // Fixed 4+2 vector from tests/golden_vectors.rs, with 16-byte shards.
    const shards = new Uint8Array(96);
    for (let index = 0; index < 64; index += 1) shards[index] = index;
    check(codec.encode(shards, 4, 2) === 0, "golden encode must succeed");
    equalBytes(shards, Uint8Array.from({ length: 96 }, (_, index) => index), "fixed 4+2 parity vector");
}

export function exerciseRoundTrip(codec, { shardSize = 257, subarray = false, seed = 0 } = {}) {
    const dataShards = 3;
    const parityShards = 2;
    const totalBytes = (dataShards + parityShards) * shardSize;
    const prefix = subarray ? 7 : 0;
    const suffix = subarray ? 11 : 0;
    const storage = new Uint8Array(prefix + totalBytes + suffix).fill(0xa5);
    const shards = storage.subarray(prefix, prefix + totalBytes);
    shards.fill(0);
    for (let index = 0; index < dataShards * shardSize; index += 1) {
        shards[index] = (index * 17 + (index >> 5) + seed * 23) % 251;
    }
    const data = shards.slice(0, dataShards * shardSize);
    check(codec.encode(shards, dataShards, parityShards) === 0, "encode must succeed");
    equalBytes(shards.subarray(0, data.length), data, "encoding preserves data");
    checkAllParity(shards, data, shardSize);
    const encoded = shards.slice();
    shards.fill(0xcc, 0, shardSize);
    shards.fill(0xdd, 4 * shardSize);
    check(codec.reconstruct(shards, dataShards, parityShards, [false, true, true, true, false]) === 0,
        "data reconstruction must succeed");
    equalBytes(shards.subarray(0, data.length), data, "recovered data");
    equalBytes(shards.subarray(3 * shardSize, 4 * shardSize), encoded.subarray(3 * shardSize, 4 * shardSize),
        "present parity is unchanged");
    check(shards.subarray(4 * shardSize).every((value) => value === 0xdd), "missing parity stays unchanged");
    check(storage.subarray(0, prefix).every((value) => value === 0xa5), "subarray prefix is unchanged");
    check(storage.subarray(prefix + totalBytes).every((value) => value === 0xa5), "subarray suffix is unchanged");
}

export function exerciseInputValidation(codec) {
    for (const [data, parity, expected] of [
        [0, 1, 3], [1, 0, 5], [2.5, 1, 3], [2, 1.5, 5],
        [NaN, 1, 3], [1, Infinity, 5], [-1, 1, 3],
        [Number.MAX_SAFE_INTEGER + 1, 1, 3], [4294967298, 1, 2], [256, 1, 2],
    ]) {
        const shards = new Uint8Array(12).fill(7);
        check(codec.encode(shards, data, parity) === expected, "invalid encode count code");
        check(codec.reconstruct(shards, data, parity, []) === expected, "invalid reconstruct count code");
        check(shards.every((value) => value === 7), "invalid counts preserve input");
    }
    for (const length of [0, 1, 2, 4, 5, 7, 13]) {
        const shards = new Uint8Array(length).fill(7);
        const expected = length === 0 ? 11 : 9;
        check(codec.encode(shards, 2, 1) === expected, "invalid encode shape code");
        check(codec.reconstruct(shards, 2, 1, [true, true, true]) === expected, "invalid reconstruct shape code");
        check(shards.every((value) => value === 7), "invalid shapes preserve input");
    }
    for (const flags of [[], [true, true], [true, true, true, true], [true, 2, true], new Array(3)]) {
        const shards = new Uint8Array(12).fill(7);
        check(codec.reconstruct(shards, 2, 1, flags) === 12, "invalid flags code");
        check(shards.every((value) => value === 7), "invalid flags preserve input");
    }
}

export function exerciseInvalidInputs(codec) {
    exerciseInputValidation(codec);
    const insufficient = new Uint8Array(12).fill(7);
    check(codec.reconstruct(insufficient, 2, 1, [false, false, true]) === 10, "too few present shards code");
    check(insufficient.every((value) => value === 7), "insufficient input preserves bytes");
}

// White-box access is used only to grow the real generated instance's memory;
// no mocked exports or alternate WASM instance participate in these tests.
export function codecMemory(codec, bytes) {
    return Reflect.get(codec, "bindings").initSync({ module: bytes }).memory;
}

export function exerciseMemoryGrowth(codec, bytes) {
    exerciseRoundTrip(codec);
    const memory = codecMemory(codec, bytes);
    const originalSize = memory.buffer.byteLength;
    memory.grow(2);
    check(memory.buffer.byteLength > originalSize, "explicit memory.grow must increase memory");
    exerciseRoundTrip(codec, { subarray: true, seed: 3 });
    const beforeAllocation = memory.buffer.byteLength;
    const shardSize = Math.max(1024 * 1024, Math.ceil(beforeAllocation / 5) + 65536);
    exerciseRoundTrip(codec, { shardSize, seed: 5 });
    check(memory.buffer.byteLength > beforeAllocation, "large input must trigger WASM allocator growth");
    exerciseRoundTrip(codec, { seed: 7 });
}

export function exerciseRepeatedCalls(codec) {
    for (let iteration = 0; iteration < 100; iteration += 1) {
        exerciseRoundTrip(codec, { seed: iteration, subarray: iteration % 2 === 0 });
    }
}
