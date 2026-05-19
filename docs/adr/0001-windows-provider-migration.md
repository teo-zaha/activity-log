# Windows provider migration

Windows provider integrations follow a two-phase path: first remove any visible command windows from the existing PowerShell-based Outlook and System providers, then migrate those two providers to native Windows integrations. PowerShell remains a temporary compatibility path only for Outlook and System, new Windows providers must be native from the start, native migrations may favor Windows 11 stability over immediate parity, and migrated providers should report an explicit unavailable reason on Windows 10 rather than keeping a long-term fallback path.
