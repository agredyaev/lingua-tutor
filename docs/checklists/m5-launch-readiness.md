# M5 launch readiness (WBS 10.7 + gate) — 2026-07-13

## Performance vs 12-testing.md §8

| Target | Measured (local workerd) | Status |
|---|---|---|
| Dashboard < 500 ms | aggregate endpoint, single round trip; local p50 ≈ 15–40 ms | ✅ (re-check on prod after deploy) |
| Review grade < 300 ms | one batch (event + state + card status); local p50 ≈ 10–25 ms | ✅ (re-check on prod) |
| WASM ≤ 3 MB gzip | 267 KB | ✅ |
| Worker bundle | well under 1 MB compressed free-tier cap | ✅ |

## PWA / accessibility (10.7)

- [x] manifest.webmanifest (standalone, theme color), service worker registered
- [x] mobile-first layout, 640 px column, large tap targets, keyboard flow on desktop
- [x] buttons/links are real `<button>`/`<a>` elements (screen-reader navigable)
- [ ] **iPhone install check — owner action**: open prod URL in Safari → Share → Add to Home Screen; verify standalone launch + offline review of prefetched batch
- [ ] Home-screen icons: manifest `icons` empty (deliberate M5 minimum) — add 192/512 PNGs if the default tile bothers you

## Deploy prerequisites (owner)

- [ ] `terraform apply` (4 resources) + send outputs (`d1_prod_id`, `access_aud`, dev id)
- [ ] Zero Trust team name → `ACCESS_TEAM_DOMAIN`
- [ ] GitHub repo secrets: `CLOUDFLARE_API_TOKEN` (long-lived), `CLOUDFLARE_ACCOUNT_ID`
- [ ] GitHub environment `production` with required reviewer (deploy approval gate)
