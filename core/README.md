# core

Shared backend crate for `activity-log`.

## Responsibilities

- Define the backend/domain models under `src/model/`
- Collect activity from Git, GitLab, Jira, Outlook, and Windows Event Log
- Merge Git commits with matching GitLab push events
- Format collected activity as Markdown
- Provide cache and date/range utilities
- Provide the shared in-memory collection runtime used by Rocket and Tauri
- Generate the TypeScript DTO contract consumed by the Angular app

## Important entry points

- `src/lib.rs`: orchestrates collection across all sources
- `src/collect/`: one collector per source
- `src/model/`: shared models, grouped by domain/source
- `src/runtime/`: reusable collection runtime and request validation
- `src/ts.rs`: generated TypeScript contract source

## Notes

- `api`, `cli`, and the Tauri desktop crate all depend on this crate so collection behavior stays identical across runtimes.
- Windows-specific integrations for Outlook and system events are intentionally implemented via PowerShell for parity with the old backend.
