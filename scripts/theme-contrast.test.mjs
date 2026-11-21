// Every theme's text is readable on every surface it is drawn on: `--text-primary`,
// `--text-secondary` and `--text-tertiary` reach WCAG AA for normal text (4.5:1) on `--bg`,
// `--surface`, `--surface-alt` and `--surface-hover` (a hovered row), and keep their order, so
// secondary text stands out from tertiary on each of them. The tokens are read from each theme's
// block in `dashboard/src/styles.scss` (`[data-theme-option='…']`), for every theme the app
// offers (`THEMES` in `services/theme.service.ts`).
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

/** The custom properties each theme's block declares, by theme. Comments are left out. */
function themeTokens(scss) {
  const source = scss.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/[^\n]*/g, '')
  const themes = new Map()
  for (const block of source.matchAll(/\[data-theme-option='([\w-]+)'\]\s*\{([^{}]*)\}/g)) {
    const tokens = themes.get(block[1]) ?? new Map()
    for (const declaration of block[2].matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) {
      tokens.set(declaration[1], declaration[2].trim())
    }
    themes.set(block[1], tokens)
  }
  return themes
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

const offered = [...THEME_SERVICE.matchAll(/\{ id: '([\w-]+)', label: /g)].map(match => match[1])
const themes = themeTokens(STYLES)

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

test('the contrast ratio is WCAG 2’s', () => {
  assert.equal(contrast(channels('#000000'), channels('#ffffff')).toFixed(1), '21.0')
  assert.equal(contrast(channels('#767676'), channels('#ffffff')).toFixed(2), '4.54')
  assert.equal(contrast(channels('#777777'), channels('#ffffff')).toFixed(2), '4.48')
})
