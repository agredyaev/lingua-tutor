// Playwright e2e: native localhost server, fresh directory-local SQLite per run.
import { defineConfig } from "@playwright/test";
import { resolve } from "node:path";

const port = Number(process.env.E2E_PORT ?? 18788);
const root = resolve(process.cwd(), "../..");
const workspace = `${root}/target/e2e-${process.pid}`;

export default defineConfig({
  testDir: "specs",
  timeout: 30_000,
  retries: 0, // flaky test = bug (16-rust-guidelines.md §6)
  reporter: [["list"], ["html", { open: "never" }]],
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    channel: process.env.E2E_CHANNEL || undefined,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  webServer: {
    command: `mkdir -p "${workspace}" && cd "${workspace}" && "${root}/target/debug/lingua-tutor" run --no-tunnel --port ${port}`,
    url: `http://127.0.0.1:${port}/`,
    reuseExistingServer: false,
    timeout: 600_000,
  },
});
