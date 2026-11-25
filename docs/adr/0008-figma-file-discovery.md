# 0008. Figma finds activity in known files

Figma's REST API lists a file's versions and comments. The user can enter file links or keys, including branch links, or team links or IDs from their own organization. Figma provides no list of the user's teams. Drafts belong to no team. The token's identity determines whose versions and comments are collected.

A desktop tab is a discovery hint. After it verifies the account, the check accepts pasted files, configured teams, matching open tabs on macOS or Windows, or a live tab-discovered record in the file book for that account. The connection needs configuration when none are available. A run reads the tabs once on its first cache miss and shares that snapshot across connections. Each connection uses only its own account's tabs. Older cached days can take up to seven days to refresh and include a newly discovered file.

The file book remembers files found through tabs and team listings. Pasted links come from the configuration. The owner-only versioned book sits beside the day cache under `figma-<name>-<identity scope>.files.json` and survives changes to both lists. The book can forget a file after 60 days without a sighting or a contribution to a fetched day. A later refresh can then drop older activity from that file until it is discovered again.

Team discovery uses the v2 folder endpoints with `folders:read`. For a run that fetches today or yesterday in UTC, a complete folder listing is reused for 24 hours and each folder's files are listed once. An older-only run uses the book and discovers teams without a saved tree. A non-throttled 403 or 404 listing files in a saved folder triggers one rediscovery. A failed or paged listing preserves the previous complete set. Invalid file keys are skipped with one warning per run.

Concurrent runs can both start a traversal before either publishes. The newer completed tree replaces the previous tree as a whole, and each file keeps the latest sighting per team with its `last_modified`. A removed team stops selecting files immediately. Its sighting and folder records age out after 60 days. A sighting in a configured team selects a file when its reported modification is on or after the fetched range's start. A file is also read whenever its tab record is eligible under the tab retention rule, even if the team sighting is older than the range. A comment on an otherwise untouched team file may be missed if commenting leaves `last_modified` unchanged. Pasted files are read for every fetched range.

The team fields are additive in book version 1. Books written before team discovery load with empty team observations. An older build that rewrites a book drops the team fields, which is acceptable before the feature ships.

Any failed request for a pasted file or configured team's listing fails the range and identifies its position in the list. A 404 for a discovered file marks it lost. A 403 does the same only when it is a refusal and a fresh identity check for the connection still succeeds. Collection skips lost files with a grouped warning and retries them at most once per day, or when a file returns after being absent from the previous tab snapshot. The same 60-day pruning rule applies. Throttling, server errors and transfer failures fail the chunk.

A fresh metadata response can rule out versions when the file was last modified before the chunk began. Each file read also gets a comments request.

A retry reservation is spent before the request. If that request is interrupted or fails without confirming access, the file can stay hidden until its next daily retry or tab reappearance. The grouped lost-files warning is emitted when the connection's collection state drops, including on cancellation.

The read-only PAT check returned HTTP 200 for the readable file's metadata, versions and comments, and for the identity request. Access-denied response status and reply positioning remain unverified by that sample.

## Open live check for team discovery

The team-listing fixtures follow Figma's OpenAPI spec. A read-only check with a PAT that has `folders:read` is deferred until after merge. [Figma OpenAPI spec](https://github.com/figma/rest-api-spec/blob/main/openapi/openapi.yaml)

- For `GET /v2/teams/:team_id/folders`, confirm the exact envelope, including a `folders` array with string IDs and an explicit null `parent_folder_id`. Confirm that it returns top-level folders only.
- For `GET /v2/folders/:folder_id/folders`, confirm a `folders` array with string IDs and `parent_folder_id` equal to the requested folder's ID. Check a leaf folder and a plan without nested folders if available.
- For `GET /v2/folders/:folder_id/files`, confirm a `files` array with string `key` and non-null RFC 3339 `last_modified` fields.
- Check empty-list shapes and pagination on all three endpoints. Record the rate tier and, if throttling occurs, the response status and rate-limit header names.
- Confirm response statuses and error-body field names for a token without `folders:read`, an unreadable team, a deleted folder and an invite-only folder.
- Check whether `/meta`, `/versions` and `/comments` work for each listed file type, including FigJam and Slides. Compare `last_modified` before and after commenting on an otherwise unchanged file.
- Record request counts and elapsed time on the configured team. Share only shapes, statuses, counts and timings.
