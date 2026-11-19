// jsdom has no `matchMedia`, which Signality's `mediaQuery` calls; here no media query matches.
if (typeof window.matchMedia !== 'function') {
  window.matchMedia = (query: string): MediaQueryList => {
    const list: MediaQueryList = Object.assign(new EventTarget(), {
      matches: false,
      media: query,
      onchange: null,
      addListener: (): void => undefined,
      removeListener: (): void => undefined,
    })
    return list
  }
}
