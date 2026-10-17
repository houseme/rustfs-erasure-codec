import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const port = Number(process.env.RSE_WASM_TEST_PORT ?? 4179);
const binary = resolve(root, "dist/generated/rustfs_erasure_codec_bg.wasm");
const contentTypes = { ".js": "text/javascript", ".mjs": "text/javascript", ".wasm": "application/wasm", ".json": "application/json" };

const server = createServer(async (request, response) => {
    try {
        const pathname = decodeURIComponent(new URL(request.url, "http://127.0.0.1").pathname);
        if (pathname === "/") {
            response.writeHead(200, {
                "Content-Type": "text/html; charset=utf-8",
                "Content-Security-Policy": "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; object-src 'none'; base-uri 'none'",
            });
            response.end("<!doctype html><meta charset=utf-8><title>WASM integration</title><h1>WASM integration harness</h1>");
            return;
        }
        if (pathname === "/wasm-error") {
            response.writeHead(503, { "Content-Type": "text/plain" });
            response.end("unavailable");
            return;
        }
        if (pathname === "/wasm-mime-fallback") {
            response.writeHead(200, { "Content-Type": "application/octet-stream" });
            response.end(await readFile(binary));
            return;
        }
        const path = resolve(root, `.${pathname}`);
        if (!path.startsWith(resolve(root) + sep) || !["/dist/", "/tests/"].some((prefix) => pathname.startsWith(prefix))) {
            response.writeHead(404).end();
            return;
        }
        const data = await readFile(path);
        response.writeHead(200, { "Content-Type": contentTypes[extname(path)] ?? "text/plain", "Cache-Control": "no-store" });
        response.end(data);
    } catch {
        response.writeHead(404).end();
    }
});

server.listen(port, "127.0.0.1", () => console.log(`WASM test server: http://127.0.0.1:${port}`));
process.on("SIGINT", () => server.close());
process.on("SIGTERM", () => server.close());
