# Activity Log

Activity Log collects activity from multiple providers and presents a unified timeline. This glossary captures the project-specific terms that define how providers are checked and when collection actually happens.

## Language

**Provider startup check**:
A non-invasive readiness check that determines whether a provider is usable before collection begins.
_Avoid_: probe, startup collection, background collection

**Collection run**:
A fetch of activity entries from one or more providers for a requested date range.
_Avoid_: startup check, provider probe, refresh check

**PowerShell compatibility path**:
An existing Windows provider integration that is temporarily kept for parity even though it relies on shelling out.
_Avoid_: normal Windows integration, preferred Windows path

**Native Windows integration**:
A Windows provider integration that talks to Windows facilities directly instead of spawning shell commands.
_Avoid_: PowerShell path, command-based Windows integration

**PowerShell exception provider**:
A provider that is explicitly allowed to remain on the **PowerShell compatibility path** until a safe native replacement exists.
_Avoid_: standard Windows provider, permanent PowerShell provider

**Visibility fix phase**:
The phase that keeps existing provider behavior but removes any user-visible Windows command windows.
_Avoid_: native migration, provider rewrite

**Native migration phase**:
The phase that replaces a **PowerShell exception provider** with a **Native Windows integration**.
_Avoid_: visibility fix, shell hardening

**Stability-first migration**:
A native replacement may ship before full parity as long as it is reliable on the Windows versions the project chooses to support.
_Avoid_: parity-first migration, feature-complete-before-ship

**Native Windows baseline**:
The minimum Windows version the project explicitly targets for native Windows provider integrations.
_Avoid_: any Windows version, legacy Windows support

## Relationships

- A **Provider startup check** determines whether a provider is usable for a **Collection run**
- A **Collection run** gathers entries from one or more usable providers
- A **PowerShell compatibility path** is allowed only as an exception while **Native Windows integration** is the default direction for new Windows providers
- **Outlook** and **System** are the only current **PowerShell exception providers**
- The **Visibility fix phase** comes before the **Native migration phase**
- The **Native migration phase** follows a **Stability-first migration** rule
- The current **Native Windows baseline** is **Windows 11**

## Example dialogue

> **Dev:** "At app launch, should we do a **Provider startup check** or a full **Collection run**?"
> **Domain expert:** "Both can happen, but a **Provider startup check** must stay non-invasive while a **Collection run** is allowed to do the real work."

## Flagged ambiguities

- "startup" was used to mean both **Provider startup check** and **Collection run** — resolved: these are distinct concepts.
- "Windows integration" was used to mean both **PowerShell compatibility path** and **Native Windows integration** — resolved: new Windows work defaults to **Native Windows integration**.
- "temporary PowerShell support" was vague — resolved: only **Outlook** and **System** are current **PowerShell exception providers**, and both remain migration targets.
- "fix Windows collection" was ambiguous — resolved: the **Visibility fix phase** and the **Native migration phase** are separate decisions.
- "weaker integration" was clarified as **Stability-first migration** — reliability on supported modern Windows versions matters more than immediate parity.
- "modern Windows systems" was ambiguous — resolved: the current **Native Windows baseline** is **Windows 11**.
