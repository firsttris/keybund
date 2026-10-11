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
# The server itself (exec), not a pnpm wrapper: so it ends with the trap.
(exec node_modules/.bin/vite preview --port 4317 --strictPort >/dev/null) &
server=$!
trap 'kill $server 2>/dev/null || true' EXIT
for i in $(seq 60); do
  node -e "fetch('http://localhost:4317').then(r => process.exit(r.ok ? 0 : 1), () => process.exit(1))" && break
  (( i == 60 )) && { echo "vite preview did not start" >&2; exit 1; }
  sleep 1
done
node scripts/screenshots.mjs "$PWD/../docs" http://localhost:4317
