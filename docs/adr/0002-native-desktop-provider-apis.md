# Native desktop APIs for local providers

Outlook and System stay local desktop providers owned by `core`, but their Windows and macOS integrations move from command-based shells and script runners to direct platform APIs. We are choosing this so `cli`, `api`, and `desktop` keep a shared provider model while the local providers stop depending on spawned commands, preserve their current product meaning, and normalize Outlook collection around default-calendar-only overlap filtering.
