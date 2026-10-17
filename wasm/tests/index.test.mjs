import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { ReedSolomonErasure } from "../dist/src/index.js";
import { codecMemory, exerciseGoldenVector, exerciseInputValidation, exerciseInvalidInputs, exerciseMemoryGrowth, exerciseRepeatedCalls, exerciseRoundTrip } from "./scenarios.mjs";

const bytes = await readFile(new URL("../dist/generated/rustfs_erasure_codec_bg.wasm", import.meta.url));
const response = () => new Response(bytes, { headers: { "Content-Type": "application/wasm" } });

test("real WASM synchronous initialization and data-only roundtrip", () => {
    exerciseRoundTrip(ReedSolomonErasure.fromBytes(bytes));
    const padded = new Uint8Array(bytes.length + 9);
    padded.set(bytes, 4);
    exerciseRoundTrip(ReedSolomonErasure.fromBytes(padded.subarray(4, 4 + bytes.length)));
});

test("real WASM matches the complete fixed 4+2 parity vector", () => {
    exerciseGoldenVector(ReedSolomonErasure.fromBytes(bytes));
});

test("the independent oracle catches corruption in the last parity shard", () => {
    const codec = ReedSolomonErasure.fromBytes(bytes);
    const encode = codec.encode.bind(codec);
    codec.encode = (shards, data, parity) => {
        const result = encode(shards, data, parity);
        shards[shards.length - 1] ^= 1;
        return result;
    };
    assert.throws(() => exerciseRoundTrip(codec), /independent parity oracle: row 1/);
});

test("real WASM asynchronous Response and Promise<Response> initialization", async () => {
    exerciseRoundTrip(await ReedSolomonErasure.fromResponse(response()));
    exerciseRoundTrip(await ReedSolomonErasure.fromResponse(Promise.resolve(response())));
});

test("default initialization loads the binary relative to the installed module", async () => {
    exerciseRoundTrip(await ReedSolomonErasure.fromCurrentDirectory());
});

test("generated copy-back respects subarray byteOffset and adjacent bytes", () => {
    exerciseRoundTrip(ReedSolomonErasure.fromBytes(bytes), { subarray: true });
});

test("real WASM returns stable errors for malformed operation inputs", () => {
    exerciseInvalidInputs(ReedSolomonErasure.fromBytes(bytes));
});

test("repeated calls and valid maximum total shard count", () => {
    const codec = ReedSolomonErasure.fromBytes(bytes);
    exerciseRepeatedCalls(codec);
    const shards = new Uint8Array(256);
    shards[0] = 19;
    assert.equal(codec.encode(shards, 1, 255), ReedSolomonErasure.RESULT_OK);
    assert.ok(shards.every((byte) => byte === 19));
});

test("generated memory views refresh after explicit and allocator-driven growth", () => {
    exerciseMemoryGrowth(ReedSolomonErasure.fromBytes(bytes), bytes);
});

test("synchronous and concurrent asynchronous factories own independent instances", async () => {
    const codecs = [ReedSolomonErasure.fromBytes(bytes), ...await Promise.all([
        ReedSolomonErasure.fromResponse(response()),
        ReedSolomonErasure.fromResponse(Promise.resolve(response())),
    ])];
    const memories = codecs.map((codec) => codecMemory(codec, bytes));
    assert.equal(new Set(memories).size, 3);
    const otherSize = memories[1].buffer.byteLength;
    memories[0].grow(3);
    assert.equal(memories[1].buffer.byteLength, otherSize);
    codecs.forEach((codec, seed) => exerciseRoundTrip(codec, { seed }));
});

test("later invalid or incompatible modules cannot reuse a previous good instance", async () => {
    const codec = ReedSolomonErasure.fromBytes(bytes);
    assert.throws(() => ReedSolomonErasure.fromBytes(new Uint8Array([1, 2, 3])), WebAssembly.CompileError);
    const emptyModule = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]);
    assert.throws(() => ReedSolomonErasure.fromBytes(emptyModule), TypeError);
    await assert.rejects(ReedSolomonErasure.fromResponse(new Response(new Uint8Array([1]), {
        headers: { "Content-Type": "application/wasm" },
    })), WebAssembly.CompileError);
    exerciseRoundTrip(codec);
});

test("HTTP errors reject and wrong-MIME responses use the generated fallback", async (context) => {
    await assert.rejects(ReedSolomonErasure.fromResponse(new Response("unavailable", { status: 503 })), /503/);
    const warn = context.mock.method(console, "warn", () => {});
    const codec = await ReedSolomonErasure.fromResponse(new Response(bytes, {
        headers: { "Content-Type": "application/octet-stream" },
    }));
    exerciseRoundTrip(codec);
    assert.ok(warn.mock.callCount() >= 1);
});

test("the existing numeric result-code constants remain stable", () => {
    const names = [
        "OK", "ERROR_TOO_FEW_SHARDS", "ERROR_TOO_MANY_SHARDS", "ERROR_TOO_FEW_DATA_SHARDS",
        "ERROR_TOO_MANY_DATA_SHARDS", "ERROR_TOO_FEW_PARITY_SHARDS", "ERROR_TOO_MANY_PARITY_SHARDS",
        "ERROR_TOO_FEW_BUFFER_SHARDS", "ERROR_TOO_MANY_BUFFER_SHARDS", "ERROR_INCORRECT_SHARD_SIZE",
        "ERROR_TOO_FEW_SHARDS_PRESENT", "ERROR_EMPTY_SHARD", "ERROR_INVALID_SHARD_FLAGS",
        "ERROR_INVALID_INDEX", "ERROR_INVALID_CUSTOM_MATRIX", "ERROR_UNSUPPORTED_CODEC_FAMILY",
        "ERROR_UNSUPPORTED_LEOPARD_PROTOTYPE",
    ];
    names.forEach((name, value) => assert.equal(ReedSolomonErasure[`RESULT_${name}`], value));
});

test("unit: facade validation rejects bad inputs before generated operations", () => {
    const calls = [];
    const codec = Object.create(ReedSolomonErasure.prototype);
    Reflect.set(codec, "bindings", {
        encode(shards, data, parity) { calls.push({ operation: "encode", shards, data, parity }); return 0; },
        reconstruct(shards, data, parity, flags) { calls.push({ operation: "reconstruct", shards, data, parity, flags }); return 0; },
    });
    exerciseInputValidation(codec);
    assert.equal(calls.length, 0);
    const shards = new Uint8Array(12);
    assert.equal(codec.encode(shards, 2, 1), 0);
    assert.equal(codec.reconstruct(shards, 2, 1, [false, true, true]), 0);
    assert.equal(calls.length, 2);
    assert.equal(calls[0].shards, shards);
    assert.equal(calls[1].shards, shards);
    assert.ok(calls[1].flags instanceof Uint8Array);
    assert.deepEqual([...calls[1].flags], [0, 1, 1]);
});
