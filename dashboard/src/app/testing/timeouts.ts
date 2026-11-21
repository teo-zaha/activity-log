/**
 * The test options of a spec that renders the app or its shell. A file's first test pays for the
 * first render of the app's components: about 0.6 s on an idle machine, 5–7 s at a load average
 * around 80 (parallel builds), past Vitest's 5 s default. Only these files get the longer limit.
 */
export const APP_RENDER_TEST = { timeout: 20_000 } as const
