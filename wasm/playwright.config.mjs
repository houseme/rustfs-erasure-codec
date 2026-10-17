import { defineConfig } from "@playwright/test";

const port = Number(process.env.RSE_WASM_TEST_PORT ?? 4179);
const baseURL = `http://127.0.0.1:${port}`;

export default defineConfig({
    testDir: "./tests/browser",
    workers: 1,
    timeout: 30000,
    reporter: "list",
    use: {
        baseURL,
        browserName: "chromium",
        channel: process.env.PLAYWRIGHT_CHANNEL || undefined,
        headless: true,
        trace: "retain-on-failure",
    },
    webServer: {
        command: "node scripts/test-server.mjs",
        url: baseURL,
        reuseExistingServer: false,
        timeout: 30000,
    },
});
