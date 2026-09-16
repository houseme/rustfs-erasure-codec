# WASM Package — rustfs-erasure-codec

WebAssembly build of the [rustfs-erasure-codec](../README.md) library, exposing `encode` and `reconstruct` operations for use in browsers and Node.js.

This package lives inside the current
[houseme/rustfs-erasure-codec](https://github.com/houseme/rustfs-erasure-codec)
repository and tracks the main Rust codebase in this checkout.

**Package:** `rustfs-erasure-codec-wasm` v0.2.5
**Dependencies:** [`wasm-bindgen`](https://github.com/rustwasm/wasm-bindgen)

## Building

Prerequisites: install [`wasm-pack`](https://rustwasm.github.io/wasm-pack/installer/).

```bash
# Build for browser (ES module)
cd wasm
wasm-pack build --target web

# Build for Node.js
cd wasm
wasm-pack build --target nodejs
```

The output `.wasm` and JS/TS bindings will be placed in `wasm/pkg/`.

## TypeScript API

The [`ReedSolomonErasure`](src/index.ts) class provides the high-level interface:

### Instantiation

```typescript
// Auto-detect environment (Node.js or browser)
const rs = await ReedSolomonErasure.fromCurrentDirectory();

// Browser: async instantiation from fetch()
const rs = await ReedSolomonErasure.fromResponse(fetch("rustfs_erasure_codec_bg.wasm"));

// Node.js: synchronous instantiation from bytes
const bytes = readFileSync("rustfs_erasure_codec_bg.wasm");
const rs = ReedSolomonErasure.fromBytes(bytes);
```

### Methods

| Method | Description |
|---|---|
| `encode(shards, dataShards, parityShards)` | Encode parity shards in-place. Returns a result code. |
| `reconstruct(shards, dataShards, parityShards, shardsAvailable)` | Reconstruct data shards in-place. `shardsAvailable` is a `boolean[]` indicating which shards are valid. |

### Result Codes

| Code | Constant | Meaning |
|---|---|---|
| 0 | `RESULT_OK` | Success |
| 1 | `RESULT_ERROR_TOO_FEW_SHARDS` | Too few shards provided |
| 2 | `RESULT_ERROR_TOO_MANY_SHARDS` | Too many shards provided |
| 3 | `RESULT_ERROR_TOO_FEW_DATA_SHARDS` | Too few data shards |
| 4 | `RESULT_ERROR_TOO_MANY_DATA_SHARDS` | Too many data shards |
| 5 | `RESULT_ERROR_TOO_FEW_PARITY_SHARDS` | Too few parity shards |
| 6 | `RESULT_ERROR_TOO_MANY_PARITY_SHARDS` | Too many parity shards |
| 7 | `RESULT_ERROR_TOO_FEW_BUFFER_SHARDS` | Too few buffer shards |
| 8 | `RESULT_ERROR_TOO_MANY_BUFFER_SHARDS` | Too many buffer shards |
| 9 | `RESULT_ERROR_INCORRECT_SHARD_SIZE` | Shard size mismatch |
| 10 | `RESULT_ERROR_TOO_FEW_SHARDS_PRESENT` | Not enough shards for reconstruction |
| 11 | `RESULT_ERROR_EMPTY_SHARD` | Empty shard encountered |
| 12 | `RESULT_ERROR_INVALID_SHARD_FLAGS` | Invalid shard flags |
| 13 | `RESULT_ERROR_INVALID_INDEX` | Invalid shard index |

## Usage Example

```typescript
import { ReedSolomonErasure } from "rustfs-erasure-codec-wasm";

const rs = await ReedSolomonErasure.fromCurrentDirectory();

const dataShards = 3;
const parityShards = 2;
const shardSize = 4;
const totalShards = dataShards + parityShards;

// Create shards: 3 data + 2 parity (initially zeroed)
const shards = new Uint8Array(totalShards * shardSize);
shards.set([1, 2, 3, 4], 0 * shardSize);  // data shard 0
shards.set([5, 6, 7, 8], 1 * shardSize);  // data shard 1
shards.set([9, 10, 11, 12], 2 * shardSize); // data shard 2

// Encode parity shards
const encodeResult = rs.encode(shards, dataShards, parityShards);
console.assert(encodeResult === ReedSolomonErasure.RESULT_OK);

// Simulate corruption: lose shard 0 and shard 4
const shardsAvailable = [false, true, true, true, false];

// Reconstruct
const reconstructResult = rs.reconstruct(shards, dataShards, parityShards, shardsAvailable);
console.assert(reconstructResult === ReedSolomonErasure.RESULT_OK);

// shards[0..4] and shards[16..20] are now restored
```

## Authors

- **Nazar Mokrynskyi** ([@nazar-pc](https://github.com/nazar-pc)) — original WASM package author
- **Darren Ldl** ([@darrenldl](https://github.com/darrenldl)) — subsequent modifications

## Maintenance

The WASM package is now maintained as part of the main repository workflow in
`houseme/rustfs-erasure-codec`.
