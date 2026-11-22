// Every theme's text is readable on every surface it is drawn on: `--text-primary`,
// `--text-secondary` and `--text-tertiary` reach WCAG AA for normal text (4.5:1) on `--bg`,
// `--surface`, `--surface-alt` and `--surface-hover` (a hovered row), and keep their order, so
// secondary text stands out from tertiary on each of them. The tokens are read from each theme's
// block in `dashboard/src/styles.scss` (`[data-theme-option='…']`), for every theme the app
// offers (`THEMES` in `services/theme.service.ts`).
//
// The accent reads too. What is drawn on an accent fill (a primary button, Material's filled
// buttons and switches, the selected day, the active day's pill) takes its `-fg` token, which
// reaches 4.5:1 on the fill and on its hover; the brand mark's icon reaches 3:1 on both ends of
// its tile. The accent itself, as text (links, Add source, the info tone's badges and pills),
// reaches 4.5:1 on `--bg`, `--surface`, `--surface-alt` and its own info tint; as an icon or a
// focus ring (the active view's icon on `--accent-soft`, a hovered row), 3:1. A token a theme
// does not declare comes from the dark block (`html.dark`, for a dark theme) or `:root`, resolved
// with the theme's own values, as the browser does on `html`.
// Run with `node --test scripts/`.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

const DASHBOARD = new URL('../dashboard/src/', import.meta.url)
const STYLES = readFileSync(new URL('styles.scss', DASHBOARD), 'utf8')
const THEME_SERVICE = readFileSync(new URL('app/services/theme.service.ts', DASHBOARD), 'utf8')

const TEXT = ['--text-primary', '--text-secondary', '--text-tertiary']
const SURFACES = ['--bg', '--surface', '--surface-alt', '--surface-hover']
const AA_NORMAL_TEXT = 4.5
const AA_NON_TEXT = 3

/** Each fill drawn in the accent, with the token of what is drawn on it. */
const ACCENT_FILLS = [
  ['--accent-fg', '--accent'],
  ['--accent-fg', '--accent-hover'],
  ['--btn-primary-fg', '--btn-primary'],
  ['--btn-primary-fg', '--btn-primary-hover'],
]
/** The brand mark's tile, both ends of its gradient, under its mark (an icon). */
const BRAND_MARK = ['--brand-mark-from', '--brand-mark-to']
const ACCENT_TEXT_ON = ['--bg', '--surface', '--surface-alt']
const ACCENT_ICON_ON = ['--surface-hover']
/** The info tone's tint behind accent text: `ui.status-tone($mix: 10%)` over `--surface`, as the
 * collection status and the feedback line draw it. */
const INFO_TONE_MIX = 0.1

/** Each top-level rule's selectors and the custom properties it declares. Comments are left out. */
function rules(scss) {
  const source = scss.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/[^\n]*/g, '')
  return [...source.matchAll(/([^{};]+)\{([^{}]*)\}/g)].map(rule => ({
    selectors: rule[1].split(',').map(selector => selector.trim()),
    tokens: new Map(
      [...rule[2].matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)].map(([, name, value]) => [
        name,
        value.trim(),
      ])
    ),
  }))
}

/** What the rules matching `selector` declare, the later rule winning. */
function declaredBy(selector) {
  const tokens = new Map()
  for (const rule of RULES.filter(rule => rule.selectors.includes(selector))) {
    for (const [name, value] of rule.tokens) tokens.set(name, value)
  }
  return tokens
}

function channels(hex) {
  const match = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex)
  if (!match) return null
  return match.slice(1).map(pair => parseInt(pair, 16) / 255)
}

/** WCAG 2's relative luminance of an sRGB colour. */
function luminance(rgb) {
  const [r, g, b] = rgb.map(c => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4))
  return 0.2126 * r + 0.7152 * g + 0.0722 * b
}

/** WCAG 2's contrast ratio of two `#rrggbb` colours, from 1 to 21. */
function contrast(a, b) {
  const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return (light + 0.05) / (dark + 0.05)
}

/** `p` of `a` over `b`, as `color-mix(in srgb, a p, b)` gives it. */
function mix(a, p, b) {
  return a.map((channel, index) => channel * p + b[index] * (1 - p))
}

/**
 * A token's colour in a theme, as the browser computes it on `html`: the theme's declaration, or
 * the dark block's for a dark theme, or `:root`'s, with `var()` and `color-mix(in srgb, …)` resolved in the theme. A mix with
 * `transparent` is laid over `backdrop`. Returns null for a value it cannot read.
 */
function resolve(theme, name, backdrop) {
  const value =
    themes.get(theme)?.get(name) ??
    (darkThemes.includes(theme) ? darkClass.get(name) : undefined) ??
    root.get(name) ??
    ''
  const variable = /^var\((--[\w-]+)\)$/.exec(value)
  if (variable) return resolve(theme, variable[1], backdrop)
  const mixed =
    /^color-mix\(in srgb, var\((--[\w-]+)\) ([\d.]+)%, (transparent|var\((--[\w-]+)\))\)$/.exec(
      value
    )
  if (mixed) {
    const first = resolve(theme, mixed[1], backdrop)
    const second = mixed[3] === 'transparent' ? backdrop : resolve(theme, mixed[4], backdrop)
    return first && second && mix(first, Number(mixed[2]) / 100, second)
  }
  return channels(value)
}

const RULES = rules(STYLES)
const offered = [...THEME_SERVICE.matchAll(/\{ id: '([\w-]+)', label: /g)].map(match => match[1])
const dark = /const DARK_THEMES: Theme\[\] = \[([^\]]*)\]/.exec(THEME_SERVICE)?.[1] ?? ''
const darkThemes = [...dark.matchAll(/'([\w-]+)'/g)].map(match => match[1])
/** Each theme's own block, `[data-theme-option='…']`. */
const themes = new Map(offered.map(theme => [theme, declaredBy(`[data-theme-option='${theme}']`)]))
/** Every dark theme's `html` carries `.dark` (`ThemeService`), so the dark block's tokens apply
 * to it where its own block does not declare them; then `:root`'s. */
const darkClass = declaredBy('html.dark')
const root = declaredBy(':root')

test('the app offers themes, and each has a block of tokens', () => {
  assert.ok(offered.length > 1, 'THEMES lists the themes')
  for (const theme of offered) assert.ok(themes.has(theme), `${theme} has a token block`)
})

for (const theme of offered) {
  test(`${theme}: text reaches ${AA_NORMAL_TEXT}:1 on every surface and keeps its order`, () => {
    const tokens = themes.get(theme)
    const colour = name => {
      const rgb = channels(tokens?.get(name) ?? '')
      assert.ok(rgb, `${theme} ${name} is a #rrggbb colour, not ${tokens?.get(name)}`)
      return rgb
    }
    for (const surface of SURFACES) {
      const ratios = TEXT.map(text => contrast(colour(text), colour(surface)))
      TEXT.forEach((text, index) =>
        assert.ok(
          ratios[index] >= AA_NORMAL_TEXT,
          `${theme} ${text} on ${surface}: ${ratios[index].toFixed(2)}:1`
        )
      )
      assert.ok(
        ratios[0] > ratios[1] && ratios[1] > ratios[2],
        `${theme} on ${surface}: primary, secondary and tertiary in that order (${ratios
          .map(ratio => ratio.toFixed(2))
          .join(', ')})`
      )
    }
  })
}

for (const theme of offered) {
  test(`${theme}: what is drawn on the accent, and the accent drawn on surfaces, reads`, () => {
    const colour = (name, backdrop) => {
      const rgb = resolve(theme, name, backdrop)
      assert.ok(rgb, `${theme} ${name} resolves to a colour`)
      return rgb
    }
    const surface = colour('--surface')
    const reaches = (fg, bg, minimum, what) => {
      const ratio = contrast(fg, bg)
      assert.ok(ratio >= minimum, `${theme} ${what}: ${ratio.toFixed(2)}:1, under ${minimum}:1`)
    }
    for (const [fg, fill] of ACCENT_FILLS) {
      reaches(colour(fg), colour(fill), AA_NORMAL_TEXT, `${fg} on ${fill}`)
    }
    const accent = colour('--accent')
    for (const on of ACCENT_TEXT_ON) {
      reaches(accent, colour(on), AA_NORMAL_TEXT, `--accent as text on ${on}`)
    }
    reaches(
      accent,
      mix(accent, INFO_TONE_MIX, surface),
      AA_NORMAL_TEXT,
      `--accent as text on its info tint`
    )
    for (const on of ACCENT_ICON_ON) {
      reaches(accent, colour(on), AA_NON_TEXT, `--accent as an icon on ${on}`)
    }
    reaches(accent, colour('--accent-soft', surface), AA_NON_TEXT, '--accent on --accent-soft')
    for (const end of BRAND_MARK) {
      reaches(colour('--brand-mark-fg'), colour(end), AA_NON_TEXT, `--brand-mark-fg on ${end}`)
    }
  })
}

test('a token resolves through var() and color-mix() in its theme', () => {
  const light = resolve('light', '--accent')
  assert.ok(light, 'the light accent is a colour')
  const soft = resolve('light', '--accent-soft', [1, 1, 1])
  assert.ok(soft, '--accent-soft resolves over a backdrop')
  assert.deepEqual(soft, mix(light, 0.14, [1, 1, 1]))
  assert.deepEqual(resolve('green', '--btn-primary'), resolve('green', '--accent'))
  assert.deepEqual(resolve('dark', '--btn-primary'), resolve('light', '--brand'))
})

test('a dark theme takes what it does not declare from the dark block, then from :root', () => {
  assert.ok(
    darkThemes.includes('dark') && darkThemes.length > 1,
    'DARK_THEMES lists the dark themes'
  )
  assert.ok(!themes.get('dim')?.has('--status-ok-fg') && darkClass.has('--status-ok-fg'))
  assert.deepEqual(resolve('dim', '--status-ok-fg'), resolve('dark', '--status-ok-fg'))
  assert.notDeepEqual(resolve('green', '--status-ok-fg'), resolve('dark', '--status-ok-fg'))
})

test('the contrast ratio is WCAG 2’s', () => {
  assert.equal(contrast(channels('#000000'), channels('#ffffff')).toFixed(1), '21.0')
  assert.equal(contrast(channels('#767676'), channels('#ffffff')).toFixed(2), '4.54')
  assert.equal(contrast(channels('#777777'), channels('#ffffff')).toFixed(2), '4.48')
})
