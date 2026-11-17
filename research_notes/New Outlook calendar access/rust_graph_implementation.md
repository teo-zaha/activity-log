# Rust + Tauri 2 implementation path for reading Microsoft 365 calendar via Microsoft Graph (as of 2026-11-16)

Version and date data comes from the crates.io API (`/api/v1/crates/{name}` and `/versions`), queried on 2026-11-16. "Released" means the `created_at` of that version.

## Which Rust crates should handle OAuth (public client, PKCE, loopback, device code)?

### Takeaway

Use `oauth2` 5.0.0 (or `openidconnect` 4.1.0 if you want ID-token validation) and point it at the Entra v2 endpoints yourself. It covers PKCE, refresh tokens and the device authorization grant. Neither of the "Azure" options fits. `azure_identity` 1.0 has no interactive or public-client user credential, and `graph-rs-sdk`/`graph-oauth` has been inactive since 2025. No official MSAL for Rust exists. The only crate named `msal` is a stub from Samba's libhimmelblau (Linux/Himmelblau-focused). For the loopback redirect, use `tauri-plugin-oauth` 2.1.0, or just bind a `TcpListener` on 127.0.0.1 inside Rust core.

### Cited Findings

- `oauth2`: latest stable 5.0.0, released 2025-01-21. No newer release since, but the repo was still being pushed to (last push 2026-02-22) and has ~50 open issues. 65M downloads total. — [crates.io oauth2](https://crates.io/crates/oauth2); [GitHub ramosbugs/oauth2-rs](https://github.com/ramosbugs/oauth2-rs)
- `oauth2` 5 features:
  - PKCE via `PkceCodeChallenge::new_random_sha256()`, `set_pkce_challenge` and `set_pkce_verifier`.
  - Device flow via `exchange_device_code()` → `StandardDeviceAuthorizationResponse` (verification URI + user code), then polling with `exchange_device_access_token(&details)`.
  - `RefreshTokenRequest`.
  - Typestate endpoints (`EndpointSet`/`EndpointNotSet`/`EndpointMaybeSet`).
  - HTTP backends: `reqwest` ^0.12 (default, async plus `reqwest-blocking`), `curl`, and `ureq` ^2. Examples disable HTTP redirects to prevent SSRF.
  - The crate ships Microsoft device-flow and Microsoft Graph examples.
  - [docs.rs oauth2 5.0.0](https://docs.rs/oauth2/5.0.0/oauth2/)
- `openidconnect`: 4.1.0 released 2026-10-31 (4.0.1 on 2025-07-06), same author (ramosbugs), actively maintained. — [crates.io openidconnect](https://crates.io/crates/openidconnect)
- `graph-rs-sdk` 3.0.1 and `graph-oauth` 3.0.0 were both released 2025-04-20. The last commit was 2025-08-28 (doc/link cleanup). 36 open issues, not archived. ~150k downloads each. **Treat as stale/low-activity.** — [crates.io graph-rs-sdk](https://crates.io/crates/graph-rs-sdk); [GitHub sreeise/graph-rs-sdk](https://github.com/sreeise/graph-rs-sdk)
- `azure_identity`: 1.0.0 GA released 2026-05-12; 1.1.0-beta.2 on 2026-11-06. Exported credentials:
  - `AzureCliCredential`, `AzureDeveloperCliCredential`, `DeveloperToolsCredential`
  - `AzurePipelinesCredential`
  - `ClientAssertionCredential`, `ClientCertificateCredential`, `ClientSecretCredential`
  - `ManagedIdentityCredential`, `WorkloadIdentityCredential`
  - **No InteractiveBrowserCredential, no DeviceCodeCredential, no public-client user flow.**
  - [docs.rs azure_identity](https://docs.rs/azure_identity/latest/azure_identity/); [crates.io azure_identity](https://crates.io/crates/azure_identity)
- `msal` crate 0.8.19 (2026-05-06): description "Stub library for libhimmelblau", repo gitlab.com/samba-team/libhimmelblau. It is not a Microsoft MSAL port for Windows/macOS desktop apps. — [crates.io msal](https://crates.io/crates/msal)
- `msal-rs` 0.1.2, last released 2024-08-14, ~6.8k downloads. **Stale.** — [crates.io msal-rs](https://crates.io/crates/msal-rs)
- `tauri-plugin-oauth` 2.1.0 released 2026-07-07 (2.0.0 on 2024-11-05). Repo last pushed 2026-11-15. ~512k downloads.
  - What it does: "spawns a temporary localhost server to capture OAuth redirects".
  - Rust API is `tauri_plugin_oauth::start(move |url| …)`, which returns the port. JS API has `start`, `cancel`, `onUrl`, `onInvalidUrl`.
  - README warning: "Because of the unprotected localhost port, you must verify the URL here. Preferably send back only the token, or nothing at all if you can handle everything else in Rust."
  - [crates.io README tauri-plugin-oauth 2.1.0](https://crates.io/crates/tauri-plugin-oauth); [GitHub FabianLars/tauri-plugin-oauth](https://github.com/FabianLars/tauri-plugin-oauth)
  - Note: the GitHub main-branch README fetched via raw.githubusercontent described the project as "WIP / Tauri plugin not yet released". That conflicts with the published 2.x crates, so the crates.io README is the one to trust.
- `tauri-plugin-deep-link` 2.6.3 released 2026-10-28 (official plugins-workspace). Unlike the loopback approach, it can launch the app when no instance is running. — [crates.io tauri-plugin-deep-link](https://crates.io/crates/tauri-plugin-deep-link); [tauri-plugin-oauth README](https://crates.io/crates/tauri-plugin-oauth)
- Entra redirect URI rules (loopback):
  - `http` is allowed only for localhost.
  - "the port component … is ignored for the purposes of matching a localhost redirect URI", so `http://localhost:1234/MyApp` ≡ `http://localhost/MyApp`.
  - Don't register multiple localhost URIs that differ only by port; differentiate by path instead.
  - IPv6 `[::1]` is not supported.
  - Microsoft recommends `127.0.0.1` over `localhost`. But "You can't … use the Redirect URIs text box in the Azure portal to add a loopback-based redirect URI that uses the http scheme". You must edit `replyUrlsWithType` in the app manifest.
  - Device code flow needs no redirect URI.
  - A public client flow configuration must be enabled. Desktop apps register under the "Mobile and desktop applications" platform.
  - Max 256 characters per URI. 256 URIs (work/school only) or 100 (with MSA).
  - Query params in redirect URIs are not allowed if personal MSAs are enabled.
  - [Microsoft Learn: Redirect URI best practices](https://learn.microsoft.com/en-us/entra/identity-platform/reply-url)

### Inferences

- Recommended stack: `oauth2` 5 (auth code + PKCE, refresh, device code fallback) with `reqwest` 0.12 in `core`. The loopback listener lives in Rust core, not the webview, which matches the project rule that collection runs in core.
- The redirect URI registered in Entra should be something like `http://localhost` (Mobile & desktop platform). The app binds an ephemeral port and sends `http://localhost:{port}`, or binds `127.0.0.1` with a manifest-edited `http://127.0.0.1` entry.
- The browser is opened via the system browser (Tauri opener/shell). An embedded webview would lose SSO cookies and passkeys.
- `tauri-plugin-oauth` is a thin helper. A ~50-line `std::net::TcpListener`/`tokio` listener in `core` avoids a Tauri dependency, so the CLI can reuse it. The CLI can also use device code.
- Custom-scheme deep links (tauri-plugin-deep-link) add OS registration complexity and aren't needed, because Entra accepts loopback for public clients.
- For the PKCE auth code flow, `oauth2`'s client must be built without a client secret. Entra public clients send `client_id` in the request body. The fetched docs.rs page doesn't spell out whether `AuthType::RequestBody` is needed, so verify that empirically.

### Gaps

- No primary-source statement found on whether Entra treats `http://127.0.0.1:{port}` as port-agnostic like `localhost`. The doc says the port exception is "only true for localhost redirect URIs" while also recommending 127.0.0.1. Test it.
- Whether `tauri-plugin-oauth` binds to `127.0.0.1` or `localhost`, and how its port range is configured, wasn't confirmed from the README excerpt.
- No confirmation of an `oauth2` 5.x point release or a 6.0 in 2025–2026. crates.io shows none.

## Can/should the app call Windows WAM (WebAuthenticationCoreManager) directly from Rust?

### Takeaway

It's technically feasible. The `windows` crate projects WinRT `Windows.Security.Authentication.Web.Core`, and at least two Rust codebases do it (askWAM-rs, an IronRDP PR). But there's no maintained Rust library for it, and the app must register `ms-appx-web://Microsoft.AAD.BrokerPlugin/{client_id}` and pass a parent HWND. It's worth it only if target tenants enforce Conditional Access device compliance / token protection or users strongly expect silent SSO. Otherwise ship browser + PKCE first and add WAM later as a Windows-only `*_impl`.

### Cited Findings

- Microsoft: WAM is "a Windows 10+ component that acts as an authentication broker". Listed benefits:
  - "Enhanced security. See Token protection."
  - "Support for Windows Hello, Conditional Access, and FIDO keys."
  - "Fast single sign-on."
  - "Ability to sign in silently with the current Windows account."
  - [Microsoft Learn: Acquire token with WAM](https://learn.microsoft.com/en-us/entra/identity-platform/scenario-desktop-acquire-token-wam)
- WAM availability: "Windows 10 and later, and on Windows Server 2019 and later. On Mac, Linux, and earlier versions of Windows, MSAL automatically falls back to a browser." B2C and AD FS authorities are not supported. — [Microsoft Learn: WAM](https://learn.microsoft.com/en-us/entra/identity-platform/scenario-desktop-acquire-token-wam)
- The app registration must include the redirect URI `ms-appx-web://Microsoft.AAD.BrokerPlugin/{client_id}`. Interactive calls must be parented to the app window: "This is mandatory so that WAM is correctly parented to your app". Recommended pattern: silent with the cached account or `OperatingSystemAccount`, then interactive on UI-required. — [Microsoft Learn: WAM](https://learn.microsoft.com/en-us/entra/identity-platform/scenario-desktop-acquire-token-wam)
- Rust prior art:
  - **askWAM-rs**: a Rust port of Dirk-jan Mollema's askWAM, CC0 licensed. It's a Windows CLI that gets Entra access tokens via WAM silently using accounts signed in to Windows, taking `--client-id` and `--scope` (defaults: Teams client ID and the Graph scope). Its README doesn't show which crate/version of the WinRT bindings it uses. — [GitHub askWAM-rs](https://github.com/Harrison-Wells-Cyber/askWAM-rs)
  - **IronRDP PR #2247** (Devolutions): adds WAM silent and interactive token acquisition in Rust. Review notes say the parent HWND should be used for interactive requests. — [GitHub IronRDP PR #2247](https://github.com/Devolutions/IronRDP/pull/2247) (from search snippet; PR page not fetched)
- `windows` crate: latest 0.62.2, released 2025-10-06. — [crates.io windows](https://crates.io/crates/windows)
- Security research on WAM token abuse (askWAM originated as an offensive-security tool) suggests some EDR/security tooling scrutinizes this API pattern. — [Huntress: Stealing OAuth tokens through Microsoft's front door](https://www.huntress.com/blog/stealing-oauth-tokens-through-microsofts-front-door) (search result only, not fetched)

### Inferences

- Implementation sketch, unverified against crate docs:
  - Feature `Security_Authentication_Web_Core` plus `Security_Credentials`.
  - `WebAuthenticationCoreManager::FindAccountProviderWithAuthorityAsync("https://login.microsoft.com", "organizations")`.
  - Build `WebTokenRequest::Create(provider, scope, client_id)`, then call `GetTokenSilentlyAsync`.
  - On `UserInteractionRequired`, use `IWebAuthenticationCoreManagerInterop::RequestTokenForWindowAsync(hwnd, …)`, since an unpackaged Win32 app needs the HWND interop.
  - This requires the Tauri main window HWND, so it fits in `desktop/` or behind a callback, not in a headless CLI.
- Given the project's `*_impl` OS-dispatch rule, a `token_provider/windows` (WAM) module with a cross-platform browser/PKCE fallback is the natural shape.
- WAM manages its own token cache, so no refresh-token storage is needed on that path, which sidesteps the Credential Manager size problem below.
- macOS has an equivalent broker (Enterprise SSO plug-in via Company Portal), but it's only reachable through MSAL for ObjC/Swift. There's no Rust path, so macOS should use the browser + PKCE flow.

### Gaps

- No official Microsoft Rust sample for WAM. Exact `windows`-crate type names and feature flags weren't verified on docs.rs.
- Whether WAM tokens obtained by a third-party unpackaged Win32 app satisfy Conditional Access "compliant device" / token-protection policies wasn't documented in the fetched sources. The WAM page lists "Conditional Access" support generically.
- The macOS Enterprise SSO plug-in claim is from background knowledge and wasn't sourced in this session.

## How should OAuth tokens be stored securely?

### Takeaway

Use `keyring` 4.x (built on `keyring-core` 1.x plus per-platform store crates such as `windows-native-keyring-store`). Plan for Windows Credential Manager's blob cap of ~2560 bytes, where keyring counts UTF-16 bytes, so only ~1280 ASCII characters fit. Entra refresh tokens can exceed this. Either chunk across several entries, or store a DPAPI-encrypted (`CryptProtectData`) token file and keep only a key or nothing in Credential Manager. Avoid `tauri-plugin-stronghold`: maintainers called it not recommended and slated for removal in Tauri v3, and it's in a 3.0.0-alpha series.

### Cited Findings

- `keyring` 4.2.0 released 2026-08-29 (4.1.x through summer 2026). `keyring-core` 1.0.1 released 2026-10-13 (1.0.0 on 2026-04-21). Repo moved to `open-source-cooperative/keyring-rs`. — [crates.io keyring](https://crates.io/crates/keyring); [crates.io keyring-core](https://crates.io/crates/keyring-core)
- `windows-native-keyring-store` 1.1.0 (2026-05-24) is "a credential store for the keyring crate" backed by Windows Credential Manager. Details:
  - Built on `keyring-core ^1`; usage calls `keyring_core::set_default_store(...)`.
  - Entries are generic credentials named by prefix + user + delimiter (default ".") + service + suffix.
  - Default persistence is "Enterprise" (alternatives: "Session" and "Local", configurable).
  - Concurrent operations on the same entry from different threads may run out of order.
  - [docs.rs windows-native-keyring-store](https://docs.rs/windows-native-keyring-store/latest/windows_native_keyring_store/)
- keyring-rs's Windows backend checks `password.encode_utf16().count() * 2` against `CRED_MAX_CREDENTIAL_BLOB_SIZE` and returns `ErrorCode::TooLong`. — [docs.rs keyring 2.3.3 src/windows.rs](https://docs.rs/crate/keyring/2.3.3/source/src/windows.rs) (older version; via search snippet)
- Windows Credential Manager's practical blob cap is 2560 bytes. Other libraries (zalando go-keyring, R keyring) work around it by chunking. — [zalando go-keyring keyring_windows.go](https://www.github.com/zalando/go-keyring/blob/master/keyring_windows.go); [r-lib keyring development notes](https://rdrr.io/github/r-lib/keyring/f/inst/development-notes.md) (search snippets)
- Stronghold:
  - A Tauri maintainer said Stronghold "is no longer recommended and will be deprecated and therefore removed in v3". — [GitHub tauri-apps discussion #7846](https://github.com/orgs/tauri-apps/discussions/7846) (search snippet)
  - crates.io shows `tauri-plugin-stronghold` stable 2.4.0 plus 3.0.0-alpha.0–alpha.3 (Sept–Oct 2026), so the plugin still ships alongside Tauri v3 prereleases. — [crates.io tauri-plugin-stronghold](https://crates.io/crates/tauri-plugin-stronghold)
  - The two sources conflict on whether it's actually removed. Treat it as not recommended either way.
- `tauri-plugin-keyring` 0.1.0 is from 2024-12-23 with no repo listed. **Stale.** — [crates.io tauri-plugin-keyring](https://crates.io/crates/tauri-plugin-keyring)

### Inferences

- Size of Entra refresh tokens (typically ~1.5–2 KB of ASCII, sometimes more) versus ~1280 UTF-16 characters means a single `keyring` entry on Windows will likely hit `TooLong` for refresh tokens.
- Practical options on Windows:
  - (a) Chunk the token into N entries (`…/rt/0..n`) plus a count.
  - (b) Store `keyring` secrets as bytes, if the v4 API's `set_secret` stores raw bytes rather than UTF-16 (halving the overhead).
  - (c) Store the token JSON in a file under the app data dir, encrypted with DPAPI `CryptProtectData` via `windows::Win32::Security::Cryptography`. This is user-scoped and needs no extra crate.
- On macOS, the Keychain via `keyring` has no comparable small cap, so store directly.
- Store only refresh token, account id (home_account_id/oid+tid) and granted scopes. Access tokens can stay in memory, since they live ~60–90 minutes.
- Should not be in the per-day disk cache directory.

### Gaps

- Exact v4 `keyring` API behavior for binary secrets (`set_secret`) on the Windows store wasn't verified. The windows-native-keyring-store docs page doesn't mention size limits.
- No authoritative Microsoft statement of Entra refresh-token length was retrieved.

## How exactly to call Graph calendarView (ranges, paging, time zones, recurrence, cancellations, delta, throttling, permissions)?

### Takeaway

Call `GET /me/calendarView?startDateTime={RFC3339 Z}&endDateTime={RFC3339 Z}&$select=…` with `Prefer: outlook.timezone="UTC"`, and follow `@odata.nextLink` until it's absent. calendarView already expands recurring series into occurrences and exceptions. It returns any event overlapping the window. Query each calendar via `/me/calendars/{id}/calendarView` if non-default calendars matter. Least-privileged scope is `Calendars.ReadBasic`, which excludes body, attachments and extensions. Delta (`/me/calendarView/delta`) needs `Calendars.Read`, works only on the primary calendar, can't use `$select`, and pins one range per sync round, so it fits a per-UTC-day cache poorly. Outlook throttling is 10,000 requests per 10 minutes and 4 concurrent requests per app per mailbox, with 429 responses.

### Cited Findings

- Endpoints:
  - Default calendar: `GET /me/calendar/calendarView?…`, `GET /users/{id}/calendarView?…`.
  - Specific calendar: `GET /me/calendars/{id}/calendarView?…`.
  - Calendar in a group: `GET /me/calendarGroups/{id}/calendars/{id}/calendarView?…`.
  - calendarView returns "the occurrences, exceptions, and single instances of events in a calendar view defined by a time range".
  - [Microsoft Learn: List calendarView](https://learn.microsoft.com/en-us/graph/api/user-list-calendarview?view=graph-rest-1.0)
- Range semantics: "The values of startDateTime and endDateTime are interpreted using the timezone offset specified in the value and aren't impacted by the value of the Prefer: outlook.timezone header if present. If no timezone offset is included in the value, it is interpreted as UTC." — [Microsoft Learn: List calendarView](https://learn.microsoft.com/en-us/graph/api/user-list-calendarview?view=graph-rest-1.0)
- `Prefer: outlook.timezone` sets the time zone of start/end in the response: "If not specified, those time values are returned in UTC." `Prefer: outlook.body-content-type` selects text or html for body. — [Microsoft Learn: List calendarView](https://learn.microsoft.com/en-us/graph/api/user-list-calendarview?view=graph-rest-1.0)
- `$select` caveat: "The createdDateTime and lastModifiedDateTime properties of event don't support $select." Supports "some of the OData query parameters". — [Microsoft Learn: List calendarView](https://learn.microsoft.com/en-us/graph/api/user-list-calendarview?view=graph-rest-1.0)
- Permissions for calendarView:
  - Least privileged is `Calendars.ReadBasic` (delegated work/school and personal); higher privileged are `Calendars.Read` and `Calendars.ReadWrite`. — [Microsoft Learn: List calendarView](https://learn.microsoft.com/en-us/graph/api/user-list-calendarview?view=graph-rest-1.0)
  - `Calendars.ReadBasic` (delegated, ID 662d75ba-a364-42ad-adee-f5f880ea4878): "Allows the app to read events in user calendars, except for properties such as body, attachments, and extensions." No admin consent required, MSA-consentable. — [Microsoft Learn: Permissions reference](https://learn.microsoft.com/en-us/graph/permissions-reference)
  - `Calendars.Read` (delegated, 465a38f9-76ea-45b9-9f34-9e8b0d4b0b42): "Allows the app to read events in user calendars." No admin consent (delegated). — [Microsoft Learn: Permissions reference](https://learn.microsoft.com/en-us/graph/permissions-reference)
  - Conflict: the calendarView page lists an _Application_ `Calendars.ReadBasic`, while the permissions reference shows ReadBasic as delegated-only. Irrelevant for a delegated desktop app.
- Event properties relevant to activity logging:
  - `isCancelled`: "Set to true if the event has been canceled."
  - `isAllDay`: start/end "must be set to midnight and be in the same time zone".
  - `showAs`: `free`, `tentative`, `busy`, `oof`, `workingElsewhere`, `unknown`.
  - `type`: `singleInstance`, `occurrence`, `exception`, `seriesMaster`.
  - `responseStatus`, `seriesMasterId`.
  - `start`/`end` (`DateTimeTimeZone`, "By default, the end time is in UTC").
  - `iCalUId` ("different for each occurrence in a recurring series").
  - `id`: changes when moved between containers unless `Prefer: IdType="ImmutableId"`.
  - `onlineMeeting` (use `joinUrl`; `onlineMeetingUrl` "will be deprecated").
  - `isOnlineMeeting`, `onlineMeetingProvider`, `organizer`, `isOrganizer`, `sensitivity`, `categories`, `originalStart`.
  - `webLink` opens only legacy OWA. Current format: `https://outlook.office365.com/calendar/item/{url-encoded id}`.
  - [Microsoft Learn: event resource](https://learn.microsoft.com/en-us/graph/api/resources/event?view=graph-rest-1.0)
- Recurrence and cancellations: `instances` "includes occurrences that are part of the recurrence pattern, and exceptions modified, but doesn't include occurrences canceled from the series". `cancelledOccurrences` is only returned on a GET of the series master with `$select`. — [Microsoft Learn: event resource](https://learn.microsoft.com/en-us/graph/api/resources/event?view=graph-rest-1.0)
- Delta:
  - `GET /me/calendarView/delta?startDateTime=…&endDateTime=…`, described as changes "in a calendarView … of the user's primary calendar".
  - Query params must be given only on the initial request; they're encoded into `@odata.nextLink`/`@odata.deltaLink`.
  - "You cannot use $select". Also unsupported: `$expand`, `$filter`, `$orderby`, `$search`.
  - `Prefer: odata.maxpagesize={x}` is supported.
  - Deleted items appear under `@removed` with reason `deleted`. These include in-range deletions _and_ "Events that are outside the date range and that have been added, deleted, or updated", so you must filter yourself.
  - Least privileged permission is `Calendars.Read`, with ReadBasic listed only as "higher privileged". The docs table is inconsistent, but this implies ReadBasic may not suffice.
  - [Microsoft Learn: event delta](https://learn.microsoft.com/en-us/graph/api/event-delta?view=graph-rest-1.0)
- Throttling, Outlook limits "per app ID and mailbox combination":
  - "10,000 API requests in a 10-minute period".
  - "Four concurrent requests".
  - 150 MB upload per 5 minutes.
  - They apply to event, calendar and calendarGroup. JSON batching sends at most 4 Outlook sub-requests in parallel.
  - [Microsoft Learn: Throttling limits](https://learn.microsoft.com/en-us/graph/throttling-limits)
- Throttled responses are `429 Too Many Requests`. Headers `x-ms-throttle-scope` and `x-ms-throttle-information` are provided for diagnostics. Some services (not Outlook) explicitly don't send `Retry-After`. — [Microsoft Learn: Throttling limits](https://learn.microsoft.com/en-us/graph/throttling-limits)

### Inferences

- Concrete request per cached UTC day `[d, d+1)`:
  ```
  GET https://graph.microsoft.com/v1.0/me/calendarView
      ?startDateTime=2026-11-16T00:00:00Z&endDateTime=2026-11-17T00:00:00Z
      &$select=id,iCalUId,subject,start,end,isAllDay,isCancelled,showAs,type,seriesMasterId,
               responseStatus,organizer,isOrganizer,location,onlineMeeting,isOnlineMeeting,webLink,sensitivity,categories
      &$top=100
  Prefer: outlook.timezone="UTC"
  Prefer: IdType="ImmutableId"   (both can be one header, comma-separated)
  ```
  Loop on `@odata.nextLink`, which carries the original query. Build the URL with RFC 3339 `Z` instants, which matches the project's "instants only" rule. No local time zone is needed in core.
- Response `start.dateTime` comes as a naive string like `2026-11-16T09:00:00.0000000` with `timeZone: "UTC"`. Parse it as naive and attach UTC. Don't parse it as RFC 3339, because it has no offset and has 7 fractional digits.
- calendarView returns events that _overlap_ the window. An event crossing a UTC midnight appears in two day-buckets, so dedupe by `id`/`iCalUId` when merging days.
- All-day events are anchored to midnight in their original time zone. With UTC output they'll show as non-midnight instants for users outside UTC, so treat `isAllDay` specially in the dashboard.
- For activity logging, filter out `isCancelled == true`, `showAs == free` (optional), and `responseStatus.response == "declined"`. Declined meetings may still appear on the calendar depending on client settings. This is unverified, see Gaps.
- `Calendars.ReadBasic` should suffice for subject/time/location/organizer/showAs. Add `offline_access` to get a refresh token. Typical scopes: `offline_access Calendars.ReadBasic` (plus `User.Read`/`openid profile` if account identity is needed).
- Delta doesn't fit the per-UTC-day cache: it needs `Calendars.Read`, a fixed window per round, no `$select`, and primary calendar only. Plain calendarView per missing/stale day is simpler and well under the 10k/10 min budget.
- Cap concurrency at ≤4 per mailbox (e.g., a semaphore). On 429, honor `Retry-After` if present, else exponential backoff.

### Gaps

- Default page size and max `$top` for calendarView weren't in the fetched page. Commonly cited as default 10, which I didn't verify, so set `$top` or `Prefer: odata.maxpagesize` explicitly.
- How declined meetings and organizer-cancelled meetings appear in an attendee's calendarView (e.g., "Canceled:" prefix plus `isCancelled=true` until removed) wasn't confirmed from Microsoft docs in this session.
- Whether `/me/calendars/{id}/calendarView/delta` (non-default calendars) is supported in v1.0 wasn't confirmed. The v1.0 delta page lists only `/me/calendarView/delta`.
- Whether `Calendars.ReadBasic` strips `attendees` isn't stated. The docs say "such as body, attachments, and extensions", which isn't exhaustive.
- Whether Outlook-resource 429s always include `Retry-After` wasn't confirmed on the general throttling page (not fetched).

## Existing Rust projects reading Graph calendar

### Takeaway

There are few public Rust examples. The closest are a 2026 Linux mail/calendar client using Microsoft Graph, a Rust Teams/Outlook agent platform, and `graph-rs-sdk`'s own examples (stale). The `oauth2` crate ships Microsoft device-flow and Graph examples useful for the auth half.

### Cited Findings

- `cumulas-inc/email-client` (pushed 2026-10-20): "A fast, focused mail, calendar, contacts and tasks app for Linux. It works with Microsoft 365 and Outlook.com accounts (through Microsoft Graph)…" — [GitHub cumulas-inc/email-client](https://github.com/cumulas-inc/email-client)
- `Finyasy/OfficeClaw` (pushed 2026-03-06): "Rust-first … agent platform for Microsoft Teams that automates Outlook mail and calendar via Microsoft Graph". — [GitHub Finyasy/OfficeClaw](https://github.com/Finyasy/OfficeClaw)
- `oauth2` crate docs reference a Microsoft device authorization flow example and a Microsoft Graph example. — [docs.rs oauth2 5.0.0](https://docs.rs/oauth2/5.0.0/oauth2/)
- `graph-rs-sdk` provides typed Graph calendar/event clients but is low-activity since 2025. — [GitHub sreeise/graph-rs-sdk](https://github.com/sreeise/graph-rs-sdk)

### Inferences

- Hand-written `serde` structs for the ~15 selected event fields plus `reqwest` are less weight than `graph-rs-sdk`. They also fit the project rule that shared models live in `core/src/model/`.

### Gaps

- The source of these repos wasn't inspected. Their auth approach (loopback vs device code) and calendarView handling are unverified.
