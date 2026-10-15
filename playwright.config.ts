// Playwright e2e: native localhost server, fresh SQLite per run.
import { defineConfig } from "@playwright/test";

const port = Number(process.env.E2E_PORT ?? 18788);
const db = `target/e2e-${process.pid}/lingua.sqlite3`;

export default defineConfig({
  testDir: "tests/e2e",
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
    command: `cargo run -p lingua-tutor-local -- serve --db ${db} --port ${port}`,
    url: `http://127.0.0.1:${port}/api/v1/health`,
    reuseExistingServer: false,
    timeout: 600_000,
  },
});
