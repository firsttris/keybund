# Development

Everything needed to build, change and release Keybund. The code map is in [Modules](modules.md),
the test layers in [Testing](testing.md).

## Setup

You need Rust (stable, 1.80 or newer), Node.js 22 with pnpm 10, [`just`](https://github.com/casey/just),
OpenSSH and Git, and for the desktop app the WebKitGTK development files.

=== "Fedora, Bazzite (toolbox)"

    A toolbox keeps the host system clean – the recommended way on immutable systems:

    ```sh
    packaging/toolbox.sh          # once: toolbox "keybund-dev" with webkit2gtk, Rust, Node, pnpm, just
    toolbox enter keybund-dev
    just setup                    # pnpm install in app/
    ```

=== "Debian, Ubuntu"

    ```sh
    sudo apt install build-essential libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev \
      libayatana-appindicator3-dev openssh-client git
    curl -sSf https://sh.rustup.rs | sh
    corepack enable && cargo install just
    just setup
    ```

## Everyday commands

```sh
just dev            # the desktop app with hot reload (Tauri + Vite)
just dev-mock       # only the frontend in the browser, against the in-memory mock backend
just                # lint + every test (what CI runs)
just lint           # cargo fmt --check, clippy -D warnings, eslint, tsc
just test-rust      # Rust unit, snapshot, integration and CLI end-to-end tests
just test-frontend  # Vitest with coverage thresholds
just e2e            # Playwright against the mock backend
just coverage       # Rust coverage report (cargo-llvm-cov)
just bindings       # regenerate app/src/lib/bindings.ts after changing a Tauri command
just snapshots      # accept changed render snapshots (after reviewing them)
just screenshots    # retake the pictures in docs/
just docs           # serve this documentation on http://127.0.0.1:8000
just build          # release builds of CLI and AppImage
just install        # build and install to ~/.local/bin with a menu entry
```

Try the CLI against a throw-away home without touching your real configuration:

```sh
HOME=$(mktemp -d) cargo run -p keybund-cli -- doctor
```

## Making changes

### A change in the core

1. Change the module in `crates/core/src/`, with unit tests in the same file.
2. If the generated files change, run `cargo test -p keybund-core render`; review the new snapshots
   in `crates/core/src/snapshots/` and accept them with `just snapshots`.
3. If behaviour that the app relies on changes, mirror it in `app/src/lib/mock/backend.ts` – the
   mock's contract tests (`backend.test.ts`) and the end-to-end tests must describe the real backend,
   not a fantasy.

### A new or changed Tauri command

1. Implement the logic in `app/src-tauri/src/api.rs` (testable without Tauri).
2. Add a thin wrapper in `commands.rs` and register it in the `keybund_commands!` list in `lib.rs`.
3. `just bindings` regenerates `app/src/lib/bindings.ts`; the `bindings` test fails until you do.
4. Add the command to the mock backend, a hook in `lib/queries.ts`, and use it in a page.

### A change in the UI

Run `just dev-mock` and open http://localhost:1420 – the full app against demo data, with hot
reload, in any browser. Texts are German; keep them short and explain in plain words what happens
(the app is meant for people who are not ssh experts). Components come from shadcn/ui (zinc theme,
dark). After a visible change, `just screenshots` updates the documentation pictures.

### The mock backend

`app/src/lib/mock/backend.ts` implements every Tauri command in memory: profiles, rendering, the
doctor, host key prompts, connection tests with success and failure, imports, uninstall. It is used
whenever the frontend does not run inside Tauri – `just dev-mock`, Vitest, Playwright and the
screenshots. Its seed data (`seedState()`) is the demo you see in the pictures. In the browser
console, `window.__keybundMock.state` shows and changes its state.

## Documentation and screenshots

- The documentation lives in `docs/` and is published with MkDocs Material to
  [firsttris.github.io/keybund](https://firsttris.github.io/keybund/) by `.github/workflows/docs.yml`
  on every push to `main` that touches `docs/`, `mkdocs.yml` or `requirements-docs.txt`.
  `mkdocs build --strict` must pass (no broken links). Locally: `just docs`.
- The pictures in `docs/` are taken by `scripts/screenshots.sh` (build, `vite preview`,
  `app/scripts/screenshots.mjs` with Playwright) from the mock's demo data, with a fixed clock, so an
  unchanged page gives an identical picture. The workflow **Update screenshots** (Actions → Run
  workflow) runs it in the official Playwright image and commits the pictures that changed.
- The README and the documentation are in English; the app's texts are German.

## Continuous integration

`.github/workflows/ci.yml` runs on pushes to `main` and on pull requests that are ready for review
(drafts don't build; Markdown and `docs/` changes are skipped):

| Job | What |
|---|---|
| Rust | `cargo fmt --check`, `clippy -D warnings`, `cargo test --workspace` (with a real `sshd`) |
| Rust coverage | `cargo llvm-cov`; core and CLI must stay above 85 % of lines |
| Frontend | eslint, `tsc -b`, Vitest with coverage thresholds (90 % lines), `vite build` |
| Playwright | every end-to-end flow on a desktop and a narrow viewport |

## Releases

The version lives only in `Cargo.toml` (`[workspace.package]`); `app/package.json` follows it, and the
Tauri app takes it from Cargo. A release starts only from a tag `vX.Y.Z`:

1. **Actions → Bump version → Run workflow** and choose `patch`, `minor` or `major`. The workflow
   raises the version, commits `Release vX.Y.Z` to `main`, tags it and starts `release.yml`.
2. `release.yml` runs the complete CI (`ci.yml` via `workflow_call`), builds the CLI binary, the
   AppImage and the `.deb` on Ubuntu 22.04 (for an old enough glibc) and creates the GitHub release
   with generated notes and `SHA256SUMS`. A version with `-` (`0.2.0-rc.1`) becomes a pre-release.

From a checkout instead: set the version in `Cargo.toml` and `app/package.json`, run `cargo check`
(updates `Cargo.lock`), commit, then `git tag v0.2.0 && git push --follow-tags`. If tag and
`Cargo.toml` disagree, the release stops. Run by hand on a branch, `release.yml` only builds, without
publishing – useful to try the packaging.

Before a release, by hand: `ssh <alias>` to a real server, `sftp://<alias>` in Dolphin, a push in two
client folders, a clone through the helper, and `keybund doctor` green.

The shared workflows (`bump-version.yml`, `github-release.yml`, `screenshots.yml`) come from
[firsttris/workflows](https://github.com/firsttris/workflows) (`@v1`).

## Conventions

- Rust: `cargo fmt` (see `rustfmt.toml`), clippy clean with `-D warnings`, errors through the core's
  `Error` with a `kind`, no `unwrap` on user data.
- Comments explain *why* (an ssh or Git rule, a pitfall), not what the next line does.
- Every bug fix comes with a test that fails without it.
- No new runtime dependencies on the user's system beyond OpenSSH and Git.

## License

Keybund is licensed under the GPL-3.0 (`LICENSE`), with an additional term under section 7(b) and
the commercial license described in `NOTICE`. By contributing you agree that your contribution is
licensed the same way.
