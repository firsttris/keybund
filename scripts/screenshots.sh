#!/usr/bin/env bash
# Takes the pictures in docs/ (README, documentation, banner, social preview)
# from the demo data of the mock backend: `just screenshots`
# Builds the frontend (outside of Tauri it runs against the in-memory mock),
# serves it with vite preview, runs app/scripts/screenshots.mjs and stops the
# server again. The workflow "Update screenshots" runs this in the Playwright
# image and commits the pictures that changed.
set -euo pipefail
cd "$(dirname "$0")/../app"
pnpm build
# The server itself (exec), not a pnpm wrapper: so it ends with the trap. An IP, not
# "localhost", which can resolve to ::1 in a container without IPv6.
log=$(mktemp)
(exec node_modules/.bin/vite preview --host 127.0.0.1 --port 4317 --strictPort >"$log" 2>&1) &
server=$!
trap 'kill $server 2>/dev/null || true; rm -f "$log"' EXIT
for i in $(seq 60); do
  node -e "fetch('http://127.0.0.1:4317').then(r => process.exit(r.ok ? 0 : 1), () => process.exit(1))" && break
  (( i == 60 )) && { echo "vite preview did not start:" >&2; cat "$log" >&2; exit 1; }
  sleep 1
done
node scripts/screenshots.mjs "$PWD/../docs" http://127.0.0.1:4317
