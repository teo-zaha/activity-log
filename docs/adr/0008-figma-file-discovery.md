# 0008. Figma finds activity in known files

Figma's REST API lists a file's versions and comments. It does not list a user's files or teams. Drafts belong to no team. A connection starts with the file URLs or keys the user enters, including links to branches. The token's identity determines whose versions and comments are collected.

A desktop tab is a discovery hint. Closing a tab must not remove that file's entries when cached days refresh. Desktop discovery therefore needs a file book scoped to the connection and its verified identity. Team discovery can use the same book to remember files and avoid listing every folder on every collection.

A Figma file can consume two rate-limited calls even before paging versions. Any failed request for a pasted file, including a 403, 404 or 429, fails the current range and identifies the file by its position in the list. The day cache then has no incomplete answer to save for that range. The file book will decide how to handle a discovered file that vanishes.

The read-only PAT check returned HTTP 200 for the readable file's metadata, versions and comments, and for the identity request. Access-denied response status and reply positioning remain unverified by that sample.
