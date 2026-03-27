# Monlist2.exe Monlist Server Communication

## Scope

This document summarizes the Monlist server communication recovered from static analysis of:

- `/Users/crusher/Desktop/Monlist/Monlist2.exe`

Bugzilla and Mantis integrations are intentionally excluded. Per user clarification, `/api.xml` is the Monlist API.

## Confidence Level

The binary exposes part of the Monlist server protocol, but not a complete self-describing contract.

- `Confirmed` means the string or behavior was directly present in the executable
- `Inferred` means the behavior is a cautious deduction from those strings

## Confirmed Monlist Server Findings

### Server Configuration

The binary contains these Monlist-specific labels:

- `Monlist server`
- `Monlist password`
- `server`
- `Server`
- `fServer`

This indicates the application expects a server address and a Monlist-specific password.

### Monlist XML API Surface

The binary contains the Monlist XML API:

- `/api.xml`
- `?cmd=`
- `logon`
- `logoff`
- `email`
- `password`
- `name`
- `<token>`
- `token`
- `<response>`
- `<error`

This is the clearest protocol-level Monlist API surface exposed by the executable.

### Confirmed XML Commands

The following command names and response item tags are embedded:

- `listProjects` -> `<project>`
- `listAreas` -> `<area>`
- `listCategories` -> `<category>`
- `listPriorities` -> `<priority>`
- `listPeople` -> `<person>`
- `<minversion>`
- `<version>`
- `<url>`

The binary also contains these related field names:

- `sProject`
- `sArea`
- `sCategory`
- `sPriority`
- `sFullName`

### Confirmed XML Submission Fields

The executable includes a request or data mapping with these fields:

- `title`
- `description`
- `project`
- `area`
- `assignTo`
- `version`
- `computer`
- `email`
- `fields`
- `attachFiles`
- `attachSendAs`
- `attachFields`
- `<case ixBug=`

Even though one internal routine name references bug upload, the transport itself is part of the same `/api.xml` command-based interface and therefore belongs to the Monlist communication model.

### Server Communication Status Text

The binary contains these connection and transfer messages:

- `Connecting to server...`
- `Fehler beim Senden an den Server!`
- `Fehler beim Senden an den Server! (Speichern der Stundendaten)`

These messages strongly suggest that the application uploads work or time data to a Monlist backend and has dedicated error handling for failed submissions.

### Monlist Data Domain

The executable contains several work-time and vacation related strings, including:

- `Stundenaufstellung`
- `Urlaub`
- `urlaubAlt`
- `urlaubNeu`
- `Urlaub (Alt / Neu):`
- `Project:`
- `Project no.:`
- `Personalnummer`

This supports the conclusion that the Monlist server communication is centered on time tracking, project assignment, and vacation-related data.

### Networking Hints

The binary imports or embeds networking-related components:

- `wsock32.dll`
- `Winapi.WinInet`
- `Winapi.UrlMon`
- proxy-related fields such as `proxyServer`, `proxyBypass`, `proxyUser`, `proxyPassword`

This confirms the application performs network communication and supports proxy environments.

## Inferred Monlist Server Behavior

Based on the recovered strings, the Monlist workflow likely looks like this:

1. the user configures a Monlist server address
2. the client calls `/api.xml?cmd=logon`
3. the user authenticates with a Monlist password and related account fields
4. the server returns a session token
5. the client loads projects, areas, categories, priorities, and people through command-based XML calls
6. the client submits time, project, or related records with optional attachments
7. the client reports an error if saving time data fails

## What Could Not Be Recovered Reliably

Static analysis of the executable did not reveal enough Monlist-specific detail to confirm:

- the full base URL that hosts `/api.xml`
- whether the transport is always HTTP or may also support HTTPS
- the exact request format for each `cmd`
- the full response schema for each command
- whether the `<case ixBug=...>` flow is used for time data, issue-like records, or both
- the exact authentication handshake

## Practical Conclusion

The Monlist-specific interface in `Monlist2.exe` is the command-based XML API at `/api.xml`.

What can be stated confidently is:

- the client connects to a configurable Monlist server
- it uses a Monlist password
- it exposes a command-based XML API at `/api.xml`
- it performs token-based login and logout
- it queries projects, areas, categories, priorities, and people
- it sends time, project, or related submission data
- it has explicit failure handling for saving that data to the server

## Recommended Next Step

If you want exact Monlist API documentation instead of a binary-based reconstruction, the next step should be live traffic capture on Windows while using `Monlist2.exe`. That would allow us to document:

- the real endpoint URLs
- request methods
- payload schema
- authentication flow
- success and error responses
