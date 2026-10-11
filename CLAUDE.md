# Notes for Claude

## Documentation

The documentation is in `docs/` (<https://firsttris.github.io/keybund/>). Read the pages that touch
the change before starting: `architecture.md` for the data flow from `profiles.toml` to the
generated files, `modules.md` for where code lives, `generated-files.md` for what Keybund writes,
`development.md` for recipes, the mock backend and translations, `testing.md` for the test layers.
Links in the README and the docs point to the GitHub Pages site, not to `docs/*.md`.

## Reference implementations

Base details on the sources, not on memory: OpenSSH (`ssh_config(5)`, `readconf.c` for `Include`,
`Match` and first-value-wins, `ssh-copy-id`) and Git (`git-config(1)` for `includeIf "gitdir:"`,
`core.sshCommand`, `gpg.format=ssh`; `config.c` for include order). Clone them shallowly
(`git clone --depth 1`) into the scratchpad and name the source file in comments and commit
messages where it settles a detail. When it is about what ssh or Git will do, ask them in a test
(`ssh -G`, `git config --show-origin`) instead of comparing strings.

## With every change

Update what the change affects, in the same commit, and only that:

- the tests for what changed: Rust unit/snapshot/integration (`crates/*`), the mock backend and its
  contract tests (`app/src/lib/mock/`), Vitest, Playwright (`app/e2e/`);
- `app/src/lib/bindings.ts` after changing a Tauri command (`just bindings`);
- both message catalogs `app/messages/{de,en}.json` for every text, also texts from Rust (`msg!`);
- the docs in `docs/` that describe it;
- the README when a feature a user sees is added, changed or removed;
- the screenshots after a change to the look: **Actions → Update screenshots → Run workflow**, then
  look at the new pictures.

## Checks

Green before every push, then check CI:

```sh
just lint            # cargo fmt --check, clippy -D warnings, Biome, tsc
just test            # cargo test --workspace, Vitest with coverage, Playwright
mkdocs build --strict    # when docs/ or mkdocs.yml changed
```

`just format` formats Rust and TypeScript. In the cloud container Playwright needs
`PW_CHROMIUM_PATH` set to the preinstalled Chromium.

## Commits

- Work on `main` directly and push there; no pull requests unless the user asks for one.
- One topic per commit, with a message that says why.

## Working rules

- Commands as root (`sudo`, `pkexec`) are run by the user; don't work around that.
- The user's running processes are not stopped; use other ports for tests (Vite uses 1420, the
  screenshots 4317).
- Never touch the real `~/.ssh` or `~/.gitconfig` in tests: every core function takes an `Env`, use
  `Env::with_home(tempdir)`.
- Language: code, comments, docs, commits in English; the app's interface in English and German;
  the CLI's messages are German for now. Answer the user in German.

## Code style

Technology choices follow `tech-stack.md` in firsttris/workflows. Deviations of this project:

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

Functional, not object-oriented: pure functions that take data and return data, side effects at
the edges (rendering in `render.rs` is pure, `apply` writes); function components and hooks, no
classes; errors as values. A file does one thing: split it past ~400 lines, a function past ~80.
A refactor changes no behaviour. Comments explain why (an ssh or Git rule, a pitfall), not what.
Every bug fix comes with a test that fails without it.

## Keybund specifics

- Linux only (KDE, GNOME; X11 and Wayland). No new runtime dependencies beyond OpenSSH and Git.
- Keybund writes only its own files (`~/.ssh/config.d/keybund.conf`, `~/.config/keybund/`) plus one
  `Include` line in `~/.ssh/config` and one `[include]` in `~/.gitconfig`, always with a backup
  first. It never reads a private key.
- Outside Tauri the frontend runs against the in-memory mock backend (`just dev-mock`); Vitest,
  Playwright and the screenshots use it. It must behave like the real backend.
- The backend sends messages, never text: `msg!("key", arg = …)` in Rust, `t(msg)` in the frontend.
  Vitest and Playwright read the German texts; `e2e/i18n.spec.ts` checks the English app.
- Colours are tokens in `app/src/styles.css`, light and dark; a colour outside them needs a `dark:`
  variant.
- Releases start with **Actions → Bump version**; the version lives only in `Cargo.toml`.
