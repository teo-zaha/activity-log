# api

Rocket HTTP API for `activity-log`.

## Responsibilities

- Expose `GET /api/activity`
- Expose `POST /api/collect`
- Keep the latest collected log in memory
- Return `202` while a collection is already in progress
- Trigger the initial 30-day collection on startup

## Important entry points

- `src/lib.rs`: routes, app state, and Rocket builder
- `src/main.rs`: loads environment variables and launches Rocket

## Notes

- This crate is API-only. It does not serve Angular static assets.
- The actual collection work is delegated to `core`.
