# Non-Graph ways for a third-party Windows desktop app to read calendar events when the user runs new Outlook (olk.exe), as of 16 November 2026

Scope note: these notes cover every option except Microsoft Graph. The date of each source is given where it matters. Anything without a source is in the Inferences or Gaps sections.

Summary table (synthesized from the sections below):

| Option                                                 | Feasible (Nov 2026)?                                                    | Fidelity                                                                  | Reliability                                                                      | Policy risk                                                              |
| ------------------------------------------------------ | ----------------------------------------------------------------------- | ------------------------------------------------------------------------- | -------------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| New Outlook COM, MAPI or VBA object model              | No. None exists                                                         | n/a                                                                       | n/a                                                                              | n/a                                                                      |
| Read the olk.exe WebView2 IndexedDB offline store      | Technically yes, as reverse engineering                                 | Full: events plus calendars, if cached                                    | Low. Undocumented, changes without notice, and unstable while olk.exe is running | High. Unsupported, and likely against enterprise security expectations   |
| WinRT AppointmentStore / Windows Calendar              | No, in practice. Mail & Calendar is retired                             | n/a                                                                       | n/a                                                                              | Low                                                                      |
| Published ICS URL (OWA "Publish a calendar")           | Yes, if the tenant allows anonymous publishing                          | Free/busy only, titles+locations, or all details (user picks; admin caps) | Medium. Refresh latency is undocumented                                          | Medium. Anyone who has the URL can read it, and admins often restrict it |
| Office.js add-in bridging to localhost                 | Weak. No API to enumerate the calendar; would need Graph via NAA anyway | Current item only, without Graph                                          | Low. Only works while the add-in runs                                            | Medium. Needs deployment or admin consent                                |
| EWS (Exchange Online)                                  | Dying. Being disabled since Oct 2026, fully off April 2027              | Full                                                                      | Very low                                                                         | Needs a tenant admin allow-list until April 2027                         |
| EWS (Exchange Server on-premises)                      | Yes                                                                     | Full                                                                      | High                                                                             | Low. But new Outlook barely supports on-prem mailboxes anyway            |
| Tell user to switch back to classic Outlook (COM/MAPI) | Yes, until at least 2029                                                | Full                                                                      | High                                                                             | Low until the enterprise cutover phase                                   |

---

## 1. New Outlook automation: object model, local API, architecture, offline cache

### Takeaway

New Outlook (olk.exe) has no COM, MAPI or VBA object model, and Microsoft has announced no local API, CLI or automation interface for it; its only extensibility is Office.js web add-ins. It runs Outlook on the web inside WebView2 and keeps an undocumented Chromium IndexedDB (LevelDB) offline store under `%LOCALAPPDATA%\Microsoft\Olk\EBWebView\Default\IndexedDB`. That store does contain calendar events. One open-source project tried to read it in October 2026 and could not make the calendar part reliable.

### Cited Findings

- COM and VSTO add-ins are not supported in the new Outlook on Windows "for reliability reasons". They remain supported in classic Outlook, and the migration path is Outlook web add-ins. — [Microsoft Learn: Develop Outlook add-ins for the new Outlook on Windows](https://learn.microsoft.com/en-us/office/dev/add-ins/outlook/one-outlook); [Microsoft Learn: Identify COM add-ins](https://learn.microsoft.com/en-us/microsoft-365-apps/outlook/get-started/state-of-com-add-ins)
- New Outlook does not support VBA. The VBA for Outlook add-in works only in classic Outlook. — [Microsoft Learn: Identify COM add-ins](https://learn.microsoft.com/en-us/microsoft-365-apps/outlook/get-started/state-of-com-add-ins)
- A Microsoft Q&A answer states that new Outlook for Windows "does not support COM, VBA, or MAPI automation". This is a community answer, not official documentation. — [Microsoft Q&A: need a support in new outlook automation](https://learn.microsoft.com/en-in/answers/questions/5951650/need-a-support-in-new-outlook-automation)
- Microsoft said in 2023 that it "will definitely not support COM add-ins" in new Outlook (older report). — [BornCity, Oct 2023](https://borncity.com/win/2023/10/20/new-outlook-microsoft-will-definitely-not-support-com-add-ins/)
- Architecture:
  - New Outlook requires the WebView2 runtime and .NET Framework 4.7.2 or later.
  - It ships as an MSIX package `Microsoft.OutlookForWindows_..._8wekyb3d8bbwe` in WindowsApps, and is part of Windows starting with Windows 11 24H2.
  - User settings live in `%LOCALAPPDATA%\Packages\Microsoft.OutlookForWindows_8wekyb3d8bbwe\` and `%LOCALAPPDATA%\Microsoft\olk\`.
  - Source: [Microsoft Learn: Deploy the New Outlook for Windows on VDI (updated 2026-05-29)](https://learn.microsoft.com/en-us/microsoft-365-apps/outlook/get-started/virtualized-desktop-infrastructure)
- Microsoft documents these olk subfolders:
  - `EBWebView/default/IndexedDB`, `EBWebView\Default\Local Storage` and `olk/Attachments`: "Stores user and app data"
  - `EBWebView/default/Cache`: network call contents
  - `Service Worker/CacheStorage` and `Code Cache`: cached JS
  - `WebStorage`
  - `olk/logs` and `olk/cache`: diagnostics
  - Source: [Microsoft Learn VDI page](https://learn.microsoft.com/en-us/microsoft-365-apps/outlook/get-started/virtualized-desktop-infrastructure)
- New Outlook gained offline access in 2024. Office365ITPros guessed the store was in `EBWebView\Default\Cache\Cache_data` and noted that Microsoft has not documented it (older, 2024). — [office365itpros.com, Oct 2024](https://office365itpros.com/2024/10/10/offline-access-new-outlook/)
- A Japanese Microsoft support blog says the IndexedDB folder also holds settings, so turning offline access off does not delete it. — [jpmessaging blog](https://jpmessaging.github.io/blog/Offline-Cache-in-New-Outlook/)
- New Outlook can create, edit and delete single calendar events offline. Recurring events are not supported offline. — [MC1101906 via mc.merill.net](https://mc.merill.net/message/MC1101906)
- The ourostack/m365crawl project's attempt to read the store (issue #145, opened 9 Oct 2026, closed 23 Oct 2026):
  - The store is `Olk\EBWebView\Default\IndexedDB\https_outlook.office.com_0.indexeddb.leveldb` plus a `.blob` directory. It is a Chromium IndexedDB (LevelDB) database that the project's existing Chromium IndexedDB reader (built for Teams) could decode.
  - Object stores hold folders, messages, bodies, events, calendars and calendar groups. One sample had 926 events, 11 calendars and 4 calendar groups.
  - Real-host runs were flagged `source_stability/unstable_source_observation`, with intermittent changes while the client was running.
  - Calendar import failed twice: once refused at a repeat-compare stage, and once with 69,996 "outlook_event_unmapped" omission units. Calendar support was "not qualified" and deferred.
  - The issue does not mention DPAPI or app-bound encryption.
  - Source: [GitHub ourostack/m365crawl #145](https://github.com/ourostack/m365crawl/issues/145)
- Authentication tokens are stored separately under `%LOCALAPPDATA%\Microsoft\OneAuth`. Deleting that folder forces sign-in again (secondary forum source). — [WindowsForum thread](https://windowsforum.com/threads/fix-new-outlook-startup-crash-by-renaming-olk-and-oneauth-caches.393823/)
- Another open-source project, OLK2IMAP / OLK2Any, also reads the olk folders, including `Olk\Attachments`. — [GitHub MasterTH-PyPy/OLK2IMAP](https://github.com/MasterTH-PyPy/OLK2IMAP)

### Inferences

- Reading the IndexedDB store is technically possible because it is standard Chromium LevelDB, and it gives full fidelity for whatever happens to be cached. It is still a bad idea as a product dependency:
  - The schema is OWA-internal and undocumented, so it can change with any weekly OWA build.
  - LevelDB files are locked or being compacted while olk.exe runs, which matches the "unstable source" result.
  - Coverage depends on the offline sync window and settings, and admins can disable offline.
  - Recurring events are probably stored as masters or exceptions, which would explain the "unmapped" failures.
  - Endpoint security or DLP tools may flag a process that reads another app's browser profile.
- Because there is no local API, any supported integration with new Outlook has to go through the server (Graph, EWS or ICS) or through an Office.js add-in.

### Gaps

- I found no Microsoft announcement or roadmap item for a local API, COM replacement, URI-scheme automation or CLI for new Outlook as of November 2026.
- I could not confirm whether the IndexedDB values are encrypted at rest beyond normal NTFS/user-profile protection. The m365crawl decode suggests plaintext structured values, but it does not state this.
- No source documents how many days or months of calendar data the offline store keeps.

---

## 2. Windows Calendar store (WinRT AppointmentStore) and the retired Mail & Calendar apps

### Takeaway

The Windows Mail, Calendar and People apps reached end of support on 31 December 2024 and no longer sync. I found no evidence that new Outlook writes to the WinRT AppointmentStore. AppointmentStore is not a viable route to Exchange/M365 calendars in 2026.

### Cited Findings

- Support for Windows Mail, Calendar and People ended on 31 December 2024. Users can no longer send or receive email or events in those apps, and Microsoft directs users to new Outlook. — [Microsoft Support: Outlook for Windows, the future of Mail, Calendar and People](https://support.microsoft.com/en-us/help/17198)
- The apps were removed from the Store. Sideloaded copies fail at account setup. — [Windows Central](https://www.windowscentral.com/software-apps/microsoft-renders-mail-and-calendar-apps-inoperable-as-it-forces-users-over-to-the-new-outlook-on-windows)
- Windows Mail stopped syncing with Outlook/Hotmail accounts by April 2025. — [Windows Latest, Apr 2025](https://windowslatest.com/2025/04/29/windows-11s-mail-calendar-stops-syncing-with-outlook-hotmail-accounts)
- New Outlook syncs shared calendars over REST (a server-side change in Aug–Sep 2026). This is a cloud mechanism, with no sign of OS-level calendar integration. — [Windows Latest, Jul 2026](https://www.windowslatest.com/2026/07/29/microsoft-says-the-new-outlook-switch-was-painful-for-classic-holdouts-due-to-a-calendar-bug-now-getting-fixed/)

### Inferences

- AppointmentStore was filled by the account sync of the Mail & Calendar app (the Windows "Email & accounts" data provider). With that app retired and new Outlook storing data in its own WebView2 profile, AppointmentStore will at most contain stale or local data. Don't plan on it.

### Gaps

- I found no source that says explicitly whether new Outlook writes to `Windows.ApplicationModel.Appointments.AppointmentStore`, or whether Windows 11 2026 builds still sync Exchange calendars into it through a system account.

---

## 3. Published calendars (ICS URL)

### Takeaway

Users can publish a calendar from Outlook on the web or new Outlook (Settings > Calendar > Shared calendars > Publish a calendar). This produces a secret HTML URL and a secret ICS URL. The user picks the detail level: busy only, titles and locations, or all details. The tenant sharing policy's Anonymous entry caps that level and can disable publishing entirely. It is the only zero-auth, non-Graph, non-EWS server option. The data is read-only and the URL is a bearer secret.

### Cited Findings

- Publishing is at Settings > View all Outlook settings > Calendar > Shared calendars > Publish a calendar. It creates an HTML link and an ICS link. — [Microsoft Support: Share your calendar in Outlook on the web](https://support.microsoft.com/en-us/outlook/share-your-calendar-in-outlook-on-the-web); [UW–Madison KB](https://kb.wisc.edu/helpdesk//130454)
- Detail levels:
  - "Can view when I'm busy"
  - "Can view titles and locations": busy times plus title and location
  - "Can view all details": all details of events
  - Source: [Microsoft Support](https://support.microsoft.com/en-us/outlook/share-your-calendar-in-outlook-on-the-web)
- Third-party guides say "all details" also exposes invitees, meeting body text and links. — [ablebits](https://www.ablebits.com/office-addins-blog/make-outlook-calendar-public/); [FAU helpdesk](https://helpdesk.fau.edu/TDClient/2061/Portal/KB/Article/162831/Outlook-Calendar-Share-Open-Publish-and-Subscribe)
- Users often find only "Can view when I'm busy" offered, because of tenant policy. — [Microsoft Q&A](https://learn.microsoft.com/en-us/answers/questions/4695885/how-do-i-publish-my-calendar-to-show-more-than-can)
- Admin control is through sharing policies (`New-SharingPolicy` / `Set-SharingPolicy`) using the `Anonymous:` domain with these levels:
  - `CalendarSharingFreeBusySimple`: busy only
  - `CalendarSharingFreeBusyDetail`: free/busy plus subject and location
  - `CalendarSharingFreeBusyReviewer`: adds the body
  - Policies are assigned per mailbox with `Set-Mailbox -SharingPolicy`.
  - Source: [Microsoft Learn: Set-SharingPolicy](https://learn.microsoft.com/powershell/module/exchange/Set-SharingPolicy); [New-SharingPolicy](https://learn.microsoft.com/powershell/module/exchange/New-SharingPolicy)
- On Exchange Server on-premises, Internet Calendar Publishing also needs `Set-OwaVirtualDirectory -CalendarPublishingEnabled $true` plus a sharing policy (older Exchange 2010/2013 docs). — [Enable Internet calendar publishing (Exchange 2013)](https://technet.microsoft.com/en-us/library/jj853046)
- Sources disagree on the Exchange Online default:
  - Sikich says the default policy contains an `Anonymous: CalendarSharingFreeBusyReviewer` entry. — [Sikich](https://www.sikich.com/?p=23276)
  - c7solutions says anonymous publishing is not allowed by default. — [c7solutions](https://c7solutions.com/?p=5159)
  - Check per tenant with `Get-SharingPolicy`.
- I did not find `CalendarPublishingEnabled` in the visible part of the current `Set-OwaMailboxPolicy` docs, which were truncated. That cmdlet does have calendar-related Exchange Online switches (InterestingCalendarsEnabled, LocalEventsEnabled, PersonalAccountCalendarsEnabled). — [Microsoft Learn: Set-OwaMailboxPolicy](https://learn.microsoft.com/en-us/powershell/module/exchangepowershell/set-owamailboxpolicy)

### Inferences

- Implementation is simple: the user pastes an ICS URL, the app does an HTTPS GET and parses it (RRULE expansion and time zones are required). The app needs no auth and no app registration, which also fits the repo's instants-only backend.
- Security: the URL is a long random bearer secret. Anyone who gets it can read the calendar until the user unpublishes or resets the links. The app must store it like a credential.
- Fidelity depends on the policy. Many enterprises restrict anonymous publishing to free/busy only, or disable it, because it leaks data outside the tenant. A busy-only ICS gives time blocks with no titles, which is of limited value for an activity log.

### Gaps

- No primary source for refresh latency or caching of published ICS feeds from Exchange Online. Anecdotes suggest minutes to hours, but I found nothing citable.
- No survey data on how often enterprises disable anonymous calendar publishing.
- I could not confirm the current Exchange Online default sharing policy's Anonymous entry from a primary 2026 Microsoft doc.

---

## 4. Outlook add-in (Office.js) as a bridge to a local app

### Takeaway

New Outlook supports Office.js web add-ins, but the Mailbox API only exposes the item in context and has no calendar enumeration. Its non-Graph back doors are dead: legacy Exchange Online tokens are off for all tenants, so `makeEwsRequestAsync` and EWS/REST callback tokens no longer work, and EWS itself is being retired. Delegated data access in an add-in now means nested app authentication (NAA) to Graph, which is Graph anyway. As a non-Graph bridge, this route is not practical.

### Cited Findings

- New Outlook supports web add-ins as the replacement for COM. — [Microsoft Learn: One Outlook add-ins](https://learn.microsoft.com/en-us/office/dev/add-ins/outlook/one-outlook)
- "Legacy Exchange Online user identity tokens and callback tokens are no longer supported and turned off across all Microsoft 365 tenants." Calls such as `getUserIdentityTokenAsync` fail with errors 9017/9018. Microsoft recommends MSAL with NAA for delegated access. Exchange tokens still work for Exchange on-premises. — [Microsoft Learn: NAA and Outlook legacy tokens FAQ (updated 2026-03-18)](https://learn.microsoft.com/en-us/office/dev/add-ins/outlook/faq-nested-app-auth-outlook-legacy-tokens)
- Per the NAA FAQ as summarized by search, Exchange Online tokens were turned off for all tenants by October 2025, including tenants that had re-enabled them. — [Microsoft Learn: Use EWS from an Outlook add-in](https://learn.microsoft.com/en-us/office/dev/add-ins/outlook/web-services)
- NAA does not support the deprecated "approved client app" Conditional Access grant, which retired in March 2026. Conditional Access device policies can break NAA sign-in. — [NAA FAQ](https://learn.microsoft.com/en-us/office/dev/add-ins/outlook/faq-nested-app-auth-outlook-legacy-tokens)
- Add-ins in Microsoft Marketplace must be certified, and NAA add-ins that need Graph scopes need admin-consent handling. — [NAA FAQ](https://learn.microsoft.com/en-us/office/dev/add-ins/outlook/faq-nested-app-auth-outlook-legacy-tokens)

### Inferences

- An add-in runs only when the user opens its task pane, or for specific event-based activations such as compose or send events. It cannot run in the background to enumerate a date range.
- The Office.js Mailbox API reads the selected appointment, not "all events between start and end". Bulk reads need Graph (via NAA) or EWS, and both are out of scope or dead in Exchange Online.
- Pushing data from the add-in to `https://localhost` needs a trusted TLS cert on localhost, because the add-in is HTTPS and mixed content is blocked. WebView2 local/private network access rules may also apply. This is fragile.
- Deployment needs either sideloading (a dev workflow) or centralized deployment by an admin.
- Net: an add-in only makes sense if you already accept Graph. It would then get Graph tokens through NAA without its own sign-in UX, but the desktop app could just call Graph directly.

### Gaps

- I found no source that explicitly lists `makeEwsRequestAsync` support status in new Outlook for Windows. It is moot for Exchange Online because tokens are off.
- I did not verify current Office.js requirement sets for any calendar-range API. I am not aware of one.

---

## 5. EWS in Exchange Online: retirement status in November 2026

### Takeaway

EWS is being disabled tenant by tenant in Exchange Online, starting in October 2026. As of the 21 Oct 2026 Exchange blog post, it was in "the first stage of EWS disablement", where admins can still re-enable it with `Set-OrganizationConfig -EWSEnabled $true` plus the `EWSAllowedAppIDs` allow-list. Re-enabling stops working in April 2027, and EWS is then fully disabled. Microsoft's own apps are not exempt. A new third-party app has no future on EWS against Exchange Online.

### Cited Findings

- Microsoft's timeline: "October 2026: EWS starts to be disabled globally for all organizations"; "April 2027: EWS is fully disabled." After Midnight Blizzard (January 2024), the scope widened from third-party apps to all Microsoft apps as well. — [Microsoft Learn: Deprecation of EWS in Exchange Online (updated 2026-10-23)](https://learn.microsoft.com/en-us/exchange/clients-and-mobile-in-exchange-online/deprecation-of-ews-exchange-online)
- The 2023 announcement originally said blocking applies to non-Microsoft apps, and that Exchange Server on-premises is unchanged. — [Microsoft 365 Developer Blog (2023)](https://devblogs.microsoft.com/microsoft365dev/retirement-of-exchange-web-services-in-exchange-online/); [Exchange Team Blog (2023)](https://techcommunity.microsoft.com/blog/exchange/retirement-of-exchange-web-services-in-exchange-online/3924440)
- 21 Oct 2026 Exchange Team Blog post "Exchange Online EWS application stopped working? Do this":
  - "Retirement of this protocol is now in progress"; "We are now in the first stage of EWS disablement, where customers are still able to re-enable EWS"; "Re-enabling EWS will stop working in April 2027."
  - Admins re-enable through Baseline Security Mode or `Set-OrganizationConfig -EWSEnabled $true`, which takes under an hour and up to 4 hours.
  - Microsoft pre-populated `EwsAllowedAppIDs` with apps seen in the last 60 days.
  - Allow-list changes take 24 hours to apply.
  - "EWS will not be an option that your client applications can use after April 2027."
  - Source: [Exchange Team Blog, 21 Oct 2026](https://techcommunity.microsoft.com/blog/exchange/exchange-online-ews-application-stopped-working-do-this-/4566215)
- MC1447678 (as summarized by a third party, 13 Aug 2026):
  - `EWSAllowedAppIDs` reached general availability in late July 2026.
  - From October 2026, `EWSEnabled=True` with no list blocks everything except cross-tenant org relationships, and `True` plus a list allows only the listed apps.
  - Each update replaces the whole list.
  - View it with `Get-OrganizationConfig -RetrieveEwsOperationAccessPolicy`.
  - Source: [mwpro.co.uk summary of MC1447678](https://mwpro.co.uk/blog/2026/08/13/mc1447678-exchange-online-prepares-for-ews-retirement-with-ewsallowedappids-allow-list-for-approved-apps/)
- LazyAdmin's 2 Oct 2026 update, citing MC1485116 (third-party):
  - Microsoft recorded `$true`/no-list tenants on 2 Oct and populated their lists on 8–9 Oct.
  - Allow-list enforcement started 10 Oct 2026.
  - `$null` tenants are switched off later after a 7-day Message Center warning.
  - Source: [LazyAdmin](https://lazyadmin.nl/office-365/ews-is-being-blocked-in-exchange-online-how-to-find-every-app-still-using-it/)
  - This conflicts with "October 1" in other sources. Microsoft Learn says only "October 2026".
- Known casualties: Apple Mail/Calendar on macOS, classic Outlook for Mac, outdated Outlook desktop builds and backup vendors such as Veeam. — [Exchange Team Blog, Oct 2026](https://techcommunity.microsoft.com/blog/exchange/exchange-online-ews-application-stopped-working-do-this-/4566215); [Mailbox Insights](https://mailboxinsights.com/ews-retirement-exchange-online/)

### Inferences

- In November 2026, an EWS calendar reader for Exchange Online would only work in tenants whose admin explicitly re-enabled EWS and added the app's Entra app ID to `EWSAllowedAppIDs`, and only until April 2027. EWS also needs OAuth through Entra (Basic auth is long gone), so an app registration and consent are needed exactly as for Graph. Don't build it.

### Gaps

- I could not access the MC1485116 primary text, and MC676299 / MC1227454 were not retrieved. Exact per-tenant flip dates for `$null` tenants are not public.

---

## 6. Exchange Server on-premises and hybrid

### Takeaway

As of mid/late 2026, new Outlook still does not properly support on-premises Exchange mailboxes: reports describe at most an IMAP-based path from Q1 2026 without calendar integration. On-prem users therefore mostly can't use new Outlook for Exchange calendars at all. For on-prem mailboxes, EWS against the on-prem server remains supported and is the right non-Graph API, and Exchange tokens for add-ins still work there.

### Cited Findings

- New Outlook would support Exchange Server starting Q1 2026 "but only through IMAP". It treats those systems like any Internet mail service, with no extra calendar integration. — [Directions on Microsoft, Mar 2026](https://www.directionsonmicrosoft.com/microsoft-delays-by-a-year-its-new-outlook-rollout/)
- "No support for Exchange on-premises (at least not yet in mid-2026)." — [CodeTwo blog](https://www.codetwo.com/admins-blog/new-outlook-for-windows/)
- Microsoft Q&A (community answer): new Outlook does not support on-premises Exchange accounts, including Exchange SE, and support "will be added later". — [Microsoft Q&A](https://learn.microsoft.com/en-us/answers/a/9545960)
- EWS retirement affects only Exchange Online. Exchange Server is unchanged. — [Microsoft 365 Developer Blog](https://devblogs.microsoft.com/microsoft365dev/retirement-of-exchange-web-services-in-exchange-online/)
- "For Exchange on-premises, Exchange tokens still work and are supported." `Office.UserProfile.accountType == "enterprise"` indicates an on-prem mailbox. — [NAA FAQ](https://learn.microsoft.com/en-us/office/dev/add-ins/outlook/faq-nested-app-auth-outlook-legacy-tokens)

### Inferences

- A user with an on-prem mailbox almost certainly still uses classic Outlook, where COM/MAPI works. EWS on-prem (NTLM/Kerberos or OAuth via hybrid modern auth) is a solid fallback. Graph does not reach purely on-prem mailboxes.

### Gaps

- I did not find an official Microsoft Learn "Accounts in new Outlook" statement dated 2026 with a firm on-prem support date. Hybrid REST/Graph access to on-prem mailboxes was not researched.

---

## 7. Classic Outlook availability and timeline (fallback: tell users to switch back)

### Takeaway

Classic Outlook is supported "until at least 2029". Microsoft delayed the enterprise "opt-out" phase, where new Outlook becomes the default but users can toggle back, from April 2026 to about March/April 2027. A later "cutover" phase will remove the toggle, with no date announced. As of November 2026, telling users to switch back to classic Outlook so the app can use COM/MAPI is fully viable, at least for enterprise Microsoft 365 Apps users.

### Cited Findings

- "Existing installations of classic Outlook through perpetual and subscription licensing will continue to be supported until at least 2029" (2024 announcement, older). — [office365itpros, Mar 2024](https://office365itpros.com/2024/03/11/new-outlook-for-windows-2029/)
- In March 2026, Microsoft delayed the enterprise opt-out phase by 12 months to 2027. During opt-out, users and admins can still switch back and policies that block new Outlook still apply. In the later cutover phase, users cannot switch back. — [The Register, 6 Mar 2026](https://www.theregister.com/2026/03/06/microsoft_outlook_migration_postponed/); [Petri](https://petri.com/microsoft-delays-new-outlook-default-rollout-2027/); [Directions on Microsoft](https://www.directionsonmicrosoft.com/microsoft-delays-by-a-year-its-new-outlook-rollout/)
- Sources disagree on the opt-out month: Petri/TechRadar say March 2027, an older Directions item says April 2027. — [TechRadar](https://www.techradar.com/pro/microsoft-delays-enterprise-outlook-switchover-to-2027-says-move-will-ensure-organizations-have-the-time-they-need-to-prepare)
- No end-of-support date for classic Outlook has been given. Windows Latest says April 2029 (unconfirmed). — [TechRadar](https://www.techradar.com/pro/microsoft-delays-enterprise-outlook-switchover-to-2027-says-move-will-ensure-organizations-have-the-time-they-need-to-prepare); [Windows Latest, Jul 2026](https://windowslatest.com/2026/07/26/microsoft-still-wants-to-force-new-outlook-on-everyone-by-2027-even-as-users-call-it-painful-and-prefer-outlook-classic)
- Some customers will need to download and deploy classic Outlook separately from the suite to keep it. — [office365itpros](https://office365itpros.com/2024/03/11/new-outlook-for-windows-2029/)
- Office 2024 LTSC received code that removes classic Outlook's EWS dependency; 2019/2021 LTSC did not. This comes from Microsoft staff comments, not the post body. — [Exchange Team Blog comments, Oct 2026](https://techcommunity.microsoft.com/blog/exchange/exchange-online-ews-application-stopped-working-do-this-/4566215)

### Inferences

- Recommended product posture:
  1. Keep the classic Outlook COM/MAPI collector.
  2. Detect new Outlook (olk.exe or MSIX present, classic not running or not installed) and show guidance.
  3. Offer Graph as the main supported path.
  4. Offer a user-supplied published ICS URL as a no-admin, low-fidelity fallback.
  5. Skip IndexedDB scraping, AppointmentStore, Office.js bridges and EWS for Exchange Online.

### Gaps

- No announced date for the cutover phase, or for removal of the consumer (Home/Business) toggle.

---

macOS (brief): "Classic Outlook on macOS" (legacy Outlook for Mac) is listed among the clients broken by the Exchange Online EWS disablement. — [Exchange Team Blog, Oct 2026](https://techcommunity.microsoft.com/blog/exchange/exchange-online-ews-application-stopped-working-do-this-/4566215). Gap: I did not research how much AppleScript the new Outlook for Mac supports.
