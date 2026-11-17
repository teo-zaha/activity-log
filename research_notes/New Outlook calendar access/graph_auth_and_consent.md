# Microsoft Graph delegated calendar access from a native desktop public client (Rust/Tauri, Windows + macOS): auth and consent, as of November 2026

Research date: 2026-11-16. All Microsoft Learn pages cited were fetched on that date; their `updated_at` metadata is noted where relevant. Secondary blogs are labelled as such.

**Headline finding (affects every other section):** Microsoft's managed default user consent policy ("Let Microsoft manage your consent settings", `microsoft-user-default-recommended`), which is the default for new tenants and was rolled onto tenants that had kept the old "allow user consent" setting, has **excluded `Calendars.Read`, `Calendars.ReadBasic`, `Calendars.ReadWrite`, `Calendars.Read.Shared` and `Calendars.ReadWrite.Shared` from user consent since the November 2025 wave**. In a tenant on that default, a colleague **cannot self-consent** to a third-party or in-house app that reads their calendar through Graph. An admin has to grant consent, either directly or through the admin consent workflow. See section 3.

---

## 1. Which OAuth flows the Microsoft identity platform supports for public desktop clients, and current recommendations and restrictions

### Takeaway

For a desktop public client, the options are: (a) authorization code + PKCE with a loopback (`http://localhost` / `127.0.0.1`) redirect in the system browser; (b) the WAM broker on Windows, which Microsoft prefers because it gives SSO, device-bound refresh tokens and Conditional Access device signals; (c) the Enterprise SSO plug-in / Company Portal broker on macOS, which needs MDM; (d) device code flow. Microsoft positions device code flow for input-constrained devices only and tells tenant admins to "get as close as possible to a unilateral block". It is a poor primary flow for a desktop app in 2026.

### Cited Findings

**Auth code + PKCE with loopback redirect**

- Redirect URIs for native desktop apps are registered under the **"Mobile and desktop applications"** platform. This includes apps "that run natively on a mobile device or desktop machine" (Electron, Windows desktop, and so on) and "macOS apps using cross-plat tech we don't support (Electron)". The same applies to a Tauri app. — [Microsoft Learn: Redirect URI best practices (updated 2026-06-15)](https://learn.microsoft.com/en-us/entra/identity-platform/reply-url)
- Localhost exception (per RFC 8252 §7.3 / §8.3): `http` is accepted for loopback, and "the port component (for example, `:5001` or `:443`) is ignored for the purposes of matching a localhost redirect URI". So a desktop app can bind an ephemeral port and register only `http://localhost` (or `http://localhost/path`). — [Redirect URI best practices](https://learn.microsoft.com/en-us/entra/identity-platform/reply-url)
- Microsoft says to "prefer 127.0.0.1 over localhost" to avoid firewall or interface-renaming problems. However, an `http://127.0.0.1` redirect "can't" be added via the portal text box; you have to edit `replyUrlsWithType` in the app manifest. IPv6 loopback `[::1]` "isn't currently supported". — [Redirect URI best practices](https://learn.microsoft.com/en-us/entra/identity-platform/reply-url)
- Tension in the same doc: it also says HTTP "is supported _only_ for _localhost_ URIs and should be used only during active local application development and testing". This contradicts the RFC 8252 loopback section on the same page, which treats loopback as the native-app pattern. — [Redirect URI best practices](https://learn.microsoft.com/en-us/entra/identity-platform/reply-url)
- Do not register several localhost URIs that differ only by port. The server "picks one arbitrarily"; differentiate them by path instead. — [Redirect URI best practices](https://learn.microsoft.com/en-us/entra/identity-platform/reply-url)

**WAM (Windows Web Account Manager broker)**

- WAM is "a Windows component that ships with the OS" and acts as an authentication broker. Microsoft lists these benefits: enhanced security, "Windows Hello, conditional access policies, and FIDO keys", the built-in account picker, and **Token Protection** ("WAM ensures that the refresh tokens are device bound and enables apps to acquire device bound access tokens"). — [MSAL.NET: Using WAM (updated 2026-11-02)](https://learn.microsoft.com/en-us/entra/msal/dotnet/acquiring-tokens/desktop-mobile/wam)
- WAM requires Windows 10 1703+ / Windows Server 2019+. MSAL "will automatically fallback to a browser if WAM cannot be used". WAM "supports only Microsoft Entra ID". On Mac and Linux, MSAL falls back to a browser. — [MSAL.NET WAM](https://learn.microsoft.com/en-us/entra/msal/dotnet/acquiring-tokens/desktop-mobile/wam)
- WAM redirect URI to register (Mobile and desktop platform): `ms-appx-web://microsoft.aad.brokerplugin/{client_id}`. A parent window handle (HWND) must be supplied, and the app "MUST be running in the context of an active, interactive Windows user session". — [MSAL.NET WAM](https://learn.microsoft.com/en-us/entra/msal/dotnet/acquiring-tokens/desktop-mobile/wam)
- With WAM, `AcquireTokenSilent(..., OperatingSystemAccount)` can sign in silently as the Windows-session account. Microsoft's recommended pattern is: silent first, interactive only on `MsalUiRequiredException`. — [MSAL.NET WAM](https://learn.microsoft.com/en-us/entra/msal/dotnet/acquiring-tokens/desktop-mobile/wam)

**macOS broker / SSO**

- MSAL for macOS/iOS documents three things: SSO between apps from the same Apple developer via a shared keychain (`com.microsoft.identity.universalstorage` on macOS); brokered SSO "as long as the mobile device is registered with Microsoft Entra ID", which "requires a broker application be installed"; and browser SSO. Browser SSO with Safari "is currently not available on macOS. MSAL on macOS only supports WKWebView". The page dates from 2024-02 (updated 2025-07) and is mostly iOS-focused. — [MSAL ObjC: Configure SSO on macOS and iOS](https://learn.microsoft.com/en-us/entra/msal/objc/single-sign-on-macos-ios)

**Device code flow**

- Microsoft describes device code flow as for "input-constrained devices such as a smart TV, IoT device, or a printer". `/common`, `/consumers`, `/organizations` or a tenant are accepted. A refresh token is returned if `offline_access` was requested. The user has 15 minutes (`expires_in` default). — [Microsoft Learn: OAuth 2.0 device authorization grant (updated 2026-06-15)](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code)
- Conditional Access has an **Authentication Flows** condition that can target "Device code flow" and "Authentication transfer". Microsoft's guidance: "We recommend organizations get as close as possible to a unilateral block on device code flow … Only allow device code flow in well documented and secured use cases, like legacy tooling that can't be updated." The suggested scope is all users and all resources, with grant set to Block access. — [Microsoft Learn: Block authentication flows with Conditional Access (ms.date 2026-03-24)](https://learn.microsoft.com/en-us/entra/identity/conditional-access/policy-block-authentication-flows)
- Secondary sources report that Microsoft rolled out a **Microsoft-managed Conditional Access policy "Block device code flow"** starting around Feb 2025, targeting tenants with no recent device-code sign-ins (one source says "no device code sign-ins in the previous 25 days"). — [Sentrium Security](https://www.sentrium.co.uk/labs/blocking-device-code-flow-in-microsoft-entra-id), [vand3rlinden.com](https://vand3rlinden.com/post/m365-blocking-dcf-abuse/), [Cyber Defense](https://cyberdefense.co.th/insights/entra-device-code-sign-ins) (secondary; I did not confirm this against a Microsoft primary source).
- Device code phishing using first-party client IDs is a widely documented attack technique, which is why admins block the flow. — [TrustedSec](https://trustedsec.com/blog/the-new-hotness-in-phishing-device-code-attacks-in-m365), [CloudBreach](https://cloudbreach.io/blog/device-code-phishing-microsoft-authentication-broker)

**ROPC (username/password)**

- Microsoft "does not recommend" it. It "doesn't support personal Microsoft accounts and Microsoft Entra accounts with enabled multi-factor authentication". — [MSAL.NET WAM](https://learn.microsoft.com/en-us/entra/msal/dotnet/acquiring-tokens/desktop-mobile/wam)

### Inferences

- For a Rust/Tauri app, the practical default is auth code + PKCE: open the system browser and listen on `http://127.0.0.1:<ephemeral>` (register via the manifest) or `http://localhost`. This works on Windows and macOS with no broker dependency, and it hits whatever Conditional Access the tenant enforces in the browser. That includes device-compliance policies, which can fail in a non-broker browser flow if the browser lacks the device identity.
- WAM is the better Windows UX and is Microsoft's preferred option. It is also the only way to satisfy "require compliant / hybrid-joined device" and Token Protection CA policies from a native app on Windows without relying on browser extensions. From Rust it would require calling WinRT `Windows.Security.Authentication.Web.Core` directly or binding to Microsoft's native MSAL runtime. I did not verify either.
- Avoid device code flow as the main path. In 2026 there is a real chance it is blocked by a tenant CA policy, Microsoft-managed or admin-created, and it trains users into a phishing-prone pattern.

### Gaps

- I found no Microsoft primary source (Learn page or Message Center post) confirming the Microsoft-managed "Block device code flow" CA policy's rollout dates and criteria. A claim that new Security Defaults tenants block device code flow from 1 July 2026 came from a single blog and is unconfirmed.
- I did not find current (2026) Microsoft docs on WAM for non-.NET or non-MSAL callers (raw WinRT `WebAuthenticationCoreManager`, or the `msalruntime` C/C++ library used by MSAL Python/Node). The Rust feasibility of WAM is unverified.
- I did not fetch Microsoft docs on the macOS **Enterprise SSO plug-in / Platform SSO** for apps that do _not_ use MSAL. Whether a non-MSAL Tauri app gets SSO or device-compliance signals through it (for example, via ASWebAuthenticationSession interception) is unconfirmed.
- No Microsoft-supported MSAL exists for Rust (not verified in this session; based on absence in Microsoft docs consulted).

---

## 2. App registration requirements: own registration, multi-tenant public client, service principal on consent, publisher verification

### Takeaway

Yes, the app needs its own Entra app registration: a public client with "Allow public client flows" and a Mobile/desktop redirect. One multi-tenant registration ("Accounts in any organizational directory") in the developer's tenant can be used by users of any tenant. On first consent, a service principal plus a delegated permission grant is created in the user's tenant. Without publisher verification, which needs a Microsoft AI Cloud Partner Program account and a custom domain, consent from other tenants is restricted or warned, and many tenant consent policies exclude unverified apps entirely. For a colleague-only tool, a **single-tenant registration in the company tenant** avoids publisher verification and cross-tenant issues. It still needs admin consent for Calendars.Read (section 3).

### Cited Findings

- Multi-tenant setup: set Supported account types to "Accounts in any organizational directory" and send requests to `/common` or `/organizations`. — [Microsoft Learn: Convert single-tenant app to multitenant (updated 2026-06-15)](https://learn.microsoft.com/en-us/entra/identity-platform/howto-convert-app-to-be-multi-tenant)
- "When a user from a different tenant signs in to the application for the first time, Microsoft Entra ID asks them to consent … If they consent, then a representation of the application called a _service principal_ is created in the user's tenant … A delegation is also created in the directory that records the user's consent." — [Convert to multitenant](https://learn.microsoft.com/en-us/entra/identity-platform/howto-convert-app-to-be-multi-tenant)
- Admin "sign-up" for a whole tenant is done with an authorize request using `prompt=consent` by an admin. Once that is done, "no other users in the tenant are prompted for consent". If an admin consents _without_ `prompt=consent`, consent "applies only for their user account". — [Convert to multitenant](https://learn.microsoft.com/en-us/entra/identity-platform/howto-convert-app-to-be-multi-tenant)
- MSAL caching pitfall for multi-tenant apps: after the first `/common` sign-in, later calls should use the user's tenant endpoint, or the cache misses and the user is prompted again. — [Convert to multitenant](https://learn.microsoft.com/en-us/entra/identity-platform/howto-convert-app-to-be-multi-tenant)
- Native clients "don't validate access tokens and must treat them as opaque". — [Convert to multitenant](https://learn.microsoft.com/en-us/entra/identity-platform/howto-convert-app-to-be-multi-tenant); also "Don't attempt to validate or read tokens for any API you don't own". — [Device code flow doc](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code)
- **Publisher verification** requirements:
  - a verified Microsoft AI Cloud Partner Program (formerly MPN) **partner global account**;
  - the app registered with a work/school account ("Apps that are registered by using a Microsoft account can't be publisher verified");
  - the tenant associated with the PGA;
  - a publisher domain set, which "can't be *.onmicrosoft.com";
  - an MFA-signed-in user with Application Admin / Cloud App Admin plus a Partner Center admin role.

  It costs nothing and needs no license. — [Microsoft Learn: Publisher verification overview (updated 2026-06-15)](https://learn.microsoft.com/en-us/entra/identity-platform/publisher-verification-overview)

- "Beginning November 2020, if risk-based step-up consent is enabled, users can't consent to most newly registered multitenant apps that aren't publisher verified." This applies to apps registered after 8 Nov 2020 requesting more than basic sign-in/profile from users in _other_ tenants. A warning says the app "was created by an unverified publisher". — [Publisher verification overview](https://learn.microsoft.com/en-us/entra/identity-platform/publisher-verification-overview)
- The built-in `microsoft-user-default-low` policy ("Allow user consent for apps from verified publishers, for selected permissions") allows consent "only for apps from verified publishers and apps that are registered in your tenant, and only for permissions that you classify as low impact". — [Microsoft Learn: Configure how users consent (updated 2026-08-04)](https://learn.microsoft.com/en-us/entra/identity/enterprise-apps/configure-user-consent)
- "Applications that require users to be assigned to the application must have their permissions consented by an administrator." — [Configure user consent](https://learn.microsoft.com/en-us/entra/identity/enterprise-apps/configure-user-consent)
- Redirect URI limits: 256 for org-only audiences, 100 if personal accounts are included. Query parameters in redirect URIs are not allowed when personal accounts are supported. — [Redirect URI best practices](https://learn.microsoft.com/en-us/entra/identity-platform/reply-url)

### Inferences

- For a tool shared with colleagues in one company, a single-tenant registration in that tenant, created by or with an admin, is the simplest path. "Registered in your tenant" also counts as trusted under `microsoft-user-default-low`.
- Distributing to _other_ tenants with one multi-tenant registration is technically supported. Without publisher verification, though, expect consent warnings or blocks, and Calendars.Read would still need admin consent in each tenant on Microsoft's managed default. Each customer tenant's admin would effectively have to onboard the app.
- An individual developer without a company Partner Center account and custom domain cannot get publisher verification.

### Gaps

- I did not confirm whether the 2020 "unverified multitenant apps blocked from user consent" rule now applies regardless of the risk-based step-up setting, which was itself an older feature. The current Learn page still frames it as conditional on risk-based step-up consent being enabled.
- I did not check whether Microsoft has since 2025 started requiring publisher verification for _all_ new multi-tenant apps (there is no evidence either way in the sources consulted).

---

## 3. Consent: is Calendars.Read user-consentable, and how the 2025/2026 default consent policy changes affect it

### Takeaway

At the permission level, `Calendars.Read` (delegated) is still a "user-consentable" permission. Microsoft's **managed default user consent policy**, however, explicitly excludes all `Calendars.*` delegated permissions, plus Mail, Chat, OnlineMeetings, MailboxSettings and, since mid-2026, Contacts, Tasks and People. The only exemption is a short allow-list of popular mail clients. So in any tenant on the Microsoft-managed default (new tenants, and tenants that never chose a custom setting), a calendar-reading desktop app requires **admin consent**. Tenants that explicitly chose the legacy "Allow user consent for apps" or a custom policy are reported as unaffected. There, user consent for Calendars.Read may still work, subject to publisher/tenant conditions.

### Cited Findings

- General baseline (older framing still on the page): "By default, all users are allowed to consent to applications for permissions that don't require administrator consent. For example, by default, a user can consent to allow an app to access their mailbox…" Microsoft recommends allowing user consent "only for applications that have been published by a verified publisher". — [Configure how users consent (updated 2026-08-04)](https://learn.microsoft.com/en-us/entra/identity/enterprise-apps/configure-user-consent)
- Built-in policies include `microsoft-user-default-low`, `microsoft-user-default-recommended` ("Permissions consentable based on Microsoft's current recommendations") and `microsoft-user-default-allow-consent-apps` ("Popular mail clients consentable for users"). The two latter are "Microsoft managed policies. The conditions included in the policies are automatically updated based on Microsoft's latest security recommendations for end-user consent." — [Microsoft Learn: Manage app consent policies (updated 2026-08-28)](https://learn.microsoft.com/en-us/entra/identity/enterprise-apps/manage-app-consent-policies)
- **Key quote:** "The setting labeled 'Let Microsoft manage your consent settings,' the Microsoft managed policy, will update with Microsoft's latest recommended default consent settings. **This is also the default for a new tenant.** The setting's rules are currently: End users can consent for any user consentable delegated permissions EXCEPT:
  - For Microsoft Graph: `Files.Read.All`, `Files.ReadWrite.All`, `Sites.Read.All`, `Sites.ReadWrite.All`, `Mail.Read`, `Mail.ReadWrite`, `Mail.ReadBasic`, `Mail.Read.Shared`, `Mail.ReadBasic.Shared`, `Mail.ReadWrite.Shared`, `MailboxItem.Read`, **`Calendars.Read`, `Calendars.ReadBasic`, `Calendars.ReadWrite`, `Calendars.Read.Shared`, `Calendars.ReadWrite.Shared`**, `Chat.Read`, `Chat.ReadWrite`, `OnlineMeetings.Read`, `OnlineMeetings.ReadWrite`, `MailBoxFolder.Read`, `MailBoxFolder.ReadWrite`, `MailBoxSettings.Read`, `MailBoxSettings.ReadWrite`, `Contacts.ReadWrite`, `Contacts.Read.Shared`, `Contacts.ReadWrite.Shared`, `Tasks.Read`, `Tasks.Read.Shared`, `Tasks.ReadWrite`, `Tasks.ReadWrite.Shared`, `People.Read`.
  - For Office 365 Exchange Online: `EAS.AccessAsUser.All`, `EWS.AccessAsUser.All`, `IMAP.AccessAsUser.All`, `POP.AccessAsUser.All`."

  — [Manage app consent policies](https://learn.microsoft.com/en-us/entra/identity/enterprise-apps/manage-app-consent-policies)

- Mail client exemption: the **`microsoft-user-allow-default-consent-apps`** policy, "enabled by default", lets users consent to the above mail/calendar permissions only for Apple Mail (`f8d98a96-…`), Spark (`b50c1dbd-…`), eM Client (`e9a7fea1-…`), Android-Samsung, Android-Mail and Thunderbird (`9e5f94bc-…`). — [Manage app consent policies](https://learn.microsoft.com/en-us/entra/identity/enterprise-apps/manage-app-consent-policies)
- Timeline, from a secondary blog that tracks Message Center:
  - **Aug 2025** (others say July 2025): Files.Read.All, Files.ReadWrite.All, Sites.Read.All, Sites.ReadWrite.All.
  - **Nov 2025**: 24 permissions including all five `Calendars.*`, Mail._, Chat._, OnlineMeetings._, Mailbox_, and the EAS/EWS/IMAP/POP Exchange scopes. The headline says "20 additional", but the table lists 24.
  - **June–July 2026**: Contacts._, People.Read, Tasks._.

  "This update only affects tenants using the Microsoft-managed user consent policies … Organizations using a custom or admin-defined policy are not affected … Users who have already granted consent to an app can continue using it." — [Topedia blog, 13 May 2026](https://blog.topedia.com/2026/05/microsoft-managed-default-app-consent-policy-will-block-8-additional-permissions/); [Topedia, Nov 2025](https://blog.topedia.com/2025/11/microsoft-managed-default-app-consent-policy-now-blocks-20-additional-permissions/)

- From secondary blogs, as surfaced by search: starting July 2025, tenants still on the legacy "allow user consent for all apps" setting were automatically switched to the Microsoft-recommended managed policy. Tenants with explicitly configured settings were not affected. — [Interian blog, Jul 2025](https://blog.interian.be/2025/07/17/microsoft-entra-consent-changes-coming-july-2025-a-guide-for-admins/), [mattchatt.co.za](https://mattchatt.co.za/upcoming-change-to-entra-app-consent-defaults-what-admins-need-to-know-before-july-16/), [AppGovScore on MC1097272](https://www.appgovscore.com/blog/microsoft-disables-user-consent-by-default-are-you-ready-for-mc1097272). I did not read these in full.
- Changing consent settings "only affect[s] future consent operations … Existing consent grants remain unchanged". — [Configure user consent](https://learn.microsoft.com/en-us/entra/identity/enterprise-apps/configure-user-consent)
- **Admin consent workflow:** "To allow users to request an administrator's review and approval of an application that the user isn't allowed to consent to, enable the admin consent workflow." — [Configure user consent](https://learn.microsoft.com/en-us/entra/identity/enterprise-apps/configure-user-consent)
- Admins can fully disable user consent ("Do not allow user consent"). "If this capability is disabled, admin consent is always required for the application to be used in the tenant." — [Convert to multitenant](https://learn.microsoft.com/en-us/entra/identity-platform/howto-convert-app-to-be-multi-tenant)

### Inferences

- The managed policy's exceptions list does not exempt apps "registered in your tenant" or from verified publishers. Even a single-tenant app registered in the company's own tenant therefore cannot be self-consented for Calendars.Read under the managed default. An admin must grant tenant-wide consent once, after which colleagues sign in without a consent prompt.
- Feasibility for colleagues in the company tenant depends on getting one IT admin to (a) create or approve the app registration and (b) grant admin consent for `Calendars.Read` (or `Calendars.ReadBasic`, which omits body/attachments) plus `offline_access`/`User.Read`. Without that, the user gets an "approval required" / admin-consent-workflow screen.
- For other tenants, every tenant needs its own admin consent. That is a significant adoption barrier for an unverified hobby app.
- `Calendars.ReadBasic` does not help with consent. It is excluded too.

### Gaps

- I found no reliable statistic on what share of enterprise tenants block all user consent versus use the managed default versus legacy. There is no Microsoft-published figure and no credible survey.
- I did not read the Message Center posts themselves (MC numbers other than MC1097272 were not captured), so exact rollout dates rest on secondary blogs.
- The Graph permissions reference was not re-checked to confirm the "Admin consent required: No" flag for `Calendars.Read` delegated in Nov 2026. The managed policy's wording "user consentable delegated permissions EXCEPT …" implies it is still flagged user-consentable.

---

## 4. Well-known first-party client IDs (Office, Azure CLI, Graph PowerShell): can a third party use them?

### Takeaway

Technically, public first-party client IDs such as Microsoft Office `d3590ed6-52b3-4102-aeff-aad2292ab01c`, Azure CLI `04b07795-8ddb-461a-bbee-02f9e1bf7b46` and Microsoft Graph Command Line Tools `14d82eec-204b-4c2f-b7e8-296a70dab67e` can be used by any client, because they are public clients pre-provisioned or pre-authorized in tenants. In practice it is impersonating a Microsoft app. It is the signature of device-code and OAuth phishing that defenders' detection rules flag. Admins can disable or Conditional-Access-target these apps, and Microsoft is changing their infrastructure (Azure AD Graph retirement, Graph CLI moving to a new verified identity). It is fragile and policy-hostile, and should not be the basis for a distributed app.

### Cited Findings

- First-party FOCI ("family of client IDs") apps can redeem one another's refresh tokens. Secureworks found 15 of ~600 Microsoft OAuth apps could do this, "all of those were first-party, pre-authorized, and present by default in tenants". — [Secureworks FOCI research (GitHub)](https://github.com/secureworks/family-of-client-ids-research/blob/main/atte22.md); [Troopers22 slides](https://troopers.de/downloads/troopers22/TR22_AbusingFamilyRefreshTokens.pdf)
- Attackers use the Office client ID with device code flow and Graph scopes because it is "a high-value first-party client_id that carries broad scopes and is inherently trusted by the tenant". — [CloudBreach](https://cloudbreach.io/blog/device-code-phishing-microsoft-authentication-broker); [TrustedSec](https://trustedsec.com/blog/the-new-hotness-in-phishing-device-code-attacks-in-m365)
- Detection content treats sign-ins via FOCI app IDs and developer tools (Azure CLI, Azure PowerShell) reaching Graph, especially by non-developers, as suspicious. — [Elastic prebuilt rule: Entra ID OAuth Phishing via First-Party Microsoft Application](https://www.elastic.co/docs/reference/security/prebuilt-rules/rules/integrations/azure/initial_access_entra_id_oauth_phishing_via_first_party_microsoft_application)
- Admins can set "Enabled for users to sign-in" = No on the Microsoft Graph Command Line Tools enterprise app and require their own registered app instead. — [ourcloudnetwork.com](https://ourcloudnetwork.com/protect-microsoft-graph-powershell-logins-with-conditional-access/). This conflicts with the Elastic write-up's claim that first-party apps "cannot be blocked". [Elastic](https://www.elastic.co/docs/reference/security/prebuilt-rules/rules/integrations/azure/initial_access_entra_id_oauth_phishing_via_first_party_microsoft_application)
- Microsoft announced that Graph Command Line Tools would move to new infrastructure with "Verified" status. Organizations using their own registered app see "no impact". — [ourcloudnetwork.com](https://ourcloudnetwork.com/microsoft-graph-command-line-tools-to-become-verified-in-entra/) (secondary)
- Azure CLI and Az PowerShell kept first-party Azure AD Graph identities only as a temporary transition path. Microsoft states "Old releases may stop working at any point during this phased shutdown". This illustrates that Microsoft changes first-party app behavior without regard to third-party reuse. — [Microsoft Tech Community: Azure AD Graph API retirement](https://techcommunity.microsoft.com/blog/azuretoolsblog/azure-ad-graph-api-retirement-action-required-for-older-versions-of-azure-cli-an/4562225)
- A Microsoft troubleshooting article exists to help admins verify first-party apps in sign-in logs, so admins actively inspect them. — [Microsoft Learn: Verify first-party Microsoft applications in sign-in reports](https://learn.microsoft.com/en-us/troubleshoot/entra/entra-id/governance/verify-first-party-apps-sign-in)
- Publisher verification requires consenting to the Microsoft identity platform for developers Terms of Use, which govern use of the platform by _your_ registered apps. — [Publisher verification overview](https://learn.microsoft.com/en-us/entra/identity-platform/publisher-verification-overview) → [Terms of Use](https://learn.microsoft.com/en-us/legal/microsoft-identity-platform/terms-of-use)

### Inferences

- Reasons not to use them:
  - Sign-ins appear as "Microsoft Office" / "Azure CLI" / "Graph CLI" in the user's tenant logs. This is misleading and matches phishing detections, which could trigger security-team escalation against the colleague.
  - CA policies often target these apps specifically (block Azure CLI/PowerShell for non-admins, block device code).
  - The set of pre-authorized scopes on those apps is not under your control and may change.
  - Redirect URIs are fixed to Microsoft's values (for example `https://login.microsoftonline.com/common/oauth2/nativeclient` or Microsoft's localhost patterns), which constrains your flow.
  - It plausibly violates the identity platform terms and the employer's acceptable-use policy.
- Note that Graph Command Line Tools has Calendars.Read delegated only if consented in that tenant. It is not universally pre-granted, so it would not reliably bypass the consent problem anyway.

### Gaps

- I did not locate explicit Microsoft Terms of Use wording that prohibits using another publisher's or Microsoft's client ID. The ToS page was not fetched in full. Treat "against ToS" as unconfirmed.
- Whether the Office FOCI client is pre-authorized for Graph `Calendars.Read` in 2026, and whether Microsoft has introduced any server-side restriction on non-Microsoft callers using first-party IDs, is unconfirmed. I found no Microsoft announcement.

---

## 5. Token lifetimes and refresh: refresh token lifetime, offline_access, CAE, re-sign-in frequency

### Takeaway

Public desktop clients get refresh tokens (request `offline_access`) with a **90-day** inactivity window that renews on use and an "until-revoked" max age. In practice an app that refreshes at least once every 90 days keeps working indefinitely, unless the tenant's Conditional Access sign-in frequency, password resets/revocation, or CAE events force re-authentication. Access tokens last 60–90 minutes, or 24–28 hours for CAE-capable clients. Refresh/session token lifetimes are not configurable by admins (since Jan 2021). Sign-in frequency is controlled via Conditional Access.

### Cited Findings

- Refresh token defaults: "24 hours for single-page applications … 90 days for all other scenarios." "Refresh tokens replace themselves with a fresh token upon every use." The old one isn't revoked; the app should delete it. — [Microsoft Learn: Refresh tokens (updated 2026-06-15)](https://learn.microsoft.com/en-us/entra/identity-platform/refresh-tokens)
- Refresh tokens "are bound to a combination of user and client, but aren't tied to a resource or tenant". — [Refresh tokens](https://learn.microsoft.com/en-us/entra/identity-platform/refresh-tokens)
- Revocation table for public-client ("token") refresh tokens:
  - Password-based tokens are revoked on user password change, SSPR, or admin reset.
  - Non-password-based tokens (for example passwordless/FIDO/WHfB) survive user password changes but are revoked by admin resets in the Entra or M365 admin center.
  - All are revoked by user or admin "revoke all refresh tokens".
  - Password _expiry_ doesn't revoke.

  — [Refresh tokens](https://learn.microsoft.com/en-us/entra/identity-platform/refresh-tokens)

- Not configurable: "As of January 30, 2021, refresh and session token lifetimes are no longer configurable … To control how frequently users are required to sign in, use Conditional Access sign-in frequency." Defaults: Refresh Token Max Inactive Time **90 days**; Single/Multi-Factor Refresh Token Max Age **Until-revoked**. — [Microsoft Learn: Configurable token lifetimes (updated 2026-06-15)](https://learn.microsoft.com/en-us/entra/identity-platform/configurable-token-lifetimes)
- Access token lifetime: "a random value ranging between 60-90 minutes (75 minutes on average)". "When both the client and the resource support Continuous Access Evaluation (CAE), the token lifetime may be automatically extended to 24-28 hours … revoked in near real time in response to critical events such as account disablement and password changes." CAE-capable clients "may receive long-lived tokens (up to 28 hours)". — [Configurable token lifetimes](https://learn.microsoft.com/en-us/entra/identity-platform/configurable-token-lifetimes)
- A refresh token is only returned "if the original scope parameter included `offline_access`". — [Device code flow doc](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code)
- Apps must handle revocation "gracefully by redirecting the user to an interactive sign-in prompt". — [Refresh tokens](https://learn.microsoft.com/en-us/entra/identity-platform/refresh-tokens)
- WAM makes refresh tokens device-bound (Token Protection). — [MSAL.NET WAM](https://learn.microsoft.com/en-us/entra/msal/dotnet/acquiring-tokens/desktop-mobile/wam)

### Inferences

- For a background activity-log collector, the user typically signs in once. The app must store the refresh token securely (Windows Credential Manager / macOS Keychain) and rotate it on each use. Re-prompting happens after 90 days of no use, on password reset or revocation, or per tenant sign-in-frequency CA (commonly 1–30 days in stricter tenants), plus any MFA re-prompt policy.
- A raw-protocol Rust client that does not declare CAE capability (`xms_cc=cp1` client capability) would get normal 60–90-minute access tokens and refresh more often. That is harmless, but the client must handle `401` with claims challenges if CAE is enforced on Graph.

### Gaps

- I did not fetch the CAE concept doc for exact client-capability (`cp1`) and claims-challenge handling requirements for public clients, so these are noted from general knowledge and unverified here.
- Typical sign-in-frequency settings in enterprises are not quantified (no source).

---

## 6. Personal Microsoft accounts vs work accounts: `common` vs `organizations` authority

### Takeaway

Use `/organizations` (or the specific tenant ID for a single-tenant app) for work/school-only access. `/common` additionally admits personal Microsoft accounts (outlook.com), which requires the registration's sign-in audience to include personal accounts and brings extra restrictions. After first sign-in, MSAL-style clients should switch to the user's home-tenant authority for cache hits.

### Cited Findings

- "`https://login.microsoftonline.com/common` for applications processing accounts in any organizational directory … and personal Microsoft accounts"; "`https://login.microsoftonline.com/organizations` for applications processing accounts in any organizational directory". "You can replace it by `organizations` if your application doesn't support Microsoft personal accounts." — [Convert to multitenant](https://learn.microsoft.com/en-us/entra/identity-platform/howto-convert-app-to-be-multi-tenant)
- Device code flow accepts `/common`, `/consumers`, `/organizations` or a tenant. Personal-account users on `/common`/`/consumers` "are asked to sign in again" to transfer auth state. — [Device code flow doc](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code)
- Registrations that include personal accounts (`AzureADandPersonalMicrosoftAccount`) are limited to 100 redirect URIs and cannot use query parameters or wildcards in redirect URIs. — [Redirect URI best practices](https://learn.microsoft.com/en-us/entra/identity-platform/reply-url)
- Token lifetime policies "aren't supported for applications developed for personal Microsoft accounts". — [Configurable token lifetimes](https://learn.microsoft.com/en-us/entra/identity-platform/configurable-token-lifetimes)
- With WAM, if the authority includes personal accounts, the Windows system account picker is shown. A tenant-specific or org-only authority shows a generic Microsoft prompt instead. — [MSAL.NET WAM](https://learn.microsoft.com/en-us/entra/msal/dotnet/acquiring-tokens/desktop-mobile/wam)
- Personal-account (MSA) tokens "may also be encrypted for consumer (Microsoft account) users". This is another reason to treat tokens as opaque. — [Device code flow doc](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code)

### Inferences

- For the stated use case (colleagues in a company tenant), use the company tenant ID as authority with a single-tenant registration. Use `/organizations` only if other tenants are targeted. Supporting outlook.com means `/common` and the "any org + personal accounts" audience. Personal accounts have no tenant admin consent policy, so `Calendars.Read` is user-consented there.

### Gaps

- I did not verify current (2026) consent behavior or any Microsoft restrictions for _personal_ accounts with Graph `Calendars.Read`, for example whether unverified apps get warnings for MSA users. This was out of focus and unverified.
