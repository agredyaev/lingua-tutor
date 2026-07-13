#!/usr/bin/env sh
# Documented restore command (11-infrastructure.md §5).
# Usage: scripts/restore-backup.sh <backup.sql> [--remote-prod]
# Default target: local dev D1. --remote-prod imports into production
# (destructive on conflicts — take a fresh backup first).
set -eu

BACKUP="${1:?usage: restore-backup.sh <backup.sql> [--remote-prod]}"
export PATH="/opt/homebrew/bin:$PATH"
cd "$(dirname "$0")/../apps/worker"

if [ "${2:-}" = "--remote-prod" ]; then
  echo "Restoring $BACKUP into PRODUCTION in 5s — Ctrl-C to abort"
  sleep 5
  npx wrangler d1 execute lingua-tutor-prod --remote --env production --file "../../$BACKUP"
else
  npx wrangler d1 execute DB --local --file "../../$BACKUP"
  echo "restored into local dev D1"
fi
