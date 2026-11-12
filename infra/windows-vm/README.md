# Windows test VM (VMware Fusion + Packer)

A committed Windows 11 (ARM) VM for exercising `activity-log`'s **live** Windows
features — Win32 Event Log and Outlook COM — from a Mac. These cannot run under
Docker or Wine (no Event Log service, no Outlook COM server), so a real Windows
kernel is required.

## One-time setup

1. Install **VMware Fusion** (free for personal use).
2. Install **Vagrant**, the `vagrant-vmware-desktop` plugin, and the VMware
   Vagrant Utility.
3. Supply a **Windows 11 ARM64 ISO** at `packer/iso/Win11_ARM64.iso` (set
   `iso_path`/`iso_checksum` in `packer/windows11-arm.pkr.hcl` or via `-var`).
4. Build the base box: `pnpm win:box:build`, then
   `vagrant box add activity-log/windows-11-arm packer/activity-log-windows-11-arm.box --force`.

## Daily use (from the repo root)

| Command                              | Action                                                           |
| ------------------------------------ | ---------------------------------------------------------------- |
| `pnpm win:up`                        | Boot + provision the VM (installs Rust + MSVC build tools)       |
| `pnpm win:test`                      | rsync the working tree → `cargo test` (live Event Log) in the VM |
| `pnpm win:collect`                   | Run the CLI collector in the VM                                  |
| `pnpm win:halt` / `pnpm win:destroy` | Stop / delete the VM                                             |

The repo is mounted read-only-ish via one-way **rsync** at `C:\vagrant`; edits
are pushed by the `vagrant rsync` step inside `win:test`/`win:collect`.

**Outlook COM** requires Outlook installed + signed in inside the VM (a manual,
licensed step — it is not auto-provisioned).

See [`docs/testing-windows-on-mac.md`](../../docs/testing-windows-on-mac.md) for
the full strategy, including the cargo-xwin compile-check tier and the
Docker/Wine limitations.
