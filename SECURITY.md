# Security policy

## Reporting a vulnerability

Please report security problems privately through GitHub's
[private vulnerability reporting](https://github.com/wiwiwi-inc-tm/activity-log/security/advisories/new)
(the repository's **Security** tab › **Report a vulnerability**). Do not open a public issue, pull
request or discussion for it.

Say what an attacker needs to start with, what they get, and how to reproduce it on the latest
`main`. Leave real tokens, hostnames and activity out of the report; demo values are enough. You
will get an answer in the advisory thread; a fix is prepared there and released before the details
are made public.

Security fixes go to the latest release and to `main`. Older releases are not patched; update to
the latest one.

## What Activity Log protects

Activity Log runs on your own machine and reads your own activity. What matters is that nothing
reaches anyone you did not configure:

- **Secrets.** GitLab, GitHub, Jira and Figma tokens and calendar feed addresses in `config.toml`
  are sealed with a key kept in the OS secure store (Keychain, Credential Manager, Secret Service),
  or a key file where there is none, and bound to the server they were entered for ([ADR 0006](docs/adr/0006-secure-secrets.md)). A
  token readable as plain text in the file, in a log line, in an error message, in the cache or in an
  export, or sent to another origin than the one it was configured for, is a vulnerability.
- **Local files.** The settings, the cache and the key file are written owner-only. A path where
  another account on the same machine can read or replace them is a vulnerability.
- **The dashboard.** Text from a source (commit messages, issue titles, meeting descriptions) is
  untrusted. Script running from it in the dashboard or the desktop app, or Markdown in an export
  that changes the document's structure, is a vulnerability.
- **Links.** `activity-log:` deep links and links in entries only navigate. One that does more is a
  vulnerability.

## Intended behaviour

- The API server (`cargo run -p api`) has no authentication. It listens on `localhost` only and is
  meant for the browser dashboard on the same machine; exposing it on a network is a configuration
  choice, not a vulnerability.
- Whoever runs as your OS user can read what you can: your config, your cache and the key in your
  secure store. An attacker who already runs code as you is out of scope.
- The servers you configure (GitLab, GitHub, Jira, Figma, a calendar feed) are trusted with the token you gave
  them. What their answers can make Activity Log do beyond showing data stays in scope.
