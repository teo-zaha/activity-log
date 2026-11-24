# Contributing

Thanks for looking. Activity Log is a small project with one maintainer, so review time is the
scarce part: a focused change with evidence gets in much faster than a large one.

## Before you start

- **Bugs:** open an [issue](https://github.com/teo-zaha/activity-log/issues/new/choose) with the
  steps, or a pull request for a small, obvious fix.
- **Features, new sources, larger refactors:** open an issue first and wait for an answer, so
  nobody spends a weekend on a change that cannot be merged.
- **Security problems:** never in public; see [SECURITY.md](SECURITY.md).

## The rules live in AGENTS.md

[`AGENTS.md`](AGENTS.md) (also `CLAUDE.md`) is the one place for the architecture and the code
rules, for people and coding agents alike: where each piece lives, the Rust and Angular rules, and
the code quality bar a review holds every change to. Read the parts your change touches. In short:
the backend contract is owned by Rust and generated for the dashboard, cases are told apart by types
and never by text, and a change updates the docs it makes wrong. Domain words are in
[`CONTEXT.md`](CONTEXT.md).

## Set up

You need Rust, Node.js and pnpm as the [README](README.md#requirements) lists, or
[Nix](https://nixos.org): `nix develop` gives the whole toolchain.

```bash
pnpm install
pnpm dashboard:browsers   # once: the headless Chromium the layout tests use
pnpm dev                  # API and dashboard with a dev profile of their own
pnpm desktop:dev          # or the desktop app
```

The development scripts use a separate dev profile, so they never touch the settings, cache or
secrets of an installed Activity Log (see [Dev profile](README.md#dev-profile)).

## Before you open a pull request

Run the verification gate and make it pass:

```bash
pnpm verify
```

It checks the generated TypeScript contract, formatting, clippy, the Rust tests, the dashboard's
lint, unit, layout and type checks, and the dashboard build. If you changed a Rust type the
dashboard reads, run `pnpm types:generate` and commit what it writes.

`pnpm verify` covers the system you run it on. CI runs Linux on every pull request; Windows-gated
code, file and path handling, the desktop crate and the secret store also need the Windows (and for
the keychain, macOS) workflow, which the maintainer runs for your branch.

## Pull requests

- **One problem per pull request.** Include everything that fix needs (Rust, the generated contract,
  the dashboard, tests, docs); put a cleanup or a second fix in its own.
- **Tests that fail without the change.** They assert kinds and results, not wording.
- **Show that it works.** Say how you found the problem, how you checked the fix and what you saw.
  For UI changes add before and after screenshots, and a short recording for motion. Use demo data,
  never your real commits, issues or meetings, and upload them to the pull request rather than
  committing them.
- **Commit messages** follow [Conventional Commits](https://www.conventionalcommits.org):
  `fix(gitlab): …`, `feat(dashboard): …`, `docs: …`. The changelog is generated from them
  (`cliff.toml`).
- **Coding agents** are welcome. Say in the description which model and tool wrote the change; you
  are still the one who answers for it in review.

By contributing you agree that your contribution is licensed under the [MIT License](LICENSE).
