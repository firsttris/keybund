# Development

Everything needed to build, change and release Keybund. The code map is in [Modules](modules.md),
the test layers in [Testing](testing.md).

## Setup

You need Rust (stable, 1.89 or newer), Node.js 22 with pnpm 10, [`just`](https://github.com/casey/just),
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
just lint           # cargo fmt --check, clippy -D warnings, Biome, tsc
just format         # cargo fmt and Biome (format, organize imports, safe fixes)
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
reload, in any browser. Every text is a message in both languages (see [Translations](#translations));
keep texts short and explain in plain words what happens (the app is meant for people who are not
ssh experts). Components come from shadcn/ui (zinc theme, light and dark). Colours are tokens in
`src/styles.css` (`:root` light, `:root.dark` dark); a colour outside the tokens needs both variants
(`text-red-700 dark:text-red-300`). `public/theme.js` sets the class before the first paint,
`src/lib/theme.ts` keeps it up to date. After a visible change, `just screenshots` updates the
documentation pictures (they show the dark theme).

### The mock backend

`app/src/lib/mock/backend.ts` implements every Tauri command in memory: profiles, rendering, the
doctor, host key prompts, connection tests with success and failure, imports, uninstall. It is used
whenever the frontend does not run inside Tauri – `just dev-mock`, Vitest, Playwright and the
screenshots. Its seed data (`seedState()`) is the demo you see in the pictures. In the browser
console, `window.__keybundMock.state` shows and changes its state. It validates like the core
(`store::validate`), and a few switches on its state let tests reach the error paths: `latency`
(also honoured by `update_profile`, `add_host` and `host_key_check`), `configError` (every command
fails like a broken `profiles.toml`), `applyError` (the profile is saved but the configs are not
written) and `hostKeyError` (`host_key_check` fails with that error kind).

## Documentation and screenshots

- The documentation lives in `docs/` and is published with MkDocs Material to
  [firsttris.github.io/keybund](https://firsttris.github.io/keybund/) by `.github/workflows/docs.yml`
  on every push to `main` that touches `docs/`, `mkdocs.yml` or `requirements-docs.txt`.
  `mkdocs build --strict` must pass (no broken links). Locally: `just docs`.
- The pictures in `docs/` are taken by `scripts/screenshots.sh` (build, `vite preview`,
  `app/scripts/screenshots.mjs` with Playwright) from the mock's demo data, with a fixed clock, so an
  unchanged page gives an identical picture. The workflow **Update screenshots** (Actions → Run
  workflow) runs it in the official Playwright image and commits the pictures that changed.
- The README, the documentation and the screenshots are in English; the app is English and German.

## Translations

The app speaks English and German. All texts live in two catalogs, **`app/messages/de.json`** and
**`app/messages/en.json`** ([inlang message format](https://inlang.com/m/reootnfj/plugin-inlang-messageFormat)),
compiled by [Paraglide JS](https://inlang.com/m/gerre34r/library-inlang-paraglideJs) into typed
functions in `app/src/paraglide` (generated, not committed; `pnpm i18n` or any Vite run compiles them).

- **Frontend texts**: `m.key()` in components – `m.settings_language()`, `m.home_connected({ when })`.
  Plurals are message variants on the input `n`; `rich(m.key, { name: <b>…</b> })` puts elements into
  a text.
- **Texts from Rust**: the backend sends messages, never text. In Rust, `msg!("doctor_sshDir_bad",
  mode = "755")` builds a `Msg { key, args }`; an argument can be another message
  (`.nested("field", msg!("field_alias"))`). The frontend renders it with `t(msg)`, the CLI through
  `Display`, from the same catalogs (embedded in the binary). Messages used from Rust must be plain
  strings with `{placeholders}` – no plural variants.
- **The language** comes from the setting (localStorage `keybund_lang`) or the system: German for
  `de-*`, English otherwise. Switching re-renders the app without a reload.
- **Tests read German**: Vitest (`src/test/setup.ts`) and Playwright (`playwright.config.ts`) run in
  German. `crates/core/tests/i18n.rs` and `src/lib/i18n.test.ts` fail when a key is missing in one
  language, placeholders differ, a message is unused or Rust uses a key that does not exist;
  `e2e/i18n.spec.ts` walks the English app looking for German left behind.

To add a text: add the key to both catalogs (`area_name`, e.g. `settings_languageHint`), use it, run
`pnpm test`. To add a language: add it to `locales` in `app/project.inlang/settings.json`, create
`app/messages/<lang>.json` with every key, extend `LANGS` and `systemLang()` in `src/lib/i18n.ts` and
`Lang` in `crates/core/src/i18n.rs`.

## Continuous integration

`.github/workflows/ci.yml` runs on pushes to `main` and on pull requests that are ready for review
(drafts don't build; Markdown and `docs/` changes are skipped):

| Job | What |
|---|---|
| Rust | `cargo fmt --check`, `clippy -D warnings`, `cargo test --workspace` (with a real `sshd`) |
| Rust coverage | `cargo llvm-cov`; core and CLI must stay above 85 % of lines |
| Frontend | Biome (`biome check`), `tsc -b`, Vitest with coverage thresholds (90 % lines), `vite build` |
| Playwright | every end-to-end flow on a desktop and a narrow viewport, in the official Playwright image of the version in `pnpm-lock.yaml` |

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
- TypeScript: [Biome](https://biomejs.dev/) formats and lints (`app/biome.json`); `just format` before a
  commit. A rule is switched off only with a `biome-ignore` comment that says why.
- Comments explain *why* (an ssh or Git rule, a pitfall), not what the next line does.
- Every bug fix comes with a test that fails without it.
- No new runtime dependencies on the user's system beyond OpenSSH and Git.

## License

Keybund is licensed under the GPL-3.0 (`LICENSE`), with an additional term under section 7(b) and
the commercial license described in `NOTICE`. By contributing you agree that your contribution is
licensed the same way.
