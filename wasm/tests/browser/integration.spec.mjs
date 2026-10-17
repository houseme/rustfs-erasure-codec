import { test, expect } from "@playwright/test";

test.beforeEach(async ({ page }) => {
    await page.goto("/");
});

for (const initialization of ["bytes", "response", "promise", "current-directory"]) {
    test(`real browser ${initialization} initialization, roundtrip and input errors`, async ({ page }) => {
        await page.evaluate(async (initialization) => {
            const { ReedSolomonErasure } = await import("/dist/src/index.js");
            const { exerciseInvalidInputs, exerciseRoundTrip } = await import("/tests/scenarios.mjs");
            const url = "/dist/generated/rustfs_erasure_codec_bg.wasm";
            let codec;
            if (initialization === "bytes") {
                codec = ReedSolomonErasure.fromBytes(await (await fetch(url)).arrayBuffer());
            } else if (initialization === "response") {
                codec = await ReedSolomonErasure.fromResponse(await fetch(url));
            } else if (initialization === "promise") {
                codec = await ReedSolomonErasure.fromResponse(fetch(url));
            } else {
                codec = await ReedSolomonErasure.fromCurrentDirectory();
            }
            exerciseRoundTrip(codec, { subarray: true });
            exerciseInvalidInputs(codec);
        }, initialization);
    });
}

test("real browser repeated calls and generated memory-view refresh", async ({ page }) => {
    await page.evaluate(async () => {
        const { ReedSolomonErasure } = await import("/dist/src/index.js");
        const { exerciseRepeatedCalls, exerciseMemoryGrowth } = await import("/tests/scenarios.mjs");
        const bytes = await (await fetch("/dist/generated/rustfs_erasure_codec_bg.wasm")).arrayBuffer();
        const codec = ReedSolomonErasure.fromBytes(bytes);
        exerciseRepeatedCalls(codec);
        exerciseMemoryGrowth(codec, bytes);
    });
});

test("real browser matches the complete fixed 4+2 parity vector", async ({ page }) => {
    await page.evaluate(async () => {
        const { ReedSolomonErasure } = await import("/dist/src/index.js");
        const { exerciseGoldenVector } = await import("/tests/scenarios.mjs");
        exerciseGoldenVector(await ReedSolomonErasure.fromCurrentDirectory());
    });
});

test("real browser factories isolate concurrent initialization and failed later inputs", async ({ page }) => {
    await page.evaluate(async () => {
        const { ReedSolomonErasure } = await import("/dist/src/index.js");
        const { check, codecMemory, exerciseRoundTrip } = await import("/tests/scenarios.mjs");
        const url = "/dist/generated/rustfs_erasure_codec_bg.wasm";
        const bytes = await (await fetch(url)).arrayBuffer();
        const codecs = [ReedSolomonErasure.fromBytes(bytes), ...await Promise.all([
            ReedSolomonErasure.fromResponse(fetch(url)),
            ReedSolomonErasure.fromCurrentDirectory(),
        ])];
        const memories = codecs.map((codec) => codecMemory(codec, bytes));
        check(new Set(memories).size === 3, "instances must not share memory");
        const otherSize = memories[1].buffer.byteLength;
        memories[0].grow(3);
        check(memories[1].buffer.byteLength === otherSize, "memory growth must stay isolated");
        for (const invalid of [new Uint8Array([1, 2, 3]), new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0])]) {
            let rejected = false;
            try { ReedSolomonErasure.fromBytes(invalid); } catch { rejected = true; }
            check(rejected, "later bad module must not reuse an initialized instance");
        }
        let rejected = false;
        try {
            await ReedSolomonErasure.fromResponse(new Response(new Uint8Array([1]), { headers: { "Content-Type": "application/wasm" } }));
        } catch { rejected = true; }
        check(rejected, "asynchronous invalid module must reject");
        codecs.forEach((codec, seed) => exerciseRoundTrip(codec, { seed }));
    });
});

test("real browser HTTP errors and generated wrong-MIME fallback", async ({ page }) => {
    const warnings = [];
    page.on("console", (message) => { if (message.type() === "warning") warnings.push(message.text()); });
    await page.evaluate(async () => {
        const { ReedSolomonErasure } = await import("/dist/src/index.js");
        const { check, exerciseRoundTrip } = await import("/tests/scenarios.mjs");
        let error;
        try { await ReedSolomonErasure.fromResponse(fetch("/wasm-error")); } catch (caught) { error = caught; }
        check(error instanceof Error && error.message.includes("503"), "HTTP status must be reported");
        exerciseRoundTrip(await ReedSolomonErasure.fromResponse(fetch("/wasm-mime-fallback")));
    });
    expect(warnings.some((warning) => warning.includes("MIME type"))).toBeTruthy();
});
