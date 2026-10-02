import assert from "node:assert/strict";
import test from "node:test";

import { ReedSolomonErasure } from "../src/index.ts";

// A tiny real WASM module exposes the i32 boundary used by wasm32 usize.
const coercionModule = new WebAssembly.Module(new Uint8Array([
    0, 97, 115, 109, 1, 0, 0, 0, 1, 6, 1, 96, 1, 127, 1, 127,
    3, 2, 1, 0, 7, 10, 1, 6, 99, 111, 101, 114, 99, 101, 0, 0,
    10, 6, 1, 4, 0, 32, 0, 11,
]));
const coerce = new WebAssembly.Instance(coercionModule).exports.coerce;

function fixture() {
    const memory = new WebAssembly.Memory({ initial: 1 });
    const calls = { allocations: [], frees: [], encode: [], reconstruct: [] };
    let nextPointer = 64;
    const exports = {
        memory,
        __wbindgen_malloc(length) {
            calls.allocations.push(length);
            const pointer = nextPointer;
            nextPointer += length;
            return pointer;
        },
        __wbindgen_free(pointer, length) {
            calls.frees.push([pointer, length]);
        },
        encode(pointer, length, data, parity) {
            data = coerce(data) >>> 0;
            parity = coerce(parity) >>> 0;
            calls.encode.push([data, parity]);
            const shardSize = length / (data + parity);
            new Uint8Array(memory.buffer).fill(99, pointer + shardSize * data, pointer + length);
            return ReedSolomonErasure.RESULT_OK;
        },
        reconstruct(pointer, length, data, parity, flagsPointer, flagsLength) {
            data = coerce(data) >>> 0;
            parity = coerce(parity) >>> 0;
            calls.reconstruct.push([
                data, parity,
                [...new Uint8Array(memory.buffer, flagsPointer, flagsLength)],
            ]);
            const shardSize = length / (data + parity);
            new Uint8Array(memory.buffer).fill(42, pointer, pointer + shardSize * data);
            return ReedSolomonErasure.RESULT_OK;
        },
    };
    // Constructor privacy is a TypeScript API constraint; the mock substitutes
    // exports only to exercise the production wrapper without a package build.
    const codec = Reflect.construct(ReedSolomonErasure, [exports]);
    return { codec, calls };
}

test("invalid counts are rejected before WASM allocation or integer coercion", () => {
    for (const value of [NaN, Infinity, -Infinity, 0, -1, 2.5, Number.MAX_SAFE_INTEGER + 1, 4294967298]) {
        for (const field of ["data", "parity"]) {
            const { codec, calls } = fixture();
            const shards = new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0]);
            const before = shards.slice();
            const data = field === "data" ? value : 2;
            const parity = field === "parity" ? value : 1;
            const expected = value === 4294967298
                ? ReedSolomonErasure.RESULT_ERROR_TOO_MANY_SHARDS
                : field === "data"
                    ? ReedSolomonErasure.RESULT_ERROR_TOO_FEW_DATA_SHARDS
                    : ReedSolomonErasure.RESULT_ERROR_TOO_FEW_PARITY_SHARDS;
            assert.equal(codec.encode(shards, data, parity), expected);
            assert.equal(codec.reconstruct(shards, data, parity, [false, true, true]), expected);
            assert.deepEqual(shards, before);
            assert.deepEqual(calls, { allocations: [], frees: [], encode: [], reconstruct: [] });
        }
    }
    assert.equal(coerce(4294967298), 2, "regression fixture must demonstrate wasm32 wrapping");
});

test("total count and buffer shape are checked before allocation", () => {
    for (const [length, data, parity, expected] of [
        [12, 256, 1, ReedSolomonErasure.RESULT_ERROR_TOO_MANY_SHARDS],
        [12, 1, 256, ReedSolomonErasure.RESULT_ERROR_TOO_MANY_SHARDS],
        [0, 2, 1, ReedSolomonErasure.RESULT_ERROR_EMPTY_SHARD],
        [2, 2, 1, ReedSolomonErasure.RESULT_ERROR_INCORRECT_SHARD_SIZE],
        [13, 2, 1, ReedSolomonErasure.RESULT_ERROR_INCORRECT_SHARD_SIZE],
    ]) {
        const { codec, calls } = fixture();
        const shards = new Uint8Array(length).fill(7);
        assert.equal(codec.encode(shards, data, parity), expected);
        assert.equal(codec.reconstruct(shards, data, parity, []), expected);
        assert.deepEqual(calls.allocations, []);
        assert.ok(shards.every((value) => value === 7));
    }
});

test("availability flags must be a complete boolean array", () => {
    for (const flags of [[], [true, true], [true, true, true, true], [true, 2, true], new Array(3)]) {
        const { codec, calls } = fixture();
        const shards = new Uint8Array(12).fill(7);
        assert.equal(codec.reconstruct(shards, 2, 1, flags), ReedSolomonErasure.RESULT_ERROR_INVALID_SHARD_FLAGS);
        assert.deepEqual(calls.allocations, []);
        assert.ok(shards.every((value) => value === 7));
    }
});

test("valid encoding copies back the entire parity region only", () => {
    const { codec, calls } = fixture();
    const shards = new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0]);
    assert.equal(codec.encode(shards, 2, 1), ReedSolomonErasure.RESULT_OK);
    assert.deepEqual([...shards], [1, 2, 3, 4, 5, 6, 7, 8, 99, 99, 99, 99]);
    assert.deepEqual(calls.encode, [[2, 1]]);
    assert.equal(calls.allocations.length, calls.frees.length);
});

test("valid reconstruction preserves parity and passes exact availability bytes", () => {
    const { codec, calls } = fixture();
    const shards = new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 99, 99, 99, 99]);
    assert.equal(codec.reconstruct(shards, 2, 1, [false, true, true]), ReedSolomonErasure.RESULT_OK);
    assert.deepEqual([...shards], [42, 42, 42, 42, 42, 42, 42, 42, 99, 99, 99, 99]);
    assert.deepEqual(calls.reconstruct, [[2, 1, [0, 1, 1]]]);
    assert.equal(calls.allocations.length, calls.frees.length);
});

test("the largest valid total shard count is accepted", () => {
    const { codec, calls } = fixture();
    const shards = new Uint8Array(256).fill(7);
    assert.equal(codec.encode(shards, 255, 1), ReedSolomonErasure.RESULT_OK);
    assert.deepEqual(calls.encode, [[255, 1]]);
    assert.ok(shards.subarray(0, 255).every((value) => value === 7));
    assert.equal(shards[255], 99);
});
