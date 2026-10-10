#!/usr/bin/env bash
# Creates a Fedora toolbox with everything needed to build Keybund, so the
# host system (e.g. Bazzite) stays untouched.
set -euo pipefail
name="${1:-keybund-dev}"
toolbox create -y "$name" 2>/dev/null || true
toolbox run -c "$name" sudo dnf install -y \
  webkit2gtk4.1-devel openssl-devel curl wget file libappindicator-gtk3-devel librsvg2-devel \
  gcc gcc-c++ make patchelf nodejs pnpm git openssh-clients just
toolbox run -c "$name" bash -c 'command -v cargo || curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y'
echo "Fertig. Mit 'toolbox enter $name' wechseln, dann 'just setup && just'."
