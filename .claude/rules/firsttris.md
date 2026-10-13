# Shared rules for Claude

<!-- Copied verbatim from firsttris/workflows (templates/rules.md) into every project as
.claude/rules/firsttris.md; change it there, not in the project. -->

These rules hold in every firsttris project. The project's own `CLAUDE.md` adds what is specific
to it: where it says otherwise, it wins.

## Before a change

- Read the documentation pages in `docs/` that touch the change (the project's `CLAUDE.md` names
  them). Links in the README and the docs point to the GitHub Pages site, not to `docs/*.md`; a new
  page also goes into `nav` in `mkdocs.yml`.
- Base details on the sources, not on memory: the specifications, man pages and upstream code the
  project's `CLAUDE.md` lists. Clone them shallowly (`git clone --depth 1`) into the scratchpad and
  name the source file in comments and commit messages where it settles a detail.

## With every change

Update what the change affects, in the same pull request, and only that:

- the tests for what changed; every bug fix comes with a test that fails without it;
- the docs in `docs/` that describe it;
- the README when a feature a user sees is added, changed or removed;
- every message catalog (`messages/{de,en}.json` or `_locales/`) for every new text;
- the screenshots after a change to the look: **Actions → Update screenshots → Run workflow**, then
  look at the new pictures.

## Checks

Run the project's checks (its `CLAUDE.md` lists them) and get them green before every push, plus
`mkdocs build --strict` when `docs/` or `mkdocs.yml` changed. After the push, check CI; a red CI is
the next task.

## Pull requests

- One feature per pull request; it carries only its own commits.
- Before every push check whether the branch's pull request is still open. If it is merged, start
  again from the current `origin/main` and open a **new** pull request.
- Merge with squash. Releases only when the user says so.

## Working rules

- Commands as root (`sudo`, `pkexec`) are run by the user; don't work around that.
- The user's running processes are not stopped; use other ports for tests.
- Tests never touch the real home directory or the user's configuration; they work in temporary
  directories.
- Code, comments, docs, commits and pull requests in English; talk to the user in German.

## Code style

Technology choices follow `tech-stack.md` in firsttris/workflows; the project's `CLAUDE.md` names
its deviations and why.

Clean Code, written functionally: clear names, small functions that do one thing, dependencies
passed in, no duplication or dead code, comments that say why; the ideas behind SOLID and the
design patterns are welcome as functions, data and closures, but not as classes, inheritance or
class hierarchies.

Functional, not object-oriented: pure functions that take data and return data, side effects at
the edges; function components and hooks, no classes; errors as values. A file does one thing:
split it past ~400 lines, a function past ~80. A refactor changes no behaviour.
