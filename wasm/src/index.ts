import { createBindings, type GeneratedBindings } from "../generated/rustfs_erasure_codec.js";

/** Reed-Solomon operations backed by a separately initialized WASM instance. */
export class ReedSolomonErasure {
    public static readonly RESULT_OK = 0;
    public static readonly RESULT_ERROR_TOO_FEW_SHARDS = 1;
    public static readonly RESULT_ERROR_TOO_MANY_SHARDS = 2;
    public static readonly RESULT_ERROR_TOO_FEW_DATA_SHARDS = 3;
    public static readonly RESULT_ERROR_TOO_MANY_DATA_SHARDS = 4;
    public static readonly RESULT_ERROR_TOO_FEW_PARITY_SHARDS = 5;
    public static readonly RESULT_ERROR_TOO_MANY_PARITY_SHARDS = 6;
    public static readonly RESULT_ERROR_TOO_FEW_BUFFER_SHARDS = 7;
    public static readonly RESULT_ERROR_TOO_MANY_BUFFER_SHARDS = 8;
    public static readonly RESULT_ERROR_INCORRECT_SHARD_SIZE = 9;
    public static readonly RESULT_ERROR_TOO_FEW_SHARDS_PRESENT = 10;
    public static readonly RESULT_ERROR_EMPTY_SHARD = 11;
    public static readonly RESULT_ERROR_INVALID_SHARD_FLAGS = 12;
    public static readonly RESULT_ERROR_INVALID_INDEX = 13;
    public static readonly RESULT_ERROR_INVALID_CUSTOM_MATRIX = 14;
    public static readonly RESULT_ERROR_UNSUPPORTED_CODEC_FAMILY = 15;
    public static readonly RESULT_ERROR_UNSUPPORTED_LEOPARD_PROTOTYPE = 16;

    /** Load this package's WASM binary relative to the module, in Node or a browser. */
    public static async fromCurrentDirectory(): Promise<ReedSolomonErasure> {
        const url = new URL("../generated/rustfs_erasure_codec_bg.wasm", import.meta.url);
        if (url.protocol === "file:") {
            // Keep Node-specific loading out of the browser's initial module graph.
            const { readFile } = await import("node:fs/promises");
            return ReedSolomonErasure.fromBytes(await readFile(url));
        }
        return ReedSolomonErasure.fromResponse(fetch(url));
    }

    /** Initialize from a fetched response, including a promise returned by fetch(). */
    public static async fromResponse(source: Response | Promise<Response>): Promise<ReedSolomonErasure> {
        const bindings = createBindings();
        await bindings({ module_or_path: source });
        return new ReedSolomonErasure(bindings);
    }

    /** Synchronously initialize a new instance from WASM bytes. */
    public static fromBytes(bytes: BufferSource): ReedSolomonErasure {
        const bindings = createBindings();
        bindings.initSync({ module: bytes });
        return new ReedSolomonErasure(bindings);
    }

    private readonly bindings: GeneratedBindings;

    private constructor(bindings: GeneratedBindings) {
        this.bindings = bindings;
    }

    private static validateInputs(shards: Uint8Array, dataShards: number, parityShards: number): number {
        // The generated bridge converts counts to wasm32 integers; reject
        // fractions and wraparound before that conversion can lose information.
        if (!Number.isSafeInteger(dataShards) || dataShards < 1) {
            return ReedSolomonErasure.RESULT_ERROR_TOO_FEW_DATA_SHARDS;
        }
        if (!Number.isSafeInteger(parityShards) || parityShards < 1) {
            return ReedSolomonErasure.RESULT_ERROR_TOO_FEW_PARITY_SHARDS;
        }
        const totalShards = dataShards + parityShards;
        if (totalShards > 256) {
            return ReedSolomonErasure.RESULT_ERROR_TOO_MANY_SHARDS;
        }
        if (shards.length === 0) {
            return ReedSolomonErasure.RESULT_ERROR_EMPTY_SHARD;
        }
        if (shards.length < totalShards || shards.length % totalShards !== 0) {
            return ReedSolomonErasure.RESULT_ERROR_INCORRECT_SHARD_SIZE;
        }
        return ReedSolomonErasure.RESULT_OK;
    }

    /** Encode parity in-place. Returns a RESULT_* code and preserves data shards. */
    public encode(shards: Uint8Array, dataShards: number, parityShards: number): number {
        const validation = ReedSolomonErasure.validateInputs(shards, dataShards, parityShards);
        if (validation !== ReedSolomonErasure.RESULT_OK) {
            return validation;
        }
        return this.bindings.encode(shards, dataShards, parityShards);
    }

    /** Restore missing data shards in-place; missing parity shards remain untouched. */
    public reconstruct(shards: Uint8Array, dataShards: number, parityShards: number, shardsAvailable: boolean[]): number {
        const validation = ReedSolomonErasure.validateInputs(shards, dataShards, parityShards);
        if (validation !== ReedSolomonErasure.RESULT_OK) {
            return validation;
        }
        if (shardsAvailable.length !== dataShards + parityShards) {
            return ReedSolomonErasure.RESULT_ERROR_INVALID_SHARD_FLAGS;
        }
        for (const flag of shardsAvailable) {
            if (typeof flag !== "boolean") {
                return ReedSolomonErasure.RESULT_ERROR_INVALID_SHARD_FLAGS;
            }
        }
        const availability = Uint8Array.from(shardsAvailable, (flag) => flag ? 1 : 0);
        return this.bindings.reconstruct(shards, dataShards, parityShards, availability);
    }
}
