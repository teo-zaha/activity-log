# Activity Log

Activity Log collects activity from multiple providers and presents a unified timeline. This glossary captures the project-specific terms that define how providers are checked and when collection actually happens.

## Language

**Provider startup check**:
A non-invasive readiness check that determines whether a provider is usable before collection begins.
_Avoid_: probe, startup collection, background collection

**Collection run**:
A fetch of activity entries from one or more providers for a requested date range.
_Avoid_: startup check, provider probe, refresh check

**Command-based provider integration**:
A local provider integration that shells out to operating-system commands or script interpreters to inspect readiness or collect entries.
_Avoid_: native provider, platform API integration, library-backed provider

**Native provider integration**:
A local provider integration that talks to platform libraries or desktop APIs directly from Rust instead of spawning commands.
_Avoid_: shell path, command-based provider, library-backed provider

**Library-backed provider integration**:
A provider integration that embeds a Rust or domain library directly for collection instead of spawning commands or talking to operating-system desktop APIs.
_Avoid_: shell path, native desktop provider

**PowerShell compatibility path**:
A Windows-specific **Command-based provider integration** that exists only as a temporary compatibility step.
_Avoid_: normal Windows integration, preferred Windows path

**Command-exception provider**:
A provider that is explicitly allowed to remain on a **Command-based provider integration** until a safe native replacement exists.
_Avoid_: standard provider, permanent shell provider

**Visibility fix phase**:
The phase that keeps existing provider behavior but removes user-visible command execution side effects.
_Avoid_: native migration, provider rewrite

**Native migration phase**:
The phase that replaces a **Command-exception provider** with a **Native provider integration**.
_Avoid_: visibility fix, shell hardening

**Stability-first migration**:
An API-backed replacement may ship before full parity as long as it is reliable on the operating-system versions the project chooses to support.
_Avoid_: parity-first migration, feature-complete-before-ship

**Native Windows baseline**:
The minimum Windows version the project explicitly targets for native Windows provider integrations.
_Avoid_: any Windows version, legacy Windows support

## Relationships

- A **Provider startup check** determines whether a provider is usable for a **Collection run**
- A **Collection run** gathers entries from one or more usable providers
- A **Command-based provider integration** is allowed only as an exception while **Native provider integration** is the default direction for local desktop providers
- A **Library-backed provider integration** is neither a shell path nor a native desktop path
- The **PowerShell compatibility path** is one Windows-specific example of a **Command-based provider integration**
- **Git** is the current **Library-backed provider integration**
- **Outlook** and **System** are the only current **Command-exception providers**
- The **Visibility fix phase** comes before the **Native migration phase**
- The **Native migration phase** follows a **Stability-first migration** rule
- The current **Native Windows baseline** is **Windows 11**

## Example dialogue

> **Dev:** "Is `Outlook` still a **Command-based provider integration** on macOS if it talks to AppleScript APIs without spawning `osascript`?"
> **Domain expert:** "No. Once it uses platform APIs directly from Rust, it is a **Native provider integration** even if the platform API itself is script-oriented."

## Flagged ambiguities

- "startup" was used to mean both **Provider startup check** and **Collection run** — resolved: these are distinct concepts.
- "native" was used to mean both "not PowerShell" and "direct platform APIs" — resolved: use **Native provider integration** only for direct Rust-to-platform API work.
- "embedded library" did not fit the earlier command-vs-native wording — resolved: use **Library-backed provider integration** for providers like **Git** that embed a domain library directly.
- "temporary shell support" was vague — resolved: only **Outlook** and **System** are current **Command-exception providers**, and both remain migration targets.
- "fix Windows collection" was ambiguous — resolved: the **Visibility fix phase** and the **Native migration phase** are separate decisions.
- "weaker integration" was clarified as **Stability-first migration** — reliability on supported modern Windows versions matters more than immediate parity.
- "modern Windows systems" was ambiguous — resolved: the current **Native Windows baseline** is **Windows 11**.
