# api

Rocket HTTP API for `activity-log`.

## Responsibilities

- Expose `GET /api/activity`
- Expose `GET /api/providers`
- Expose `POST /api/collect`
- Expose `GET /api/logs` and `DELETE /api/logs` (diagnostic log from `core::diagnostics`)
- Preserve the HTTP contract over the shared collection runtime from `core`
- Return `202` while a collection is already in progress
- Trigger the initial 30-day collection on startup

## Important entry points

- `src/lib.rs`: routes and Rocket builder
- `src/main.rs`: loads environment variables and launches Rocket

## Notes

- This crate is API-only. It does not serve Angular static assets.
- The actual collection work and runtime state are delegated to `core`.
- Provider health uses `scopeMissing` when the remote server confirms that the token lacks a scope. A busy answer still uses `throttled`. An optional `fixAction` uses the web action shape on this API. Native targets are available only in the macOS desktop app.
- An unavailable Calendar feed can include a `feedHelp` action with instructions and public links to calendar settings. The `feedUnavailable` problem covers HTTP 401, 403 and 404 when the host is not busy. The reason retains the HTTP status and the original host.
