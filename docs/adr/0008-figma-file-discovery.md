# 0008. Figma finds activity in known files

Figma's REST API lists a file's versions and comments. It does not list a user's files or teams. Drafts belong to no team. A connection starts with the file URLs or keys the user enters, including links to branches. The token's identity determines whose versions and comments are collected.

A desktop tab is a discovery hint. On macOS and Windows, a connection with no pasted links can use open tabs belonging to its verified Figma account. After it verifies the account, the check accepts pasted links, matching tabs or an unpruned file-book record for that account. The connection needs configuration when none are available. A run reads the tabs once on its first cache miss and shares that snapshot across connections. Each connection uses only its own account's tabs. Older cached days can take up to seven days to refresh and include a newly discovered file.

The file book remembers files found through tabs, so closing a tab does not drop the file from the next refresh. Pasted links come from the configuration. The owner-only versioned book sits beside the day cache under `figma-<name>-<identity scope>.files.json` and survives edits to the pasted list. Once a file has gone 60 days without being seen in a tab and without contributing an entry to a fetched day, the book can forget it. A later refresh can then drop older activity from that file until it is discovered again.

A Figma file can consume two rate-limited calls even before paging versions. Any failed request for a pasted file fails the range and identifies its position in the list. A 404 for a discovered file marks it lost. A 403 does the same only when it is a refusal and a fresh identity check for the connection still succeeds. Collection skips lost files with a grouped warning and retries them at most once per day, or when a file returns after being absent from the previous tab snapshot. The same 60-day pruning rule applies. Throttling, server errors and transfer failures fail the chunk.

A fresh metadata response can rule out versions when the file was last modified before the chunk began. Each file read also gets a comments request.

A retry reservation is spent before the request. If that request is interrupted or fails without confirming access, the file can stay hidden until its next daily retry or tab reappearance. The grouped lost-files warning is emitted when the connection's collection state drops, including on cancellation.

The read-only PAT check returned HTTP 200 for the readable file's metadata, versions and comments, and for the identity request. Access-denied response status and reply positioning remain unverified by that sample.
