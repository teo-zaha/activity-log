# cli

Standalone Rust command-line interface for `activity-log`.

## Responsibilities

- Run activity collection without starting the API server
- Write JSON or Markdown output files
- Resolve relative date presets such as `today`, `week`, and `lastmonth`
- Generate Rust-backed TypeScript DTO files for the Angular app

## Important entry points

- `src/main.rs`: argument parsing, collection execution, and TypeScript generation

## Notes

- The `generate-ts` subcommand is the explicit contract sync point between the Rust backend and Angular frontend.
- Collection and formatting logic come from `core`, so CLI output should match API-backed behavior.
