# Testing Windows features from a Mac

`activity-log`'s Windows features are **live OS-service integrations**, not
subprocess/PowerShell:

- **Win32 Event Log** via the `windows` crate (`EvtQuery`/`EvtNext`/`EvtRender`)
  — `core/src/platform/windows.rs`, driven by `core/src/collect/system/windows.rs`.
- **Outlook COM automation** via `IDispatch` (`CoCreateInstance(Outlook.Application)`)
  — `core/src/collect/outlook/windows.rs` + `core/src/platform/windows.rs`.

Both are `#[cfg(target_os = "windows")]`-gated. On a Mac they aren't compiled:
each parent module's `imp` alias picks its `macos` implementation instead (the
`fallback` on other targets). To **run** them you need a real Windows kernel
(Event Log service) and, for Outlook, an installed Outlook.

## Why Docker / Wine can't run these

- **Docker Desktop on a Mac runs Linux containers only** (the daemon is
  `linux/arm64`). Windows containers require a Windows host — impossible here.
- **Wine is not Windows.** It reimplements the Win32 API surface but has **no
  Event Log service (`wevtapi`) and no registered Outlook COM server** — exactly
  the two things we integrate with. A Wine "desktop" (in Docker or native) tests
  essentially none of the real Windows behavior.
- Compile-checking doesn't need Docker either: cargo-xwin cross-compiles to
  the real MSVC target natively (see Tier 1).

## The three tiers

|                                  | Portable parse/query | Win32 Event Log (live) | Outlook COM (live)          | Win `cfg` compile-check |
| -------------------------------- | -------------------- | ---------------------- | --------------------------- | ----------------------- |
| **Tier 0 — `cargo test` on Mac** | ✅ runs today        | ❌                     | ❌                          | ❌                      |
| **Tier 1 — `cargo-xwin` on Mac** | ❌                   | ❌                     | ❌                          | ✅ (the real value)     |
| **Tier 2 — Vagrant Windows VM**  | ✅                   | ✅                     | ✅ _(if Outlook installed)_ | ✅                      |

### Tier 0 — Portable logic (runs on your Mac now)

The pure functions (query building, XML/JSON parsing, normalization) are
compiled under `#[cfg(any(target_os = "windows", test))]`, so they run in plain
`cargo test` on macOS:

```bash
pnpm core:test      # or: pnpm verify
```

Covered in `core/src/collect/system/tests.rs` and
`core/src/collect/outlook/tests.rs`.

### Tier 1 — cargo-xwin compile-check (no Docker)

Catches Windows `cfg`/compile breakage that otherwise only surfaces on a real
Windows build, without leaving your Mac. Builds against the real
`x86_64-pc-windows-msvc` target (Microsoft CRT/SDK are downloaded by
[cargo-xwin](https://github.com/rust-cross/cargo-xwin) on first run), so it
covers the whole workspace including the Tauri desktop crate.

```bash
nix develop .#windows -c pnpm win:check              # clippy -D warnings, all crates + tests
nix develop .#windows -c pnpm desktop:build:windows  # NSIS installer
```

Notes:

- The `windows` flake shell provides the Rust MSVC target, cargo-xwin, LLVM
  (`clang-cl`, `lld-link`, `llvm-rc`) and NSIS.
- Tests are compiled, not run — there is no Windows runtime on the Mac. Use
  Tier 2 or the `Windows` CI workflow to execute them.
- MSI (WiX) bundles can only be produced on a Windows host.

### Tier 2 — Real Windows VM (the source of truth for live features)

A committed VMware Fusion + Packer setup boots Windows 11 (ARM) and runs the
live collectors, driven from your Mac terminal. Full setup and commands:
[`infra/windows-vm/README.md`](../infra/windows-vm/README.md).

```bash
pnpm win:box:build   # one-time: build the Windows 11 ARM box (needs a Win 11 ARM ISO)
pnpm win:up          # boot + provision (Rust + MSVC build tools)
pnpm win:test        # rsync working tree → cargo test in the VM (live Event Log)
pnpm win:halt
```

**Outlook COM** needs Outlook installed + signed in inside the VM. That's a
manual, licensed step — provisioning cannot legally auto-install Outlook. The
Event Log path works out of the box.

## Alternatives & future options

- **Intel Macs**: skip Packer — a public x86 Windows Vagrant box works directly
  with a VirtualBox/VMware provider. The committed config targets Apple Silicon
  (Windows-on-ARM), which has no public turnkey box.
- **Synced folder**: we use one-way `rsync` because VMware HGFS shared folders
  are unreliable on a Windows-on-ARM guest. SMB or provider-native shared
  folders are alternatives if you need two-way sync.
- **GitHub Actions `windows-latest`**: wired up as
  [`.github/workflows/windows.yml`](../.github/workflows/windows.yml) — runs
  clippy + `cargo test` on real Windows for the whole workspace, the desktop
  crate included, on every push/PR (hosted runners have no Outlook, so COM
  stays uncovered there). `.github/workflows/linux.yml` does the same for
  Linux, the desktop's tests left out.
- **Self-hosted Windows runner with Outlook**: would enable automated COM tests.
