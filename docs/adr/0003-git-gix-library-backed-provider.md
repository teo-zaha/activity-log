# Git collection via `gix`

Git stays a provider owned by `core`, but its collection path moves from spawning the `git` executable to using the embedded Rust `gix` library directly. We are choosing this so `cli`, `api`, and `desktop` keep the same shared provider behavior while Git no longer depends on `git` being installed, preserves its current `GitEntry` contract, and keeps repo collection aligned with strict Git-like trust checks.
