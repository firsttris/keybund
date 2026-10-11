# Keybund – build, test, install. Run `just` for the full test suite.

set shell := ["bash", "-euo", "pipefail", "-c"]

bin_dir := env_var_or_default("XDG_BIN_HOME", env_var("HOME") / ".local/bin")
apps_dir := env_var("HOME") / ".local/share/applications"
icons_dir := env_var("HOME") / ".local/share/icons/hicolor/512x512/apps"

default: lint test

# Install frontend dependencies.
setup:
    cd app && pnpm install

# Start the desktop app in development mode.
dev:
    cd app && pnpm tauri dev

# Start only the frontend in the browser, against the in-memory mock backend.
dev-mock:
    cd app && pnpm dev:mock

# All checks that CI runs before the tests.
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cd app && pnpm lint && pnpm typecheck

# Every test layer: Rust unit/integration/E2E, Vitest, Playwright.
test: test-rust test-frontend e2e

# Rust unit, snapshot, integration (temp HOME) and CLI end-to-end tests.
test-rust: _dist
    cargo test --workspace

# Vitest unit and component tests with coverage thresholds.
test-frontend:
    cd app && pnpm coverage

# Playwright end-to-end tests against the mock backend.
e2e:
    cd app && pnpm e2e

# Rust coverage report (needs cargo-llvm-cov).
coverage: _dist
    cargo llvm-cov --workspace --html
    @echo "Report: target/llvm-cov/html/index.html"

# Regenerate app/src/lib/bindings.ts from the Tauri commands.
bindings: _dist
    UPDATE_BINDINGS=1 cargo test -p keybund-app --test bindings

# Accept changed insta snapshots after reviewing them.
snapshots:
    INSTA_UPDATE=always cargo test -p keybund-core render

# Pictures in docs/ (README, documentation, banner, social preview) from the mock backend's demo data.
screenshots:
    bash scripts/screenshots.sh

# Serve the documentation (MkDocs Material) on http://127.0.0.1:8000 with live reload.
docs:
    python3 -m venv .venv-docs && .venv-docs/bin/pip install -q -r requirements-docs.txt
    .venv-docs/bin/mkdocs serve

# Release builds of the CLI and the AppImage.
build:
    cargo build --release -p keybund-cli
    cd app && pnpm tauri build --bundles appimage

# Copy CLI and AppImage to ~/.local/bin and register the desktop entry.
install: build
    mkdir -p {{bin_dir}} {{apps_dir}} {{icons_dir}}
    install -m 755 target/release/keybund {{bin_dir}}/keybund
    install -m 755 "$(ls -t target/release/bundle/appimage/*.AppImage | head -1)" {{bin_dir}}/Keybund.AppImage
    install -m 644 app/src-tauri/icons/icon.png {{icons_dir}}/keybund.png
    sed "s|@BIN_DIR@|{{bin_dir}}|" packaging/keybund.desktop > {{apps_dir}}/keybund.desktop
    @echo "Installed keybund and Keybund.AppImage to {{bin_dir}}"

# Remove what `just install` put in place (profiles and configs stay).
uninstall:
    rm -f {{bin_dir}}/keybund {{bin_dir}}/Keybund.AppImage {{apps_dir}}/keybund.desktop {{icons_dir}}/keybund.png

# The Tauri context needs a dist folder at compile time.
_dist:
    @test -d app/dist || (mkdir -p app/dist && echo '<!doctype html>' > app/dist/index.html)
