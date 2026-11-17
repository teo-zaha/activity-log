# Provider connections in one config file

A provider kind (Git, GitLab, Jira, Calendar) can be configured more than once, and every setup is a connection identified by its kind and a stable name (`ConnectionId { source, name }`, shown as `gitlab/oss`). We chose this so a second GitLab account, a Jira site, a WSL distribution or another calendar fits the existing provider system instead of special-casing each source: config resolution, startup checks, status, cache and logs already worked per provider and now run per connection, while collectors stay unaware of connections.

Connections live in `config.toml`, one file shared by the CLI, the API server and the desktop app, with one array of tables per kind. We chose a file over encoding connections in environment variable names (such as `GITLAB_TOKEN__OSS`) so the CLI and the server stay simple to set up; the CLI edits the file with `sources` subcommands, the desktop through its settings, and both keep comments.

The file is the only list of sources. A new install has none and asks for a first one; a kind the file does not mention is not collected, and environment variables (`GITLAB_URL` and the like) no longer configure anything. We dropped implicit connections (Git on this computer, every WSL distribution, Outlook and System on by default) and the environment fallback because together they made it unclear where a source came from and why removing one needed a `kind = []` marker.

Git connections are locations (this computer, or a WSL distribution) rather than accounts. Emails found in any Git connection are pooled through a per-kind collection state and `finish_kind` step, so repositories without their own `user.email` keep matching across locations.

This replaces the per-distribution `WSL_GIT_DIRS` map, `WSL_DISTROS` and the CLI's `--set KEY=VALUE`, none of which were released.
