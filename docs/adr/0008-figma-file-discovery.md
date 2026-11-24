# 0008. Figma finds activity in known files

Figma's REST API lists a file's versions and comments. It does not list a user's files or teams. Drafts belong to no team. A connection starts with the file URLs or keys the user enters, including links to branches. The token's identity determines whose versions and comments are collected.

A desktop tab is a discovery hint. Closing a tab must not remove that file's entries when cached days refresh. Desktop discovery therefore needs a file book scoped to the connection and its verified identity. Team discovery can use the same book to remember files and avoid listing every folder on every collection.

A Figma file can consume two rate-limited calls even before paging versions. If Figma refuses access or throttles a request, the collector fails the current range so the day cache cannot store it as complete with missing entries. A file the API reports as missing is skipped and named by its position in the configured list.
