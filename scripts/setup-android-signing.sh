#!/usr/bin/env bash
# One-time private Android signing setup for GitHub Actions.
set -euo pipefail
umask 077

repo="agredyaev/lingua-tutor"
directory="$HOME/.config/lingua-tutor/android-signing"
keystore="$directory/release.p12"
password_file="$directory/password"
alias="lingua-tutor"

for tool in gh keytool openssl; do
  command -v "$tool" >/dev/null || { echo "Required command not found: $tool" >&2; exit 1; }
done

existing="$(gh secret list --repo "$repo" --json name --jq '.[].name')"
names=(ANDROID_KEYSTORE_BASE64 ANDROID_KEY_ALIAS ANDROID_KEYSTORE_PASSWORD ANDROID_KEY_PASSWORD)
count=0
for name in "${names[@]}"; do
  if grep -Fxq "$name" <<< "$existing"; then count=$((count + 1)); fi
done
if [[ "$count" == 4 ]]; then
  echo "All four Android signing secrets already exist; no changes made."
  exit 0
fi

mkdir -p "$directory"
chmod 700 "$directory"
if [[ ! -e "$keystore" && ! -e "$password_file" ]]; then
  if (( count > 0 )); then
    echo "Partial signing configuration exists, but the original local key is missing. Do not replace the key." >&2
    exit 1
  fi
  stage="$(mktemp -d "$directory/.new.XXXXXXXX")"
  trap 'rm -rf "$stage"' EXIT
  openssl rand -hex 32 > "$stage/password"
  export LINGUA_KEY_PASSWORD="$(cat "$stage/password")"
  keytool -genkeypair -storetype PKCS12 -keystore "$stage/release.p12" \
    -storepass:env LINGUA_KEY_PASSWORD -keypass:env LINGUA_KEY_PASSWORD \
    -alias "$alias" -keyalg RSA -keysize 4096 -validity 36500 \
    -dname 'CN=Lingua Tutor Android Release'
  mv "$stage/release.p12" "$keystore"
  mv "$stage/password" "$password_file"
else
  [[ -f "$keystore" && -f "$password_file" ]] || {
    echo "Incomplete local signing key backup. Restore the missing file before continuing." >&2
    exit 1
  }
  export LINGUA_KEY_PASSWORD="$(cat "$password_file")"
fi
chmod 600 "$keystore" "$password_file"
keytool -list -keystore "$keystore" -storetype PKCS12 \
  -storepass:env LINGUA_KEY_PASSWORD -alias "$alias" >/dev/null

# Send values directly to gh; never put passwords or key contents in argv.
openssl base64 -A -in "$keystore" | gh secret set ANDROID_KEYSTORE_BASE64 --repo "$repo"
printf '%s' "$alias" | gh secret set ANDROID_KEY_ALIAS --repo "$repo"
printf '%s' "$LINGUA_KEY_PASSWORD" | gh secret set ANDROID_KEYSTORE_PASSWORD --repo "$repo"
printf '%s' "$LINGUA_KEY_PASSWORD" | gh secret set ANDROID_KEY_PASSWORD --repo "$repo"
unset LINGUA_KEY_PASSWORD

echo "Android signing secrets configured for $repo."
echo "Back up the private keystore and password from $directory."
gh secret list --repo "$repo" --json name --jq '.[].name | select(startswith("ANDROID_"))'
