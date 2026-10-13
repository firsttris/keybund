# Notes for Claude

The shared rules are in `.claude/rules/firsttris.md` (from firsttris/workflows); this file adds what
is specific to Keybund and wins where it says otherwise.

## Documentation

<https://firsttris.github.io/keybund/>. Read before a change: `architecture.md` for the data flow
from `profiles.toml` to the generated files, `modules.md` for where code lives,
`generated-files.md` for what Keybund writes, `development.md` for recipes, the mock backend and
translations, `testing.md` for the test layers.

## Reference implementations

OpenSSH (`ssh_config(5)`, `readconf.c` for `Include`, `Match` and first-value-wins, `ssh-copy-id`)
and Git (`git-config(1)` for `includeIf "gitdir:"`, `core.sshCommand`, `gpg.format=ssh`; `config.c`
for include order). When it is about what ssh or Git will do, ask them in a test (`ssh -G`,
`git config --show-origin`) instead of comparing strings.

## With every change

- The tests live in Rust unit/snapshot/integration (`crates/*`), the mock backend and its contract
  tests (`app/src/lib/mock/`), Vitest and Playwright (`app/e2e/`).
- `app/src/lib/bindings.ts` after changing a Tauri command (`just bindings`).
- The message catalogs `app/messages/{de,en}.json` also take the texts from Rust (`msg!`).

## Checks

```sh
just lint            # cargo fmt --check, clippy -D warnings, Biome, tsc
just test            # cargo test --workspace, Vitest with coverage, Playwright
```

`just format` formats Rust and TypeScript. In the cloud container Playwright needs
`PW_CHROMIUM_PATH` set to the preinstalled Chromium.

## Commits

- Work on `main` directly and push there; no pull requests unless the user asks for one.
- One topic per commit, with a message that says why.

## Working rules

- Tests use other ports: Vite 1420, the screenshots 4317.
- Never touch the real `~/.ssh` or `~/.gitconfig` in tests: every core function takes an `Env`, use
  `Env::with_home(tempdir)`.
- The app's interface is in English and German; the CLI's messages are German for now.

## Code style

Deviations from `tech-stack.md` and why:

- Desktop app with Tauri 2 and a Rust core instead of a web server – no TanStack Start, axum,
  database or auth.
- Code-based TanStack Router routes (`app/src/router.tsx`) instead of file-based ones: about ten
  pages, a route plugin would add more than it saves.
- No Zod: data comes from typed Tauri bindings (specta, `bindings.ts`) and is validated in Rust;
  Zod would repeat the rules.
- The core's `Error` implements `Display` by hand instead of `thiserror`: every error is a message
  (`Msg { key, args }`) that the frontend translates.
- No `tracing`: no server; diagnosis goes through the doctor and the connection tests.
- `tsc -b` instead of `tsc --noEmit`, because of the project references.

Rendering in `render.rs` is pure, `apply` writes. Comments explain why (an ssh or Git rule, a
pitfall).

## Keybund specifics

- Linux (KDE, GNOME; X11 and Wayland) and macOS 11+; Windows is not supported yet. No new runtime
  dependencies beyond OpenSSH and Git. Platform differences go through `Env::platform` (rendering
  stays pure and the macOS output is tested on Linux); the macOS CI job runs the Rust suite against
  Apple's ssh and git.
- Keybund writes only its own files (`~/.ssh/config.d/keybund.conf`, `~/.config/keybund/`) plus one
  `Include` line in `~/.ssh/config` and one `[include]` in `~/.gitconfig`, always with a backup
  first. It never reads a private key.
- Outside Tauri the frontend runs against the in-memory mock backend (`just dev-mock`); Vitest,
  Playwright and the screenshots use it. It must behave like the real backend.
- The backend sends messages, never text: `msg!("key", arg = …)` in Rust, `t(msg)` in the frontend.
  Vitest and Playwright read the German texts; `e2e/i18n.spec.ts` checks the English app.
- Stand-in scripts in tests go through `fsutil::write_test_script` or `common::write_executable`
  (see `testing.md`).
- Colours are tokens in `app/src/styles.css`, light and dark; a colour outside them needs a `dark:`
  variant.
- Releases start with **Actions → Bump version**; the version lives only in `Cargo.toml`.
