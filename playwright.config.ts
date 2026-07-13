// Playwright e2e (12-testing.md §7): runs against wrangler dev --local
// serving the built PWA + API on one origin. Separate CI job, not a PR gate.
import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "tests/e2e",
  timeout: 30_000,
  retries: 0, // flaky test = bug (16-rust-guidelines.md §6)
  use: {
    baseURL: "http://localhost:8787",
  },
  webServer: {
    command:
      "sh -c 'cd apps/worker && rm -rf .wrangler/state && npx wrangler d1 migrations apply DB --local && npx wrangler dev --local --port 8787'",
    url: "http://localhost:8787/api/v1/health",
    reuseExistingServer: false,
    timeout: 180_000,
  },
});
