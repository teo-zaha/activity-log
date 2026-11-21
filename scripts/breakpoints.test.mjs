// The named breakpoints exist twice: `$breakpoints` in `components/_dashboard-shared.scss`, which
// the styles' `ui.down`/`ui.up` read, and `BREAKPOINTS` in `utils/breakpoints.ts`, which
// `BreakpointState` and the specs' media fakes read. This test requires both to name the same
// breakpoints with the same widths. The SCSS map is read as Sass compiles it, the TypeScript one
// from its syntax tree; a width that is no plain number of pixels is refused.
// Run with `node --test scripts/`.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'
import ts from 'typescript'

const DASHBOARD = fileURLToPath(new URL('../dashboard/', import.meta.url))
const SCSS = join(DASHBOARD, 'src/app/components/_dashboard-shared.scss')
const TS = join(DASHBOARD, 'src/app/utils/breakpoints.ts')

/** Sass as the dashboard's build compiles it (`@angular/build` brings it). */
function sass() {
  const build = createRequire(join(DASHBOARD, 'package.json')).resolve('@angular/build')
  return createRequire(build)(createRequire(build).resolve('sass', { paths: [dirname(build)] }))
}

/** `$breakpoints` of the shared stylesheet's `source`, in pixels by name, in its order. */
export function scssBreakpoints(source) {
  const { css } = sass().compileString(
    `@use 'sass:math';
     ${source}
     @each $name, $width in $breakpoints {
       @if math.unit($width) != 'px' { @error 'Breakpoint #{$name} is not in px: #{$width}'; }
       .breakpoint { name: '#{$name}'; width: math.div($width, 1px); }
     }`,
    { style: 'expanded' }
  )
  return Object.fromEntries(
    [...css.matchAll(/name: "([^"]+)";\s*width: ([^;]+);/g)].map(([, name, width]) => [
      name,
      Number(width),
    ])
  )
}

/** `BREAKPOINTS` of `breakpoints.ts`'s `source`, in pixels by name, in its order. */
export function tsBreakpoints(source) {
  const file = ts.createSourceFile('breakpoints.ts', source, ts.ScriptTarget.Latest, true)
  for (const statement of file.statements) {
    if (!ts.isVariableStatement(statement)) continue
    for (const declaration of statement.declarationList.declarations) {
      if (!ts.isIdentifier(declaration.name) || declaration.name.text !== 'BREAKPOINTS') continue
      let value = declaration.initializer
      while (value && (ts.isAsExpression(value) || ts.isSatisfiesExpression(value))) {
        value = value.expression
      }
      assert.ok(value && ts.isObjectLiteralExpression(value), 'BREAKPOINTS is an object literal')
      return Object.fromEntries(
        value.properties.map(property => {
          assert.ok(ts.isPropertyAssignment(property), `${property.getText()} is a plain property`)
          const name = property.name
          assert.ok(
            ts.isIdentifier(name) || ts.isStringLiteral(name),
            `${name.getText()} is a plain name`
          )
          const width = property.initializer
          assert.ok(ts.isNumericLiteral(width), `${name.text}'s width is a number`)
          return [name.text, Number(width.text)]
        })
      )
    }
  }
  assert.fail('breakpoints.ts declares no BREAKPOINTS')
}

const scssSource = readFileSync(SCSS, 'utf8')
const tsSource = readFileSync(TS, 'utf8')

test('the SCSS and the TypeScript breakpoints are the same', () => {
  const scss = scssBreakpoints(scssSource)
  assert.ok(Object.keys(scss).length > 0, 'the SCSS names breakpoints')
  assert.deepEqual(tsBreakpoints(tsSource), scss)
})

test('a width changed on either side is found', () => {
  const scss = scssBreakpoints(scssSource)
  const ts = tsBreakpoints(tsSource)
  assert.notDeepEqual(
    tsBreakpoints(tsSource.replace(/phone: 640/, 'phone: 641')),
    scss,
    'a TypeScript width'
  )
  assert.notDeepEqual(
    scssBreakpoints(scssSource.replace(/'phone': 640px/, "'phone': 641px")),
    ts,
    'an SCSS width'
  )
  assert.notDeepEqual(
    tsBreakpoints(tsSource.replace(/\s*'phone-narrow': 400,/, '')),
    scss,
    'a breakpoint left out'
  )
})

test('a width that is no plain number of pixels is refused', () => {
  assert.throws(() => scssBreakpoints(scssSource.replace(/'phone': 640px/, "'phone': 40em")))
  assert.throws(() => tsBreakpoints(tsSource.replace(/phone: 640/, 'phone: 600 + 40')))
})
