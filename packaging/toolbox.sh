#!/usr/bin/env bash
# Creates a Fedora toolbox with everything needed to build Keybund, so the
# host system (e.g. Bazzite) stays untouched.
set -euo pipefail

# The Bun of "packageManager" in app/package.json (what CI runs), read without jq or node
version=$(sed -n 's/.*"packageManager": *"bun@\([0-9][^"]*\)".*/\1/p' "$(dirname "$0")/../app/package.json")
[[ -n "$version" ]] || { echo "Keine Bun-Version in app/package.json (packageManager)." >&2; exit 1; }
name="${1:-keybund-dev}"
toolbox create -y "$name" 2>/dev/null || true
toolbox run -c "$name" sudo dnf install -y \
  webkit2gtk4.1-devel openssl-devel curl wget file libappindicator-gtk3-devel librsvg2-devel \
  gcc gcc-c++ make patchelf unzip git openssh-clients just
toolbox run -c "$name" bash -c 'command -v cargo || curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y'
# Bun installs the frontend's packages and runs its scripts: exactly that version, unless it is
# there already (the official installer takes the release tag and needs unzip)
printf -v install_bun '[ "$(PATH="$HOME/.bun/bin:$PATH" bun --version 2>/dev/null)" = %q ] || curl -fsSL https://bun.sh/install | bash -s %q' \
  "$version" "bun-v$version"
toolbox run -c "$name" bash -c "$install_bun"
echo "Fertig. Mit 'toolbox enter $name' wechseln, dann 'just setup && just'."
