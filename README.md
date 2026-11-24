# Lingua Tutor

Language-learning application with local SQLite storage, a browser interface and four MCP tools for ChatGPT tutoring. The tutor reads learner state, starts lessons and records results. Rust validates and persists the results, schedules card reviews and updates the learning plan.

## Install

Download an asset from [GitHub Releases](https://github.com/agredyaev/lingua-tutor/releases).

| Platform | Asset |
|---|---|
| Linux x64 | `lingua-tutor-v<VERSION>-linux-x64.zip` |
| Linux ARM64 | `lingua-tutor-v<VERSION>-linux-aarch64.zip` |
| macOS ARM64 | `lingua-tutor-v<VERSION>-darwin-arm64.zip` |
| Windows x64 | `lingua-tutor-v<VERSION>-windows-x64.zip` |
| Android 9+ (ARM64, x86-64) | `lingua-tutor-v<VERSION>-android.apk` |

The APK is available only in releases that include Android packaging. Check downloaded files against the release's `SHA256SUMS`.

### Desktop

Extract the executable to a directory on PATH, or invoke it by its full path. Desktop tunnel mode requires the OS-provided curl executable.

Choose a workspace directory and start:

```sh
mkdir -p ~/spanish
cd ~/spanish
lingua-tutor run
```

On Windows use `lingua-tutor.exe`. The runtime opens the browser at `http://127.0.0.1:8787/setup`. Follow the setup instructions to connect the OpenAI Secure MCP Tunnel, or select **Continue locally** to use the dashboard without ChatGPT.

The current directory selects the SQLite database at `.lingua-tutor/lingua.sqlite3`. Start from the same directory to reuse it.

### Android

Install the signed APK and tap **Lingua Tutor**. The app starts the Rust runtime and opens the same setup page in Android WebView. SQLite resides in the app-private files directory. No terminal or separate runtime is required.

## Update

Desktop: run `lingua-tutor backup` from the workspace, then replace only the executable.

Android: install the new APK over the existing app. The package ID and signing certificate must match. Uninstalling or clearing app data removes local learning records and backups. Export records before either action.

## ChatGPT integration

The four MCP tools are `get_active_language_profile`, `get_tutor_context`, `start_lesson` and `complete_lesson`. The application must be running for ChatGPT to use them. Tunnel credentials remain in process memory and must be entered again after restart.

## Build and verify

See [Local runtime](docs/local-runtime.md) for build commands, Android signing, CLI options, backup behavior and network boundaries.

See [Architecture](docs/spec/01-architecture.md), [Testing](docs/spec/12-testing.md) and [Tutor contract](docs/spec/07-tutor-contract.md).
