# WASM Package — rustfs-erasure-codec

WebAssembly build of the [rustfs-erasure-codec](../README.md) library, exposing `encode` and `reconstruct` operations for use in browsers and Node.js.

This package lives inside the current
[houseme/rustfs-erasure-codec](https://github.com/houseme/rustfs-erasure-codec)
repository and tracks the main Rust codebase in this checkout.

**Package:** `rustfs-erasure-codec-wasm` v0.2.5
**Runtime:** WASM plus ES modules; no npm runtime dependencies.
**Bindings:** [`wasm-bindgen`](https://github.com/wasm-bindgen/wasm-bindgen), with
the CLI version required to match `Cargo.lock` (currently 0.2.129).

## Building

Use Node.js 22 or newer and a Rust toolchain satisfying the workspace MSRV.
The npm development tools are pinned in `package-lock.json`; they are not part
of the runtime package.

```bash
# From the repository root
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
cd wasm
npm ci --ignore-scripts
npm run build
npm run typecheck

# Inspect or create the npm package after a successful build
npm pack --dry-run
npm pack
```

`npm run build` runs a locked release Cargo build, checks the installed
wasm-bindgen CLI against `Cargo.lock`, generates JS/types/WASM, and compiles the
TypeScript facade. It respects `CARGO_TARGET_DIR`. Developers who have already
built the current Rust sources can use `npm run build -- --skip-cargo` to repeat
only binding generation and TypeScript compilation.

The distributable files are in `wasm/dist/`: `src/index.js` and its declarations,
plus the JS/types/WASM in `generated/`. Serve the entire `dist` tree when using
native browser ES modules, so its relative WASM URL remains available.

Each initializer creates an independent generated binding factory and WASM
instance, including concurrent asynchronous calls. The build script wraps the
complete `--target no-modules` generator output in a static function; it does
not rewrite ABI details or evaluate generated source at runtime. The generated
bridge owns allocation, imports, slice conversion, copy-back, and memory-view
refresh after growth.

## TypeScript API

The [`ReedSolomonErasure`](src/index.ts) class provides the high-level interface:

### Instantiation

```typescript
// Load the binary relative to this package's module (Node.js or HTTP browser)
const rs = await ReedSolomonErasure.fromCurrentDirectory();

// Browser: async instantiation from fetch()
const rs = await ReedSolomonErasure.fromResponse(fetch("rustfs_erasure_codec_bg.wasm"));

// Node.js: synchronous instantiation from bytes
const bytes = readFileSync("/path/to/rustfs_erasure_codec_bg.wasm");
const rs = ReedSolomonErasure.fromBytes(bytes);
```

The Node.js byte-loading example uses `readFileSync` from `node:fs`. For browser
ES modules served directly, import the facade from `./dist/src/index.js`.
`fromResponse` accepts a `Response` or `Promise<Response>` and reports HTTP or
WASM compilation errors. `fromBytes` accepts `BufferSource` and remains
synchronous. A failed later initializer never falls back to an earlier instance.

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
| 14 | `RESULT_ERROR_INVALID_CUSTOM_MATRIX` | Invalid custom encoding matrix |
| 15 | `RESULT_ERROR_UNSUPPORTED_CODEC_FAMILY` | Unsupported codec family |
| 16 | `RESULT_ERROR_UNSUPPORTED_LEOPARD_PROTOTYPE` | Unsupported Leopard configuration |

Both shard counts must be positive, and their sum must not exceed 256. The byte
buffer must contain exactly that many equally sized, nonempty shards. Empty
buffers return code 11; short buffers and lengths not divisible by the total
shard count return code 9. Invalid input is rejected before modifying the buffer.
The TypeScript wrapper rejects non-finite, fractional, unsafe, or nonpositive
counts before allocating WASM memory (code 3 for data, code 5 for parity).
Positive safe integers whose total exceeds 256 return code 2, including values
that would otherwise wrap when converted to wasm32 integers.

Reconstruction requires exactly one availability flag per shard. The raw WASM
function accepts only bytes `0` (missing) and `1` (present); invalid values or flag
counts return code 12. The TypeScript wrapper accepts booleans. `reconstruct`
restores data shards only, so missing parity shards remain unchanged.

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

// shards[0..4] is restored; missing parity shard 4 is unchanged
```

## Authors

- **Nazar Mokrynskyi** ([@nazar-pc](https://github.com/nazar-pc)) — original WASM package author
- **Darren Ldl** ([@darrenldl](https://github.com/darrenldl)) — subsequent modifications

## Maintenance

Run `cargo test -p rustfs-erasure-codec-wasm --lib` from the repository root for
Rust host tests. Build the package first, then run the actual generated binary
through both environments:

```bash
# From wasm/, after npm ci and npm run build
npm run test:node
npx playwright install --with-deps chromium
npm run test:browser

# Alternatively, use an existing local Google Chrome installation
PLAYWRIGHT_CHANNEL=chrome npm run test:browser
```

The Node and Chromium integration suites use the real WASM binary. A separate
Node unit test spies only on the high-level generated operation methods to
confirm that facade validation runs before entering the bridge; it never mocks
the raw WASM ABI and is not counted as integration evidence.
They cover synchronous and asynchronous initialization, module-relative loading,
isolated/concurrent instances, invalid later modules, HTTP errors and MIME
fallback, data-only recovery, malformed counts/buffers/flags, repeated calls,
subarray boundaries, and explicit plus allocation-driven `memory.grow`.
Both environments check every 3+2 parity byte against an independent scalar
GF8 oracle and check the fixed 4+2 vector from the Rust golden-vector tests.
Memory-growth tests inspect the real generated instance's memory; the facade
does not expose memory management as a public API. The browser test server binds
only to loopback and uses a CSP without JavaScript `unsafe-eval`.

The WASM package is now maintained as part of the main repository workflow in
`houseme/rustfs-erasure-codec`.
