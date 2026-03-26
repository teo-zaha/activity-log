# Code Review

Review the changes on the current branch against `main`. Evaluate each area below and give a short verdict (✅ pass, ⚠️ nit, ❌ issue) with an explanation when something needs attention.

## 1 — Correctness

- Does the code do what the PR/commit description says?
- Are edge cases handled (empty collections, missing env vars, network errors)?
- Are there off-by-one errors, unchecked unwraps, or silent failures?

## 2 — Architecture & Boundaries

- Shared domain types live in `core/src/model/`, not duplicated in `api/` or `cli/`.
- Reusable logic belongs in `core`, not copied across crates.
- Angular consumes only generated DTOs from `dashboard/src/app/generated/`—no hand-written backend types.
- No thin compatibility or re-export wrapper files.

## 3 — Type Safety

- Rust: no unnecessary `.unwrap()`, `.clone()`, or `as` casts.
- TypeScript: no `any`, no weakened types, prefer discriminated-union narrowing over `as`.
- Generated DTO contract is not manually edited.

## 4 — Tests

- Rust tests are colocated (`mod.rs` + `tests.rs` in the same directory).
- New logic has at least one happy-path and one error-path test.
- Existing tests still pass (`pnpm verify`).

## 5 — API & Wire Compatibility

- Public API fields preserve wire compatibility unless the task explicitly changes the contract.
- If the contract changed, `pnpm types:generate` was re-run and the generated Angular types are up to date.

## 6 — Style & Naming

- Follows existing patterns (namespaced scripts, module layout, comment policy).
- Comments only where behavior is non-obvious.
- No leftover TODOs, debug prints, or commented-out code.

## 7 — Security & Config

- No secrets, tokens, or `.env` values committed.
- Environment variables documented in `.env.example` if new ones were added.

## Summary

End with a concise summary table:

| Area         | Verdict | Notes |
| ------------ | ------- | ----- |
| Correctness  |         |       |
| Architecture |         |       |
| Type Safety  |         |       |
| Tests        |         |       |
| Wire Compat  |         |       |
| Style        |         |       |
| Security     |         |       |

Then list any blocking issues and suggested follow-ups.
