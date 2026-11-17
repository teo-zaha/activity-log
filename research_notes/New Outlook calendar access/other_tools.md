# How comparable tools read Outlook / Microsoft 365 calendars (new Outlook era), as of Nov 2026

Scope note: research done 2026-11-16. Many vendor help pages are undated; where a date is known it is given. Several primary pages (Mozilla SUMO KB, some GitLab MR pages) failed to render via fetch; claims from those rely on search-snippet text and are flagged.

## 1. Time trackers / activity loggers: mechanism, new-Outlook stance, consent, scopes

### Takeaway

Every cloud time tracker examined reads M365 calendars server-side through a vendor-owned multi-tenant OAuth app (Microsoft Graph implied, scopes rarely documented). None reads the local Outlook client, so none has a "new Outlook" problem and none mentions new Outlook. The friction they all document is tenant admin consent. The two desktop-first tools sidestep Graph: Timing uses macOS Calendar.app, and ManicTime uses ICS or an admin's own Entra app registration.

### Cited Findings

**ManicTime (Windows desktop + Server)**

- Outlook.com calendar timeline is added by pasting the calendar's ICS publish link into the timeline editor ("choose Outlook.com calendar") — older release note (v2.5 era) — [ManicTime release notes](https://manictime.com/releases/windows/Version-v2-5)
- Office 365 calendar works only through ManicTime Server (self-hosted). The _server admin_ registers their own app in Azure: they pick the supported account types, create a **client secret**, copy the client ID, and add a redirect URI on the **Web** platform. "The connection will stop working when the secret expires, and users will have to reconnect." Cloud customers must contact support — [ManicTime Server: Office 365 calendar config](https://docs.manictime.com/server/administration/configuration/office-365-calendar)
- User-facing doc warns that the sign-in screen will say the app "is not verified" and tells users to continue because their own admins created it. "Only the user making a connection can see the data." — [ManicTime: My connections / Office 365 calendar](https://docs.manictime.com/server/administration/my-connections/office-365-calendar)
- No API permissions are named in the docs, and new Outlook is not mentioned — same sources

**Timing (macOS)**

- "Timing reads events from the Calendar app". Accounts are added in System Settings > Internet Accounts, and the Exchange instructions "should also apply to calendars synced via Office 365, Microsoft Outlook, and similar services." The integration is read-only, and scripts are offered to write entries into Calendar.app or Microsoft Outlook — [Timing help: Calendar integration](https://timingapp.com/help/calendar)

**Toggl Track**

- Native integration: Profile settings > External Calendars > Connect Outlook > "Give Toggl permission to view your calendars". Events are private to the user. A separate browser extension injects a timer into Outlook on the web. No scopes and no Outlook version are named — [Toggl Outlook integration page](https://toggl.com/track/outlook-time-tracking/?tab=native)
- Oct 3, 2025 community thread: a non-admin M365 user hit a prompt asking to "allow 365 tenant access". Toggl Support replied that it is a **tenant-wide admin consent** a Global Admin grants once, after which users connect individually. Support described the permissions as "typically calendar-read scoped (such as Calendars.Read)", without committing to an exact list, and suggested checking Conditional Access. No resolution is posted — [Toggl Community](https://community.toggl.com/t/how-to-connect-regular-non-admin-365-users-to-toggl/3214)

**Clockify**

- Calendar integration supports "Google Calendar and cloud-based Office 365 Outlook" and "doesn't work with private or company-hosted Outlook servers". All-day events are excluded. Access is revoked through Microsoft account Privacy > App access, which implies a Microsoft OAuth app. No scopes or admin consent are documented — [Clockify help](https://clockify.me/help/getting-started/integrate-google-outlook-with-your-clockify-account)

**RescueTime**

- Account Settings > Calendar Integration > Outlook Calendar > Office 365 sign-in. Permissions are needed "to create the dedicated RescueTime calendar where your daily summaries are posted". It "will never add, edit, or delete anything from your existing calendars", and meetings show in the Assistant, so it reads events and writes a separate calendar. No scopes, admin consent or new Outlook are mentioned — [RescueTime help](https://help.rescuetime.com/article/322-office-365-outlook-calendar-integration)

**Timely (Memory)**

- Settings > Apps > Office 365. "Once a user tries to add Timely, you'll see the admin consent request in your O365 admin panel which you can accept on behalf of your organization". Admins can also find "Timely 365" in the admin panel > Permissions. "We are not able to sync data using a Microsoft Exchange server"; it "only connects to Microsoft's web-based calendar services". No scopes are listed — [Timely handbook](https://www.timely.com/help/handbook/integrations/office-365-calendar/)

**Harvest**

- "Pull in a calendar event" in the web timesheet. The official article covers "Google Calendar and Outlook.com Calendar", and its title does not say whether M365 work calendars are covered. Desktop apps (Mac/Windows) don't support the calendar feature. All-day and declined events are hidden — [Harvest help](https://support.getharvest.com/hc/en-us/articles/360048180932)

**Rize**

- Settings > Integrations > connect Outlook / Microsoft 365. It "only reads calendar event titles and times", supports shared and secondary calendars, and matches Teams calls to events. No scopes or consent docs — [Rize Outlook integration](https://rize.io/integrations/outlook); [Rize docs](https://docs.rize.io/calendars/manage-calendars)

**Everhour**

- Native calendar sync for Outlook works by an admin **pasting a calendar link** (ICS) on the Integrations page, not OAuth — [Everhour help](https://support.everhour.com/article/603-calendar-integrations)

**TimeCamp**

- Imports Outlook calendar events as timesheet entries through an account-settings integration. Mechanism, scopes and consent are not documented in what I found — [TimeCamp help](https://help.timecamp.com/help/integrations-outlook-calendar)

**WakaTime**

- No native Outlook calendar integration was found, only Pipedream automations — [Pipedream](https://pipedream.com/apps/wakatime/integrations/microsoft-outlook)

**Comparable data point (TimeChimp)**

- "In many organizations, Microsoft admin consent is required before employees can link the app to Outlook". It requests read-only calendar access — [TimeChimp help](https://intercom.help/timechimp/en/articles/7887831-connect-your-outlook-calendar)

### Inferences

- The proven pattern for SaaS trackers is a vendor-owned multi-tenant Entra app using delegated Graph calendar read, done server-side. That pattern does not depend on the Outlook client at all, so "new Outlook" is a non-issue for them. For a local-only desktop app, the equivalent is a public-client (no secret) multi-tenant registration with PKCE.
- ManicTime is the closest analogue to a local-first tool. It avoided shipping a vendor client ID and instead made the customer's admin register an app, at the cost of an "unverified app" warning and secret expiry. For a personal app, "bring your own client ID" is a reasonable fallback option, but it should not be the default.
- ICS publish links (ManicTime for Outlook.com, Everhour) are a zero-consent fallback. Many tenants disable external calendar publishing, though (not verified here).
- Toggl's "tenant access" prompt suggests some vendors request permissions that need admin consent, or that the tenant's policy blocks them. Either way, docs that tell users "ask your Global Admin to consent once" are standard.

### Gaps

- None of these vendors publish their exact Graph scope lists. Toggl's "Calendars.Read" is only a support agent's hedge.
- No time tracker documentation mentions "new Outlook" or olk.exe.
- No primary documentation was found for Tackle (TimeTackle) Outlook scopes, TimeCamp's mechanism, or Clockwise's Microsoft integration.
- Whether ManicTime's Windows client has a classic-Outlook COM plugin was not confirmed.

## 2. Calendar sync tools: mechanism, and especially OGCS on new Outlook

### Takeaway

Tools built on classic Outlook COM (OGCS, gSyncit, SimpleSYN, CodeTwo) all say new Outlook is unsupported and tell users to stay on classic Outlook. OGCS's long-term answer is a Graph/OAuth rewrite (v3 dev builds since June 2024) that hit corporate admin-consent refusals. Cloud sync services (CalendarBridge, Reclaim, OneCal) use delegated Graph and publish admin-consent runbooks with direct consent URLs.

### Cited Findings

**Outlook Google Calendar Sync (OGCS, open source, phw198)**

- Issue #1640, opened May 2023: the maintainer said new Outlook "doesn't support COM integrations" so OGCS won't work, and labelled it "wont implement". On Nov 16, 2023 he said the lack of COM add-in support was "now officially confirmed" and that OGCS would need significant redevelopment to use JS and/or Graph APIs. On Jun 6, 2024: Graph "won't have need or make use of the New Outlook client – it'll function in much the same way as interfacing with the Google calendar does". He also documented that as long as classic Outlook is installed, OGCS attaches to it — [OGCS #1640](https://github.com/phw198/OutlookGoogleCalendarSync/issues/1640)
- Issue #1881 "Sync without the Outlook client!" (Jun 16, 2024): dev build v3.0.0.1 connects "in much the same way as it does for Google (using OAuth)". It was initially one-way Google to Outlook with single instances only. It was converted to discussion #1883 — [OGCS #1881](https://github.com/phw198/OutlookGoogleCalendarSync/issues/1881)
- Discussion #1883 (Jun 2024 to Oct 2025): consent shows "Read user calendar" and "Write user and shared calendars". Users report they cannot get admin approval. One user's IT refused on Jul 4, 2024: "It is not desirable to synchronize company information from a work account to a private Google account." A user suggested scraping an OWA token as a workaround. The maintainer pointed to #1888 for release status. Hotfix 3.0.0.6 is mentioned, and no stable v3 release is confirmed on that page — [OGCS discussion #1883](https://github.com/phw198/OutlookGoogleCalendarSync/discussions/1883)

**Other COM-based sync tools**

- gSyncit: "older Common Object Model Outlook add-ins, including gSyncit, are not compatible" with new Outlook. The workaround is to "continue using Outlook in 'classic' mode", and the vendor is exploring a server-side version — [Fieldston gSyncit](https://fieldstonsoftware.com/support/gsyncit/topic_new_outlook_ui.aspx)
- SimpleSYN: "only supports classic Outlook for Windows and not the new Outlook app", because new Outlook doesn't allow offline data access for third-party apps. It advises toggling back to classic Outlook — [SimpleSYN note](https://simplesyn.net/en-US/note-on-the-new-outlook-app)
- CodeTwo Outlook Sync is deprecated and no longer developed — [CodeTwo FAQ](https://www.codetwo.com/outlook-sync/faq?sts=944). Sync2: no new-Outlook statement was found, and its changelog only shows Office 365 compatibility from 2019 — [Sync2 version history](https://www.sync2.com/version-history.aspx?p=all)
- Microsoft: VSTO and COM add-ins are not supported in new Outlook for Windows, and only web add-ins are — [Microsoft Learn: add-ins for new Outlook](https://learn.microsoft.com/en-us/office/dev/add-ins/outlook/one-outlook)

**CalendarBridge (cloud, Graph)**

- Delegated only: `User.Read`, `Calendars.ReadWrite` ("Have full access to calendars"), `offline_access`. No mail, files or directory scopes. CalendarBridge states that in **November 2025** Microsoft changed Entra defaults so "all third-party apps require administrator consent before a user can grant the app access to their calendars". It publishes a proactive admin link: `https://login.microsoftonline.com/common/oauth2/v2.0/authorize?prompt=login&client_id=a0dc78e0-066a-4aeb-b1e6-0f3efa277ed3&response_type=code&scope=offline_access+Calendars.ReadWrite+User.Read&...`. Admin consent alone grants no data access; each user must still connect. It suggests "Assignment required" to limit which users can connect. A separate "Managed Syncs" mode uses tenant-wide application permissions — [CalendarBridge: approve delegated permissions](https://help.calendarbridge.com/group-admin-docs/approve-delegated-permissions-for-calendarbridge-for-microsoft-365/); [Managed syncs](https://help.calendarbridge.com/group-admin-docs/authorizing-access-for-managed-syncs/)

**Reclaim.ai**

- Article dated Sep 16, 2026: "Admin Approval Needed" errors come from tenant restrictions. It publishes an admin consent URL `https://login.microsoftonline.com/common/oauth2/authorize?client_id=247a8c1c-e511-41dd-9517-611a4e68e782&response_type=code&prompt=admin_consent`, and admins can also approve under Enterprise Applications — [Reclaim help](https://help.reclaim.ai/en/articles/11352124-why-am-i-seeing-error-admin-approval-needed-when-trying-to-connect-my-outlook-calendar-to-reclaim). Its Entra guide requires the Global Administrator, Privileged Role Administrator or Cloud Application Administrator role and warns that consent "requires several attempts before Microsoft propagates" — [Reclaim Entra/SCIM guide](https://help.reclaim.ai/en/articles/13604712-enabling-service-principal-and-scim-at-microsoft-entra-id-to-use-reclaim-ai)

**OneCal**

- An explainer for "Need admin approval" (published Nov 2023, updated Jul 2025): admins can block third-party calendar apps. The fix is Entra > Enterprise apps > user consent settings and enabling "Users can request admin consent". No scopes, consent URL or publisher-verification mention — [OneCal blog](https://www.onecal.io/blog/need-approval-how-to-request-admin-approval-outlook-microsoft)

### Inferences

- Every tool that depended on reading the local Outlook client concluded that new Outlook cannot be integrated with locally. Their user-facing message is "use classic Outlook, or wait for the cloud/Graph version". An activity-log app should not try to integrate with olk.exe.
- OGCS shows that requesting _write_ scopes (`Calendars.ReadWrite`) makes IT refusals more likely. A read-only activity log should request only `Calendars.Read` (plus `offline_access` and `User.Read`) and say so prominently.
- Publishing a ready-made admin consent URL and a short IT-facing page (exact scopes, delegated only, data stays local) is the proven pattern (CalendarBridge, Reclaim).

### Gaps

- OGCS v3's auth details (MSAL vs loopback vs device code, its client ID, publisher verification) and whether v3 shipped as stable by Nov 2026 are unknown. Issue #1888 was not fetched.
- No primary source covers SyncGene, Sync2 or Clockwise Graph scopes and consent, or a new-Outlook statement from SyncGene or Sync2.

## 3. Open-source desktop clients doing delegated Graph/OAuth for M365

### Takeaway

The open-source desktop pattern is a public client with no secret, the `common` tenant, PKCE, and a loopback/localhost or custom-scheme redirect, usually under a project-owned multi-tenant client ID. Those IDs are typically **unverified**. That (plus broad scopes) is the main source of "Need admin approval" reports, and users or IT respond by registering per-tenant client IDs or borrowing Thunderbird's well-known ID.

### Cited Findings

**GNOME Online Accounts (GOA) "Microsoft 365" provider (GNOME 46+, 2024)**

- Source: auth endpoint `https://login.microsoftonline.com/%s/oauth2/v2.0/authorize`, tenant defaults to `common`, `get_use_pkce` returns TRUE, client secret "always empty". The redirect URI is `goa-oauth2://localhost/<client_id>`. The client ID comes from build-time macro `GOA_MS_GRAPH_CLIENT_ID` and "there is no build-system default" — [goamsgraphprovider.c](https://gitlab.gnome.org/GNOME/gnome-online-accounts/-/raw/master/src/goabackend/goamsgraphprovider.c)
- Requested scopes are very broad: offline_access, calendars.readwrite(.shared), contacts.readwrite(.shared), files.readwrite(.all), mail.readwrite(.shared), mail.send(.shared), mailboxsettings.read, people.read, sites.read.all, sites.readwrite.all, tasks.readwrite(.shared), user.read, user.readbasic.all — same source
- Upstream set up a microsoft@gnome.org account and a default client ID that distros can ship — [Fedora Workstation #422](https://pagure.io/fedora-workstation/issue/422); a "goabackend: update Microsoft client ID" MR exists (contents not retrievable) — [GOA MR !187](https://gitlab.gnome.org/GNOME/gnome-online-accounts/-/merge_requests/187)
- Users hit "Need admin approval" because of the full scope set. A Feb 2025 thread reports that granting the scopes temporarily, then selecting only Files, works. Issue #407 "Microsoft 365 Graph permissions too broad" was filed Feb 14, 2025 — [GNOME Discourse](https://discourse.gnome.org/t/microsoft-graph-permissions-required-by-a-microsoft-365-account/26697)
- Issue #335 "Default Microsoft 365 client is unverified": a user cannot log in and asks for a verification ETA (no resolution visible) — [GOA #335](https://gitlab.gnome.org/GNOME/gnome-online-accounts/-/issues/335); users cite GNOME being "not a verified app" as the reason admins refuse — [GNOME Discourse](https://discourse.gnome.org/t/microsoft-365-integration-permission-issues-and-app-verification/24539)
- Universities publish tenant-specific client and tenant IDs for GOA. Waterloo tells users to pick "return to the application without granting admin consent" if prompted — [UWaterloo KB](https://uwaterloo.atlassian.net/wiki/spaces/ISTKB/pages/44816171143/Gnome+Online+Accounts); [UiO guide](https://www.uio.no/english/services/it/email-calendar/gnome-online-accounts-ms365/help/)

**Evolution-EWS**

- Wiki: since 3.27.91, Office365 OAuth2 needs an application ID and a tenant. Evolution historically had no predefined app, and the docs list built-in IDs with user-consent vs admin-approval variants — [GNOME wiki EWS/OAuth2 (moved)](https://wiki.gnome.org/Apps/Evolution/EWS/OAuth2)
- Issue #184 asks GNOME to verify Evolution's app IDs (`20460e5d-ce91-49af-a3a5-70b6be7486d1`, `751cf8be-ca07-484b-9308-fac4b9d85eff`), because university IT refuses unverified apps. No maintainer response is shown — [evolution-ews #184](https://gitlab.gnome.org/GNOME/evolution-ews/-/issues/184)
- Mailing list (2020): "each Application ID needs to be approved for access by your Tenant admins" — [evolution-list 2020-10](https://lists.gnome.org/archives/evolution-list/2020-October/msg00036.html)

**DavMail (Java EWS/IMAP gateway)**

- Modes: O365Interactive (embedded browser window handling credentials, MFA and consent), O365Modern, O365Manual. The default client ID `facd6cff-a294-4415-b59f-c5b01937d7bd` is the "DavMail provided value", and it is overridable with `davmail.oauth.clientId` and `davmail.oauth.tenantId` (default `common`). Redirect `https://localhost/common/oauth2/nativeclient`. FAQ note: in **January 2026** Microsoft changed the nativeclient endpoint to redirect to `login.microsoftonline.com/common/wrongplace`, so `https://localhost/common/oauth2/nativeclient` must be used instead. "It is no longer possible to add EWS scope under API permissions" via the UI; use the manifest editor. The refresh token is persisted — [DavMail FAQ](https://davmail.sourceforge.net/faq.html); [O365InteractiveAuthenticator](https://davmail.sourceforge.net/xref/davmail/exchange/auth/O365InteractiveAuthenticator.html)
- Users report swapping in the Outlook client ID `d3590ed6-52b3-4102-aeff-aad2292ab01c` (Microsoft first-party) when the default ID stopped working — [DavMail bug 702](https://sourceforge.net/p/davmail/bugs/702); [support request 407](https://sourceforge.net/p/davmail/support-requests/407)
- DavMail 6.5 added an interactive login built on SWT and WebView2 that supports FIDO2 and Windows Hello — [Linuxiac](https://linuxiac.com/davmail-6-5-exchange-gateway-brings-interactive-office-365-login/)

**Thunderbird**

- Mozilla KB "Microsoft OAuth Authentication and Thunderbird in 2026" (evolving as of June 2026) says, per search snippets: Microsoft "changed the way they classify certain clients, so Thunderbird's previous OAuth setup does not properly qualify it as a desktop client". Org admins must grant consent once via an admin-consent URL with client ID `9e5f94bc-e8a4-4e73-b8be-63364c29d753`. From Thunderbird 153 the sign-in opens in the **system browser** instead of an embedded tab — [Mozilla SUMO KB](https://support.mozilla.org/en-US/kb/microsoft-oauth-authentication-and-thunderbird-202) (page did not render on fetch; snippet-based)
- A 2026 Bugzilla bug reports that Microsoft now redirects to `https://localhost` instead of `http`, breaking personal-account OAuth — [Bugzilla 2036505](https://bugzilla.mozilla.org/show_bug.cgi?id=2036505); another bug says the new client ID needs IT approval — [Bugzilla 1815715](https://bugzilla.mozilla.org/show_bug.cgi?id=1815715)

**mutt_oauth2.py / aerc / isync users**

- NeoMutt's README supports three flows: `authcode`, `localhostauthcode` (localhost redirect, same machine only) and `devicecode`. It recommends registering your own app ("Mobile and desktop" platform, redirect `http://localhost/`, enable public client flows for device code). Borrowing another client's ID is "a temporary punt for end-user experimentation", "not an approach for configuring systems to be used by other people". It warns the browser step "might indicate your institution admins must grant approval" — [NeoMutt contrib/oauth2 README](https://raw.githubusercontent.com/neomutt/neomutt/main/contrib/oauth2/README.md)
- In practice, users plug Thunderbird's client ID into mutt_oauth2.py because it is "a well-known application that is likely already authorized" in tenants — [isync mailing list](https://sourceforge.net/p/isync/mailman/message/37855750/); [yagshi blog](https://www.oit.ac.jp/rd/labs/kobayashi-lab/~yagshi/articles/mu4emuttoauth2/)

### Inferences

- The proven desktop pattern is: Entra registration as a public client (Mobile and desktop platform), multi-tenant plus personal accounts, the `common` (or `organizations`) authority, auth code + PKCE, the **system browser** with a loopback redirect (`http://localhost:<port>`), and the refresh token in the OS keychain. Thunderbird moving to system-browser sign-in in 2026, plus the nativeclient/https-localhost redirect changes, suggest that embedded webviews and nativeclient redirects are becoming fragile.
- GNOME and Evolution show that an **unverified** shared client ID combined with broad scopes is what makes IT refuse. Publisher verification (free, needs a Microsoft AI Cloud Partner Program/MPN ID) and minimal scopes are the levers a small project controls.
- Reusing Thunderbird's or Outlook's client ID is common among hobbyists but not appropriate for a distributed app (NeoMutt's own guidance). Thunderbird's ID is also registered for mail protocol scopes, so it would not carry Graph Calendars.Read anyway (inference, not verified).
- A "custom client ID / tenant ID" setting (GOA, DavMail, Evolution all expose this) is the standard escape hatch for tenants whose IT will register an internal app.

### Gaps

- No Rust/Tauri/Electron open-source calendar app with documented M365 Graph auth was found in this pass.
- GOA's actual default client ID value, its verification status in Nov 2026, and the outcome of #335 and #407 are unknown.
- The exact Thunderbird 2026 KB text could not be fetched, so the old→new client ID details are not confirmed.

## 4. Common user-facing problems and how vendors document workarounds

### Takeaway

Since Nov 2025 the default ("Microsoft-managed") Entra user-consent policy blocks user consent for calendar and mail Graph permissions, reportedly including `Calendars.Read`, for third-party apps. "Need admin approval" is now the _expected_ outcome for a new multi-tenant calendar app in default-configured tenants, not an edge case. Device code flow is separately being blocked by a Microsoft-managed Conditional Access policy. Vendors respond with admin-consent URLs, Entra click-paths, scope transparency and "Assignment required" guidance.

### Cited Findings

- Microsoft-managed default consent policy: enabled mid-Aug 2025 (blocking Files.Read.All, Files.ReadWrite.All, Sites.Read.All, Sites.ReadWrite.All), then extended (announced Oct 2025, rollout "completed by late November 2025") to Outlook and Teams content. The table lists **Calendars.Read and Calendars.ReadWrite (Nov 2025)** and Mail.Read. offline_access and User.Read are not listed. "Users who have already granted consent ... can continue"; "New users or apps requesting new or broader permissions are blocked." It applies only to tenants on the default policy, not custom ones. Caveat: the article's own counts (20, 24, 36) are inconsistent, and it cites no Message Center ID — [Topedia, 8 Nov 2025, updated May 2026](https://blog.topedia.com/2025/11/microsoft-managed-default-app-consent-policy-now-blocks-20-additional-permissions/); corroborated by [CalendarBridge](https://help.calendarbridge.com/group-admin-docs/approve-delegated-permissions-for-calendarbridge-for-microsoft-365/) ("all third-party apps require administrator consent before a user can grant the app access to their calendars")
- MC1304287 (May 8, 2026; rollout June to July 2026) adds Contacts.ReadWrite, Contacts.Read.Shared, Contacts.ReadWrite.Shared, People.Read, Tasks.Read(.Shared) and Tasks.ReadWrite(.Shared) to the Microsoft-managed policy. Users can't consent "unless the app is included in the" Mail client policy. Existing consents are unaffected — [MC1304287 mirror](https://mc.merill.net/message/MC1304287)
- The "Let Microsoft manage your consent settings" option (`microsoft-user-default-recommended`) is the default for new tenants and is updated automatically — [Microsoft Learn: manage app consent policies](https://learn.microsoft.com/en-us/entra/identity/enterprise-apps/manage-app-consent-policies) (via search snippet)
- Microsoft recommends allowing user consent only for **verified publishers**. The built-in `microsoft-user-default-low` policy allows consent "only for apps from verified publishers and apps that are registered in your tenant, and only for permissions that you classify as low impact". Apps with "assignment required" always need admin consent. The admin consent workflow lets users request approval — [Microsoft Learn: configure user consent](https://learn.microsoft.com/en-us/entra/identity/enterprise-apps/configure-user-consent)
- Device code flow: Microsoft rolled out a Microsoft-managed Conditional Access policy blocking device code flow for tenants that haven't used it recently (announced around Feb/Mar 2025). Microsoft says the flow is "rarely used by customers, but is frequently used by attackers" — [Microsoft Learn: managed policies](https://learn.microsoft.com/entra/identity/conditional-access/managed-policies); [Entra blog](https://techcommunity.microsoft.com/blog/microsoft-entra-blog/-/4286758). Microsoft recommends blocking device code wherever possible — [Learn: authentication flows](https://learn.microsoft.com/en-us/entra/identity/conditional-access/concept-authentication-flows). The CIS M365 benchmark v6 L1 requires it blocked — [Tenable CIS audit](https://www.tenable.com/audits/items/CIS_Microsoft_365_Foundations_v6.0.1_L1_E5.audit:9033824c949b5560f26fa3d7bc1af197). Teams Android devices broke as a result — [Petri](https://petri.com/microsoft-teams-devices-ca-policy-change/)
- Vendor workaround patterns:
  - Direct admin-consent URLs: [CalendarBridge](https://help.calendarbridge.com/group-admin-docs/approve-delegated-permissions-for-calendarbridge-for-microsoft-365/), [Reclaim](https://help.reclaim.ai/en/articles/11352124-why-am-i-seeing-error-admin-approval-needed-when-trying-to-connect-my-outlook-calendar-to-reclaim), [Thunderbird](https://support.mozilla.org/en-US/kb/microsoft-oauth-authentication-and-thunderbird-202)
  - Entra > Enterprise apps > Admin consent requests click-paths: [CalendarBridge](https://help.calendarbridge.com/group-admin-docs/approve-delegated-permissions-for-calendarbridge-for-microsoft-365/), [Timely](https://www.timely.com/help/handbook/integrations/office-365-calendar/)
  - Enabling "Users can request admin consent": [OneCal](https://www.onecal.io/blog/need-approval-how-to-request-admin-approval-outlook-microsoft)
  - Restricting the app with "Assignment required" after consent: [CalendarBridge restrict users](https://help.calendarbridge.com/group-admin-docs/restrict-calendarbridge-access-to-certain-microsoft-365-users/)
  - A user-facing "app not verified, continue" note for an admin-registered app: [ManicTime](https://docs.manictime.com/server/administration/my-connections/office-365-calendar)
- IT refusals are often policy-based and not technical: OGCS users were told syncing work data to a private account was "not desirable" — [OGCS #1883](https://github.com/phw198/OutlookGoogleCalendarSync/discussions/1883). Evolution users' IT refused because the app IDs were unverified — [evolution-ews #184](https://gitlab.gnome.org/GNOME/evolution-ews/-/issues/184)

### Inferences

- For a personal activity-log app in Nov 2026, a work user in a default-configured tenant will almost certainly see "Need admin approval" for `Calendars.Read`, even with publisher verification, if Topedia's table is right. So the app needs an IT-facing consent page and a pre-built admin consent URL from day one. Personal MSA (outlook.com) accounts are unaffected by tenant policy.
- Prefer auth code + PKCE with a loopback redirect over device code. Device code is increasingly blocked by the managed CA policy and CIS baselines.
- Publisher verification matters mainly for tenants that use `microsoft-user-default-low` (verified publishers only) and for IT trust (the GNOME and Evolution cases). It does not by itself bypass the Microsoft-managed block on calendar scopes (inference; unconfirmed).
- Minimal scopes (`Calendars.Read`, `offline_access`, `User.Read` or even just `openid`/`profile`) give the strongest case for approval. GOA's broad scopes are the counterexample.

### Gaps

- No Microsoft-authored primary source was found that explicitly lists `Calendars.Read` in the Microsoft-managed policy. The claim rests on Topedia's table and CalendarBridge's statement. It should be verified by inspecting `microsoft-user-default-recommended` via `GET /policies/permissionGrantPolicies` in a test tenant.
- Not determined: whether verified publishers or apps registered in the user's own tenant are exempt from the Microsoft-managed calendar block.
- r/sysadmin and r/Office365 threads on admin consent were not reviewed in this pass.
