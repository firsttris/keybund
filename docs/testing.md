# Testing

Keybund edits the files that decide how you log in to every server and Git host – mistakes there
lock people out. So every layer has its own tests, and the important ones run against the real
programs: a real `git`, a real `ssh`, a real `sshd` on localhost. All of it runs in CI on every
change: about 150 Rust tests, 110 Vitest tests and 30 Playwright flows on two viewports.

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
| **CLI end-to-end** | the full life cycle through the `keybund` binary only: two clients on github.com, a private default, a server, clone, which, doctor, archive – checked with the real `git` | `crates/cli/tests/e2e.rs` |
| **IPC (Tauri)** | commands called through Tauri's IPC layer with the mock runtime and the real capabilities: JSON in, JSON out | `app/src-tauri/tests/ipc.rs` |
| **Contract** | `bindings.ts` matches the Rust commands; the mock backend behaves like the core (validation, rendering, errors) | `app/src-tauri/tests/bindings.rs`, `app/src/lib/mock/backend.test.ts` |
| **Unit & component (frontend)** | Vitest + Testing Library: whole pages and flows against the mock backend – creating keys, adding uses, tests and their fixes, imports, uninstall; 90 % line coverage required | `app/src/**/*.test.ts(x)` |
| **End-to-end (frontend)** | Playwright in Chromium: every flow on a desktop and a narrow window, including layout checks | `app/e2e/` |

## Writing tests

- **Use a temporary home.** Every core function takes an `Env`; `Env::with_home(tempdir)` points it
  at a throw-away directory. Never test against the real `~/.ssh`.
- **Ask the real tools.** When a test is about what Git or ssh will do, let them answer: run
  `git config --show-origin` or `ssh -G` on the generated files instead of comparing strings.
  Tests that need `ssh`, `sshd` or `git` skip themselves when the program is missing.
- **Snapshots for rendering.** A change in `render.rs` shows up as a snapshot diff; review it like
  code, then `just snapshots`.
- **Frontend tests click like a user.** Query by role and label (`getByRole("button", { name:
  "Testen" })`), not by CSS classes; the German labels are part of the contract.
- **Keep the mock honest.** A new backend behaviour gets a contract test in `backend.test.ts` and the
  same behaviour in Rust – otherwise the frontend tests would pass against a backend that does not
  exist.

## Manual checks before a release

Some things only a real desktop shows: `ssh <alias>` against a real server, `sftp://<alias>` in
Dolphin, a push in two client folders with different accounts, a clone through the helper, a key
with a passphrase through ksshaskpass, and `keybund doctor` green on a fresh user account.
