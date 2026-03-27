# AGENTS.md

This file defines the working rules and architecture expectations for agents editing this repository.

## Overview

`activity-log` is split into a Rust backend workspace plus an Angular frontend:

- `core/`: shared Rust library with models, collectors, cache, merge logic, markdown export, and TypeScript generation
- `api/`: Rocket HTTP API
- `cli/`: Rust CLI package containing the user-facing collection binary and the TypeScript generator binary
- `dashboard/`: Angular application that consumes generated backend DTOs
- `desktop/`: Tauri desktop shell that wraps the Angular app and talks to Rust commands

The backend contract is owned by Rust. The frontend consumes generated DTOs and should not redefine backend models by hand.

## Project Architecture

### Rust workspace

- `core/src/model/`: shared models grouped by domain/source
- `core/src/collect/`: one collector per source
- `core/src/lib.rs`: orchestration entry point used by both `api` and `cli`
- `core/src/ts.rs`: generated TypeScript contract source
- `core/src/utils/`: date utilities, markdown formatting, merge logic
- `core/src/cache/`: local file cache

### API

- `api/src/lib.rs`: Rocket routes, state, startup collection
- `api/src/main.rs`: runtime bootstrap

Routes:

- `GET /api/activity`
- `POST /api/collect`

### CLI

- `cli/src/main.rs`: argument parsing and collection execution

Important behavior:

- `cargo run -p cli --bin generate-ts --` is the explicit sync point for Angular contract files
- `api` and `cli` must use `core` for all shared logic

### Angular

- `dashboard/src/app/generated/`: generated backend DTOs, not handwritten
- `dashboard/src/app/services/`: data access and app state
- `dashboard/src/app/components/`: UI
- `dashboard/src/app/models/`: Angular-only view models
- `dashboard/src/app/utils/`: Angular-only helper logic

## Rust Rules

- Keep shared domain and API types in `core/src/model/`, not in `api` or `cli`
- Put each model struct in its own file under the matching domain/source folder
- For Rust modules with local tests, colocate them as a folder pair: `mod.rs` plus `tests.rs` in the same directory
- Do not keep Rust code in `foo.rs` while its tests live in `foo/tests.rs`; move the implementation into `foo/mod.rs` instead
- Prefer moving reusable logic into `core` instead of duplicating behavior in `api` or `cli`
- Preserve wire compatibility for public API fields unless the task explicitly changes the contract
- Use comments sparingly and only where behavior is not obvious
- Keep PowerShell and subprocess integrations parity-focused unless the task explicitly replaces them

## Angular and TypeScript Rules

- Always maximize type safety
- Do not handwrite backend DTOs in Angular
- Do not add compatibility wrapper files that only re-export or re-import generated backend DTOs
- Keep generated files under `dashboard/src/app/generated/`
- Keep Angular-only types and helpers separate from generated backend contract types
- Prefer explicit imports from generated files for backend shapes
- Prefer discriminated union narrowing over `as` casts whenever possible
- Avoid `any`, avoid weakening types for convenience, and keep helper APIs typed as specifically as possible
- Preserve existing Angular patterns unless the task explicitly calls for a redesign

## Script Rules

Use the namespaced root scripts so intent is obvious:

- `pnpm api:*`
- `pnpm cli:*`
- `pnpm core:*`
- `pnpm desktop:*`
- `pnpm dashboard:*`
- `pnpm types:generate`
- `pnpm verify`

Avoid reintroducing vague root scripts like `start`, `build`, or `collect` when a scoped script is clearer.

## Verification Rule

Always run the repo verification gate before considering a change complete:

```bash
pnpm verify
```

If `pnpm verify` cannot be run in the current environment, say so explicitly in the final handoff.

## Change Rules

- Do not reintroduce the old TypeScript backend
- Do not create thin migration or compatibility files when the code can be migrated directly
- Update docs when architecture, scripts, or workflow change
- Prefer deleting obsolete files over keeping dead compatibility layers
- Treat `desktop/gen/` as generated Tauri schema output: do not hand-edit it and keep it ignored
