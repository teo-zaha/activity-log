// The dashboard's touch rules, checked over its stylesheets (SCSS files and components' inline
// `styles`) and `index.html`:
// - every `:hover` goes through the `ui.hover` mixin (`components/_dashboard-shared.scss`), which
//   applies it only where the pointer hovers and not after a touch, and shows the same look while
//   a control is pressed; the compiled mixin is pinned below;
// - every scroller says what its overscroll does (`overscroll-behavior` beside its `overflow`, on
//   the scroller itself, for each axis it scrolls), so it hands its bounce on only where it means to;
// - the viewport covers the whole screen (`viewport-fit=cover`) and keeps pinch zoom.
// Run with `node --test scripts/`.
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, join, relative } from 'node:path'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'
import ts from 'typescript'

const DASHBOARD = fileURLToPath(new URL('../dashboard/', import.meta.url))
const SOURCE = join(DASHBOARD, 'src')

/** A stylesheet the checker cannot read; names where, so the file can be found. */
class UnreadableStylesheet extends Error {}

/**
 * The blocks of a stylesheet, each with its prelude (a selector or an at-rule), its declarations
 * and its parent. Comments are dropped; strings and `#{…}` interpolation are kept as text. A
 * comment, string, interpolation or block that does not end is an `UnreadableStylesheet`.
 */
function blocks(source) {
  const root = { prelude: '', declarations: [], parent: null }
  const found = []
  let current = root
  let text = ''
  const lineAt = index => source.slice(0, index).split('\n').length
  const unreadable = (what, index) => {
    throw new UnreadableStylesheet(`${what} that does not end, from line ${lineAt(index)}`)
  }

  const declare = () => {
    const declaration = /^\s*(-?[a-z][a-z-]*)\s*:\s*([\s\S]*?)\s*$/i.exec(text)
    if (declaration) {
      current.declarations.push({ property: declaration[1].toLowerCase(), value: declaration[2] })
    }
    text = ''
  }

  for (let i = 0; i < source.length; i++) {
    const char = source[i]
    const next = source[i + 1]
    if (char === '/' && next === '*') {
      const end = source.indexOf('*/', i + 2)
      if (end < 0) unreadable('a comment', i)
      i = end + 1
    } else if (char === '/' && next === '/') {
      const end = source.indexOf('\n', i)
      i = end < 0 ? source.length : end - 1
    } else if (char === '"' || char === "'") {
      let end = i + 1
      while (end < source.length && source[end] !== char) end += source[end] === '\\' ? 2 : 1
      if (end >= source.length) unreadable('a string', i)
      text += source.slice(i, end + 1)
      i = end
    } else if (char === '#' && next === '{') {
      let depth = 0
      let end = i + 1
      for (; end < source.length; end++) {
        if (source[end] === '{') depth++
        else if (source[end] === '}' && --depth === 0) break
      }
      if (end >= source.length) unreadable('an interpolation', i)
      text += source.slice(i, end + 1)
      i = end
    } else if (char === '{') {
      const block = { prelude: text.trim().replace(/\s+/g, ' '), declarations: [], parent: current }
      block.start = i
      found.push(block)
      current = block
      text = ''
    } else if (char === ';') {
      declare()
    } else if (char === '}') {
      if (current === root) throw new UnreadableStylesheet(`a stray } on line ${lineAt(i)}`)
      declare()
      current = current.parent
    } else {
      text += char
    }
  }
  if (current !== root) unreadable('a block', current.start)
  return found
}

function ancestors(block) {
  const chain = []
  for (let parent = block.parent; parent; parent = parent.parent) chain.push(parent)
  return chain
}

/** A prelude as a selector reads it: what quotes or attribute brackets hold is data, not syntax. */
function selectorSyntax(prelude) {
  return prelude.replace(/"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'/g, '""').replace(/\[[^\]]*\]/g, '[]')
}

const HOVER_USE = /^@include\s+(?:ui\.)?hover\b/
const HOVER_MIXIN = /^@mixin\s+hover\b/
const HOVER_MEDIA = /\(\s*(?:any-)?hover\s*:/
const SCROLLS = /^(?:auto|scroll)$/
/** At-rules a declaration inside them shares with the rule around them. */
const WRAPPER = /^@(?:media|supports|include|layer|container)\b/

/** The axes, `x` and `y`, that a two-value property such as `overflow` names in its value. */
function axesOf(property, value, base) {
  const parts = value.trim().split(/\s+/)
  if (property === base) return { x: parts[0], y: parts[1] ?? parts[0] }
  if (property === `${base}-x` || property === `${base}-inline`) return { x: parts[0] }
  if (property === `${base}-y` || property === `${base}-block`) return { y: parts[0] }
  return {}
}

/** The block a declaration belongs to (a rule or a mixin) and the wrappers between them. */
function scopeOf(block) {
  const scope = [block]
  let owner = block
  while (WRAPPER.test(owner.prelude) && owner.parent && owner.parent.prelude !== '') {
    owner = owner.parent
    scope.push(owner)
  }
  return scope
}

/** What breaks the touch rules in one stylesheet. */
function problems(source) {
  const found = []
  for (const block of blocks(source)) {
    const outer = ancestors(block)
    const syntax = selectorSyntax(block.prelude)
    const inMixin = outer.some(parent => HOVER_MIXIN.test(parent.prelude))
    const inUse = outer.some(parent => HOVER_USE.test(parent.prelude))
    if (/:hover\b/.test(syntax) && !inUse && !inMixin) {
      found.push(`${block.prelude}: :hover outside the ui.hover mixin`)
    }
    if (/^@media\b/.test(syntax) && HOVER_MEDIA.test(syntax) && !inMixin) {
      found.push(`${block.prelude}: a hover media query outside the ui.hover mixin`)
    }
    const scrolling = { x: false, y: false }
    for (const { property, value } of block.declarations) {
      for (const [axis, overflow] of Object.entries(axesOf(property, value, 'overflow'))) {
        if (SCROLLS.test(overflow)) scrolling[axis] = true
      }
    }
    const declared = { x: false, y: false }
    for (const scope of scopeOf(block)) {
      for (const { property, value } of scope.declarations) {
        for (const axis of Object.keys(axesOf(property, value, 'overscroll-behavior'))) {
          declared[axis] = true
        }
      }
    }
    for (const axis of ['x', 'y']) {
      if (scrolling[axis] && !declared[axis]) {
        found.push(`${block.prelude}: scrolls along ${axis} without overscroll-behavior for it`)
      }
    }
  }
  return found
}

/** The inline stylesheets of the components in one TypeScript source, as written. */
function inlineStyles(code, name = 'probe.ts') {
  const file = ts.createSourceFile(name, code, ts.ScriptTarget.Latest, true)
  const sheets = []
  const literal = node => {
    if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) {
      sheets.push(node.text)
    } else {
      throw new UnreadableStylesheet(`inline styles that are not plain text, line ${lineOf(node)}`)
    }
  }
  const lineOf = node => file.getLineAndCharacterOfPosition(node.getStart()).line + 1
  const visit = node => {
    if (
      ts.isPropertyAssignment(node) &&
      ts.isIdentifier(node.name) &&
      node.name.text === 'styles' &&
      ts.isObjectLiteralExpression(node.parent) &&
      ts.isCallExpression(node.parent.parent) &&
      ts.isDecorator(node.parent.parent.parent)
    ) {
      if (ts.isArrayLiteralExpression(node.initializer)) node.initializer.elements.forEach(literal)
      else literal(node.initializer)
    }
    ts.forEachChild(node, visit)
  }
  visit(file)
  return sheets
}

function* files(folder) {
  for (const entry of readdirSync(folder, { withFileTypes: true })) {
    const path = join(folder, entry.name)
    if (entry.isDirectory()) yield* files(path)
    else yield path
  }
}

/** Every stylesheet of the dashboard: SCSS and CSS files, and components' inline `styles`. */
function stylesheets() {
  const sheets = []
  for (const path of files(SOURCE)) {
    const name = relative(SOURCE, path)
    if (/\.(?:scss|css)$/.test(path)) {
      sheets.push({ name, read: () => readFileSync(path, 'utf8') })
    } else if (path.endsWith('.ts') && !path.endsWith('.spec.ts')) {
      const code = readFileSync(path, 'utf8')
      if (!/\bstyles\s*:/.test(code)) continue
      sheets.push({ name, read: () => inlineStyles(code, name).join('\n') })
    }
  }
  return sheets
}

/** What breaks the touch rules in every stylesheet, each named by its file. */
function sheetProblems(sheets) {
  return sheets.flatMap(({ name, read }) => {
    try {
      return problems(read()).map(problem => `${name}: ${problem}`)
    } catch (error) {
      if (error instanceof UnreadableStylesheet) return [`${name}: ${error.message}`]
      throw error
    }
  })
}

/** Sass as the dashboard's build compiles it (`@angular/build` brings it). */
function sass() {
  const build = createRequire(join(DASHBOARD, 'package.json')).resolve('@angular/build')
  return createRequire(build)(createRequire(build).resolve('sass', { paths: [dirname(build)] }))
}

test('the checker refuses what the rules refuse', () => {
  const refused = [
    '.a:hover { color: red; }',
    '.a { &:hover .b { color: red } }',
    '.a:not(:hover) { color: red; }',
    '.a:is(:hover) { color: red; }',
    '.a { @media (hover: hover) { &:hover { color: red } } }',
    '.a { overflow-y: auto; }',
    '.a { overflow: scroll }',
    // On an ancestor only, or on the wrong axis.
    '.a { overscroll-behavior: contain; .b { overflow-y: auto } }',
    '.a { overflow-y: auto; overscroll-behavior-x: contain }',
    '.a { overflow: auto; overscroll-behavior-y: contain }',
    '.a { overflow: hidden auto; overscroll-behavior-x: none }',
  ]
  for (const css of refused) assert.notDeepEqual(problems(css), [], css)
  assert.equal(problems('@media (hover: hover) { .a:hover { color: red } }').length, 2)
})

test('the checker allows what the rules allow', () => {
  const allowed = [
    '.a { @include ui.hover { color: red; } }',
    '.a { @include ui.hover($pressed: false) { .b { color: red } } }',
    '@mixin hover { @media (hover: hover) { &:hover { @content; } } }',
    '// .a:hover {\n.a { color: red }',
    ".a::after { content: ':hover {' }",
    '.a[data-tip=":hover"] { color: red }',
    '.a { overflow-y: auto; overscroll-behavior-y: contain }',
    '.a { overflow: auto; overscroll-behavior: contain auto }',
    '.a { overflow: hidden auto; overscroll-behavior-y: auto }',
    '.a { overflow-y: auto; overscroll-behavior: contain; @supports (x: y) { overflow-y: scroll } }',
    '.a { overscroll-behavior-y: contain; @include ui.down(phone) { overflow-y: auto } }',
    '.a { overflow: hidden; overflow-x: clip }',
  ]
  for (const css of allowed) assert.deepEqual(problems(css), [], css)
})

test('the checker names a stylesheet it cannot read, and does not hang on it', () => {
  for (const css of [
    '.a-#{ { color: red }',
    '.a { color: red',
    '.a { content: "x }',
    '/* x',
    '}',
  ]) {
    assert.deepEqual(
      sheetProblems([{ name: 'broken.scss', read: () => css }]).map(problem =>
        problem.startsWith('broken.scss: ')
      ),
      [true],
      css
    )
  }
})

test('the checker reads every inline stylesheet a component declares', () => {
  const component = `
    @Component({ selector: 'a', styles: [':host { overflow-y: auto }', \`.b:hover { color: red }\`] })
    export class A {}
    @Component({ selector: 'b', styles: ".c { overflow: scroll }" })
    export class B {}
    const notStyles = { styles: '.d:hover {}' }
  `
  assert.deepEqual(inlineStyles(component), [
    ':host { overflow-y: auto }',
    '.b:hover { color: red }',
    '.c { overflow: scroll }',
  ])
  assert.throws(
    () => inlineStyles("@Component({ styles: [base + ':host {}'] }) class C {}"),
    UnreadableStylesheet
  )
})

test('ui.hover compiles to a hover only where the pointer hovers, and to the pressed look', () => {
  const touchClass = /export const TOUCH_INPUT_CLASS = '([^']+)'/.exec(
    readFileSync(join(SOURCE, 'app', 'utils', 'touch-input.ts'), 'utf8')
  )?.[1]
  assert.ok(touchClass, 'utils/touch-input.ts names the class')
  const { css } = sass().compileString(
    `@use 'app/components/dashboard-shared' as ui;
     .a { @include ui.hover { color: red; } }
     .b { @include ui.hover($pressed: false) { color: blue; } }`,
    { loadPaths: [SOURCE] }
  )
  const compiled = blocks(css)
  const rule = selector => compiled.filter(block => block.prelude === selector)
  const guard = `:not(:root.${touchClass} *)`

  const [hover] = rule(`.a${guard}:hover`)
  assert.equal(hover?.parent.prelude, '@media (hover: hover)')
  assert.deepEqual(hover.declarations, [{ property: 'color', value: 'red' }])
  const [pressed] = rule('.a:active')
  assert.equal(pressed?.parent.prelude, '')
  assert.deepEqual(pressed.declarations, [
    { property: 'transition-duration', value: '0s' },
    { property: 'color', value: 'red' },
  ])
  assert.equal(rule(`.b${guard}:hover`)[0]?.parent.prelude, '@media (hover: hover)')
  assert.deepEqual(rule('.b:active'), [])
  // Nothing else shows either look.
  assert.deepEqual(
    compiled.filter(block => /:hover|:active/.test(block.prelude)).map(block => block.prelude),
    [`.a${guard}:hover`, '.a:active', `.b${guard}:hover`]
  )
})

test('the dashboard has stylesheets to check', () => {
  const names = stylesheets().map(sheet => sheet.name)
  assert.ok(names.includes('styles.scss'))
  assert.ok(names.includes(join('app', 'components', '_dashboard-shared.scss')))
  assert.ok(
    names.some(name => name.endsWith('.component.ts')),
    'inline component styles'
  )
})

test('no stylesheet of the dashboard breaks the touch rules', () => {
  assert.deepEqual(sheetProblems(stylesheets()), [])
})

test('the page itself neither bounces nor reloads when pulled', () => {
  const root = blocks(readFileSync(join(SOURCE, 'styles.scss'), 'utf8')).find(
    block =>
      block.parent?.prelude === '' &&
      block.prelude
        .split(',')
        .map(selector => selector.trim())
        .includes('html') &&
      block.declarations.some(
        ({ property, value }) => property === 'overscroll-behavior' && value === 'none'
      )
  )
  assert.ok(root, 'html has overscroll-behavior: none')
})

test('the viewport covers the screen and keeps pinch zoom', () => {
  const html = readFileSync(join(SOURCE, 'index.html'), 'utf8')
  const viewport = /<meta\s+name="viewport"\s+content="([^"]*)"/.exec(html)?.[1]
  assert.ok(viewport, 'a viewport meta')
  const parts = new Map(
    viewport.split(',').map(part => part.split('=').map(side => side.trim().toLowerCase()))
  )
  assert.equal(parts.get('width'), 'device-width')
  assert.equal(parts.get('viewport-fit'), 'cover')
  assert.equal(parts.get('user-scalable'), undefined)
  assert.equal(parts.get('maximum-scale'), undefined)
})
