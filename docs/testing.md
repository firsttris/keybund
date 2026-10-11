# Testing

Keybund edits the files that decide how you log in to every server and Git host – mistakes there
lock people out. So every layer has its own tests, and the important ones run against the real
programs: a real `git`, a real `ssh`, a real `sshd` on localhost. All of it runs in CI on every
change: about 150 Rust tests, 120 Vitest tests and 35 Playwright flows on two viewports.

```sh
just                # lint + all tests
just test-rust      # cargo test --workspace
just test-frontend  # Vitest with coverage thresholds
just e2e            # Playwright
just coverage       # Rust coverage (cargo-llvm-cov)
```

## The layers

| Layer | What it proves | Where |
|---|---|---|
| **Unit (Rust)** | parsing of `profiles.toml` and `ssh_config`, validation, include placement, diffing, ssh output interpretation, URL parsing, provider data, quoting | next to the code in `crates/*/src/**` |
| **Snapshot** | the exact bytes of every generated file for a sample with servers, two clients on github.com, signing and an archived key (insta) | `crates/core/src/snapshots/` |
| **Integration (Rust)** | `apply` against a temporary home; the result resolved by the real `git` (`includeIf`, worktrees, the include order) and `ssh -G`; backups, uninstall, import with cleanup, doctor checks, archive | `crates/core/tests/integration.rs` |
| **Agent** | ssh-agent detection with a private `SSH_AUTH_SOCK` | `crates/core/tests/agent.rs` |
| **Real sshd** | a throw-away `sshd` on localhost: a key with a passphrase is asked once and then served by the agent – for the connection test and for `git`; a wrong key is detected; installing a key like `ssh-copy-id` | `crates/core/tests/sshd.rs` |
| **CLI integration** | every command against a temporary home, with fake `ssh`/`xdg-open` where a network would be needed; exit codes; refusing to ask without a terminal | `crates/cli/tests/cli.rs` |
| **CLI end-to-end** | the full life cycle through the `keybund` binary only: two clients on github.com, a private default, a server, clone, which, doctor, archive – checked with the real `git`; and a NAS against a throw-away `sshd`: `ssh -G` resolves alias and IP from the generated files, `trust`, `install-key` (once), `test` succeeds and fails for real, `git ls-remote` over the alias, folder rules through a symlink, a linked worktree and a same-prefix sibling | `crates/cli/tests/e2e.rs` |
| **Scenario: two companies, private, two servers** | one person with five keys and every connection real: a GitHub stand-in (an `sshd` whose `authorized_keys` maps each key to an account and lets an account see only its own repositories, like GitHub), two server `sshd`s that allow two login tries, and an ssh-agent that before every step holds the *other* keys – another account's first – but not the one needed. First without Keybund: plain ssh logs in as the wrong account (“Repository not found”) and the server gives up – the setup catches a mix-up. Then with Keybund: `test` reaches each account, `git clone` inside a folder and `keybund clone`, commit, push and pull per folder log in as that folder's account with its e-mail, signed private commits verify, one company never reaches the other's repositories, outside every folder the private account, `trust`/`test`/`ssh`/`git` on both servers, a deploy push from a company folder to the NAS, and offboarding one company. Each key lands in the agent after its first use. Breaking `IdentitiesOnly` (git, Host blocks or the test) or `AddKeysToAgent` makes it fail | `crates/cli/tests/scenario.rs` |
| **macOS** | the whole Rust suite once more on a macOS runner, against Apple's `ssh` and `git` (`/usr/bin` goes before the runner's Homebrew, and the job fails if it does not); the CLI tests expect what the binary writes there (`gitdir/i:`, the Keychain options). Rendering for macOS is also tested on Linux through `Env::platform` | `.github/workflows/ci.yml` (`rust-macos`) |
| **IPC (Tauri)** | commands called through Tauri's IPC layer with the mock runtime and the real capabilities: JSON in, JSON out | `app/src-tauri/tests/ipc.rs` |
| **Contract** | `bindings.ts` matches the Rust commands; the mock backend behaves like the core (validation, rendering, errors) | `app/src-tauri/tests/bindings.rs`, `app/src/lib/mock/backend.test.ts` |
| **Unit & component (frontend)** | Vitest + Testing Library: whole pages and flows against the mock backend – creating keys, adding uses, tests and their fixes, imports, uninstall; 90 % line coverage required | `app/src/**/*.test.ts(x)` |
| **Translations** | both catalogs complete with the same placeholders, every message used, every key used in Rust present; language detection and switching; an English browser walks every page, dialog and test result looking for German left behind | `crates/core/tests/i18n.rs`, `app/src/lib/i18n.test.ts`, `app/e2e/i18n.spec.ts` |
| **End-to-end (frontend)** | Playwright: every flow in Chromium on a desktop and a narrow window, including layout checks, and in CI once more in WebKit – the engine Tauri uses on macOS (locally with `PW_WEBKIT=1`) | `app/e2e/` |

## Writing tests

- **Use a temporary home.** Every core function takes an `Env`; `Env::with_home(tempdir)` points it
  at a throw-away directory. Never test against the real `~/.ssh`.
- **Ask the real tools.** When a test is about what Git or ssh will do, let them answer: run
  `git config --show-origin` or `ssh -G` on the generated files instead of comparing strings.
  Tests that need `ssh`, `sshd` or `git` skip themselves when the program is missing.
  ssh reads `~/.ssh/config` from the passwd entry, not from `HOME`: a test in a sandbox that runs
  the real `ssh` through the binary uses `Sandbox::ssh_shim()`, which hands ssh the sandbox's
  generated files (`-F`), so the developer's own config never leaks in. In core tests,
  `Env::with_home` does the same for `Env::ssh_g`: it asks ssh with `-F <home>/.ssh/config` and maps
  paths ssh expanded below the real home back into the test home.
- **Platforms.** What differs between Linux and macOS goes through `Env::platform`, never `cfg!` in
  the rendering, so a test on Linux can render and check the macOS files
  (`Env { platform: Platform::MacOs, ..Env::with_home(dir) }`).
- **Snapshots for rendering.** A change in `render.rs` shows up as a snapshot diff; review it like
  code, then `just snapshots`.
- **Frontend tests click like a user.** Query by role and label (`getByRole("button", { name:
  "Testen" })`), not by CSS classes. The tests run in German (`src/test/setup.ts`,
  `playwright.config.ts`); the English texts are covered by `src/lib/i18n.test.ts` and
  `e2e/i18n.spec.ts`.
- **Keep the mock honest.** A new backend behaviour gets a contract test in `backend.test.ts` and the
  same behaviour in Rust – otherwise the frontend tests would pass against a backend that does not
  exist.

## Manual checks before a release

Some things only a real desktop shows: `ssh <alias>` against a real server, `sftp://<alias>` in
Dolphin, a push in two client folders with different accounts, a clone through the helper, a key
with a passphrase through ksshaskpass, and `keybund doctor` green on a fresh user account.
