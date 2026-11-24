# 11 — Runtime, CI and Release

## Runtime

See [Local runtime](../local-runtime.md) for commands, supported platforms, state paths, transport, backups and Android signing. `apps/local` owns the database and migrations. `apps/android` only owns Android startup and WebView integration.

## CI and release

| Workflow | Checks |
|---|---|
| `ci.yml` | Version consistency, format, strict Clippy, workspace tests, Cargo Deny, generated-output exclusion, Windows checks, Android APK build and emulator startup/restart checks. |
| `e2e.yml` | Build frontend and native server; run Playwright in Chromium. |
| `fuzz.yml` | Locked fuzz compilation, fuzz targets and minimum 86% executable-line coverage per included Rust file. |
| `release-please.yml` | Run after successful main CI; create versioned release and invoke asset build. |
| `release.yml` | Resolve tag and successful CI; build web bundle, Linux x64/ARM64, macOS ARM64, Windows x64 and Android for supported tags; run performance/packaging checks; verify SHA256 and publish asset set. |
| `android.yml` | Install SDK/NDK, build ARM64/x86-64 libraries, package and verify APK; launch test APK in Android emulator; sign release APK with repository secrets. |

Release artifacts are uploaded only after the required platform jobs succeed. A created release may have no binary assets until publication completes. CI test APKs use an ephemeral certificate, not the release key.

The release workflow checks CI for the exact tag commit. Do not treat a successful build of another commit as release evidence.

## Health

The `health` JSON-RPC operation reports runtime/schema health without learning content or tunnel credentials. Desktop `lingua-tutor check` checks the project database. Backup and restore tests use the SQLite backup API and integrity/foreign-key validation.
