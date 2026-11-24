#!/data/data/com.termux/files/usr/bin/bash
set -euo pipefail

address=http://127.0.0.1:8787

open_running_tutor() {
    local status
    status=$(proot-distro login debian -- curl -fs --max-time 2 "$address/rpc" \
        -H 'Content-Type: application/json' \
        -d '{"jsonrpc":"2.0","id":1,"method":"tunnel_status"}' 2>/dev/null) || return 1
    case "$status" in
        *'"status":"connected"'*|*'"status":"disabled"'*)
            termux-open-url "$address/dashboard"
            ;;
        *'"status":"disconnected"'*|*'"status":"connecting"'*)
            # A fresh URL makes the browser reload setup after a runtime restart.
            termux-open-url "$address/setup?launch=$$"
            ;;
        *) return 1 ;;
    esac
}

if open_running_tutor; then
    exit
fi

proot-distro login --bind "$HOME/lingua:/mnt" debian -- \
    sh -c 'cd /mnt && exec ./lingua-tutor run' &
runtime_pid=$!
trap 'kill "$runtime_pid" 2>/dev/null || true' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
for ((attempt = 0; attempt < 40; attempt++)); do
    kill -0 "$runtime_pid" 2>/dev/null || break
    if open_running_tutor; then
        break
    fi
    sleep 0.25
done
wait "$runtime_pid"
