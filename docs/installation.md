# Installation

GitHub Releases provides one native executable per supported platform.

Running a release binary does not require Rust, Cargo, Node.js, Trunk, or a separate MCP tunnel executable.

## Supported release targets

| Operating system | CPU | Release asset |
|---|---|---|
| macOS | Apple Silicon / arm64 | `lingua-tutor-vX.Y.Z-darwin-arm64.zip` |
| Linux | x86_64 | `lingua-tutor-vX.Y.Z-linux-x64.zip` |
| Linux | ARM64 / aarch64 | `lingua-tutor-vX.Y.Z-linux-aarch64.zip` |

Intel macOS and Windows binaries are not currently published.

Check your machine before downloading:

```sh
uname -s
uname -m
```

## 1. Download

Open the [latest release](https://github.com/agredyaev/lingua-tutor/releases/latest) and download:

- the ZIP matching your platform;
- `SHA256SUMS`.

Each platform ZIP must contain exactly one file named `lingua-tutor`.

## 2. Verify the download

Keep the ZIP and `SHA256SUMS` in the same directory.

### macOS

Replace the archive name with the file you downloaded:

```sh
archive="lingua-tutor-vX.Y.Z-darwin-arm64.zip"
expected="$(grep "  $archive$" SHA256SUMS | awk '{print $1}')"
actual="$(shasum -a 256 "$archive" | awk '{print $1}')"
test -n "$expected" && test "$actual" = "$expected"
```

A successful command exits without output. If it fails, do not run the binary; download the files again from the release page.

### Linux

For x86_64:

```sh
archive="lingua-tutor-vX.Y.Z-linux-x64.zip"
expected="$(grep "  $archive$" SHA256SUMS | awk '{print $1}')"
actual="$(sha256sum "$archive" | awk '{print $1}')"
test -n "$expected" && test "$actual" = "$expected"
```

For ARM64, use `lingua-tutor-vX.Y.Z-linux-aarch64.zip` instead.

## 3. Install the executable

Unpack the archive:

```sh
unzip "$archive"
```

Install it into a user-owned binary directory:

```sh
mkdir -p "$HOME/.local/bin"
install -m 0755 lingua-tutor "$HOME/.local/bin/lingua-tutor"
```

If `~/.local/bin` is not already on your `PATH`, add it to your shell configuration.

For zsh:

```sh
echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.zshrc"
export PATH="$HOME/.local/bin:$PATH"
```

For bash:

```sh
echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.bashrc"
export PATH="$HOME/.local/bin:$PATH"
```

Verify the installed binary:

```sh
lingua-tutor --help
```

## 4. macOS Gatekeeper

Release binaries are currently distributed without an Apple notarization flow. Verify the SHA-256 checksum first.

If macOS blocks a binary whose SHA-256 matches `SHA256SUMS`, remove the quarantine attribute from that installed file:

```sh
xattr -d com.apple.quarantine "$HOME/.local/bin/lingua-tutor"
```

Then run `lingua-tutor --help` again.

Do not bypass Gatekeeper for a binary whose checksum does not match `SHA256SUMS`.

## 5. Start a workspace

The current directory identifies the learning workspace:

```sh
mkdir -p "$HOME/spanish"
cd "$HOME/spanish"
lingua-tutor run
```

The runtime prompts for the tunnel ID and Runtime API key with terminal echo disabled. Those credentials stay in process memory only.

For local UI/RPC without the tunnel:

```sh
lingua-tutor run --no-tunnel
```

Then open `http://127.0.0.1:8787`.

Runtime state is stored under:

```text
<workspace>/.lingua-tutor/
├── lingua.sqlite3
└── web/
```

The release package is a single executable; the `.lingua-tutor/` directory is runtime data created after installation.

## Update

From each workspace whose data matters, create a backup before replacing the executable:

```sh
cd "$HOME/spanish"
lingua-tutor backup
```

Then download the new release, verify its checksum, unzip it, and replace the installed executable:

```sh
install -m 0755 lingua-tutor "$HOME/.local/bin/lingua-tutor"
lingua-tutor --help
```

Existing workspace data remains under each workspace's `.lingua-tutor/` directory.

## Uninstall

Remove the executable:

```sh
rm "$HOME/.local/bin/lingua-tutor"
```

Removing the executable does not delete learner data. Workspace databases remain under each `<workspace>/.lingua-tutor/` directory until you remove them.

## Troubleshooting

### `lingua-tutor: command not found`

Confirm the binary exists and `~/.local/bin` is on `PATH`:

```sh
ls -l "$HOME/.local/bin/lingua-tutor"
printf '%s\n' "$PATH"
```

### Wrong CPU architecture

If the operating system reports an incompatible executable or CPU type, check `uname -m` and install the matching asset from the supported-target table.

### Port 8787 is already in use

Choose another loopback port:

```sh
lingua-tutor run --no-tunnel --port 18787
```

Then open `http://127.0.0.1:18787`.

### Find the workspace path

Run:

```sh
lingua-tutor where
```

See [local-runtime.md](local-runtime.md) for the runtime topology, workspace behavior, credentials, backups, and health operations.

## Building from source

Contributors and CI build from source. Release users install published binaries. See [spec/12-testing.md](spec/12-testing.md) for repository acceptance commands.
