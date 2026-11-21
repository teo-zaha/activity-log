# cli

Standalone Rust command-line interface for `activity-log`.

## Responsibilities

- Run activity collection without starting the API server
- Write JSON or Markdown output files, or the export alone to standard output (`--output -`)
- Say how each source did, and exit 1 when one failed
- Resolve relative date presets such as `today`, `week` (from Monday), and `lastmonth`
- Provide a separate Rust binary that generates TypeScript DTO files for the Angular app

## Important entry points

- `src/main.rs`: argument parsing and collection execution
- `src/generate_ts.rs`: dedicated DTO generation binary and shared generation logic

## Notes

- `cargo run -p cli --bin generate-ts --` is the explicit contract sync point between the Rust backend and Angular frontend.
- Collection and formatting logic come from `core`, so CLI output should match API-backed behavior.
