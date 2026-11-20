// The dashboard has one key listener for app shortcuts, in `services/shortcut.service.ts`
// (`models/shortcuts.ts` is the table). Anywhere else a key listener on the document or the
// window is refused, in TypeScript and in templates (inline ones too):
// - `(document:keydown…)` / `(window:keyup…)` host bindings and template events;
// - `@HostListener('document:keydown…')` / `('window:…')`;
// - Signality's `onKey(…)` without a `target` (it listens on the window then) or with the
//   document or window as its target;
// - `listener(document, 'keydown', …)` and `document.addEventListener('keydown', …)`;
// each also through a name the file gives the document or the window (`const doc = document`,
// a field set to `inject(DOCUMENT)`). A target passed in from elsewhere is not followed.
// A widget's keys on its own element (`(keydown)` on the host or an element) stay where they are.
// Run with `node --test scripts/`.
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { join, posix, relative, win32 } from 'node:path'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'
import ts from 'typescript'

const APP = fileURLToPath(new URL('../dashboard/src/app/', import.meta.url))
/** The one file that listens, as segments under `APP`. */
const OWNER = ['services', 'shortcut.service.ts']

/** Whether `file` is the shortcut service, both built with one path API (`win32` on Windows). */
export function isOwner(file, app = APP, paths = { join }) {
  return file === paths.join(app, ...OWNER)
}

const KEY_EVENT = /^key(?:down|up|press)\b/i
const GLOBAL_KEY_BINDING = /\((?:document|window):key(?:down|up|press)\b/i
const GLOBAL_KEY_NAME = /^(?:document|window):key(?:down|up|press)\b/i

/**
 * An expression that names the document or the window: by its own name, or by a name it was
 * given in the file (`aliases`: a variable or a class field set to one, `this.field` too).
 */
function isGlobalTarget(node, aliases = new Set()) {
  if (ts.isParenthesizedExpression(node) || ts.isNonNullExpression(node)) {
    return isGlobalTarget(node.expression, aliases)
  }
  if (ts.isBinaryExpression(node)) {
    return isGlobalTarget(node.left, aliases) || isGlobalTarget(node.right, aliases)
  }
  if (ts.isIdentifier(node)) {
    return /^(?:document|window|globalThis)$/.test(node.text) || aliases.has(node.text)
  }
  if (ts.isPropertyAccessExpression(node)) {
    const name = node.name.text
    return (
      /^(?:document|window|defaultView|ownerDocument)$/.test(name) ||
      (node.expression.kind === ts.SyntaxKind.ThisKeyword && aliases.has(name))
    )
  }
  if (ts.isCallExpression(node)) {
    return node.expression.getText() === 'inject' && node.arguments[0]?.getText() === 'DOCUMENT'
  }
  return false
}

/** The name a call is made by: `onKey`, `onKey.capture` read as `onKey`, `x.addEventListener`. */
function callName(call) {
  let callee = call.expression
  while (
    ts.isPropertyAccessExpression(callee) &&
    /^(?:capture|passive|once|stop|prevent|self)$/.test(callee.name.text)
  ) {
    callee = callee.expression
  }
  if (ts.isIdentifier(callee)) return { name: callee.text, object: null }
  if (ts.isPropertyAccessExpression(callee))
    return { name: callee.name.text, object: callee.expression }
  return { name: null, object: null }
}

/** The names a file gives the document or the window, followed through names given to those. */
function aliasesIn(file) {
  const aliases = new Set()
  let grew = true
  while (grew) {
    grew = false
    const visit = node => {
      const named =
        (ts.isVariableDeclaration(node) || ts.isPropertyDeclaration(node)) &&
        node.initializer !== undefined &&
        ts.isIdentifier(node.name)
      if (named && !aliases.has(node.name.text) && isGlobalTarget(node.initializer, aliases)) {
        aliases.add(node.name.text)
        grew = true
      }
      ts.forEachChild(node, visit)
    }
    visit(file)
  }
  return aliases
}

const stringOf = node => (node && ts.isStringLiteralLike(node) ? node.text : null)

/** The global key listeners `source` (TypeScript) adds, as `line: what`. */
export function globalKeyListenersInScript(source, fileName = 'file.ts') {
  const file = ts.createSourceFile(fileName, source, ts.ScriptTarget.Latest, true)
  const aliases = aliasesIn(file)
  const found = []
  const report = (node, what) => {
    const line = file.getLineAndCharacterOfPosition(node.getStart()).line + 1
    found.push(`${line}: ${what}`)
  }

  const visit = node => {
    if (ts.isPropertyAssignment(node)) {
      const key = stringOf(node.name)
      if (key !== null && GLOBAL_KEY_BINDING.test(key)) report(node, key)
    }
    // An inline template.
    if (
      ts.isStringLiteralLike(node) &&
      ts.isPropertyAssignment(node.parent) &&
      node.parent.initializer === node &&
      node.parent.name.getText() === 'template' &&
      GLOBAL_KEY_BINDING.test(node.text)
    ) {
      report(node, 'template')
    }
    if (ts.isDecorator(node) && ts.isCallExpression(node.expression)) {
      const name = node.expression.expression.getText()
      const event = stringOf(node.expression.arguments[0])
      if (name === 'HostListener' && event !== null && GLOBAL_KEY_NAME.test(event)) {
        report(node, `@HostListener('${event}')`)
      }
    }
    if (ts.isCallExpression(node)) {
      const { name, object } = callName(node)
      const args = node.arguments
      if (name === 'onKey') {
        const options = args.find(arg => ts.isObjectLiteralExpression(arg))
        const target = options?.properties.find(
          property => ts.isPropertyAssignment(property) && property.name.getText() === 'target'
        )
        if (target === undefined || isGlobalTarget(target.initializer, aliases)) {
          report(
            node,
            target === undefined
              ? 'onKey on the window'
              : `onKey on ${target.initializer.getText()}`
          )
        }
      }
      if (name === 'listener' && args.length >= 2) {
        const event = stringOf(args[1])
        if (event !== null && KEY_EVENT.test(event) && isGlobalTarget(args[0], aliases)) {
          report(node, `listener(${args[0].getText()}, '${event}')`)
        }
      }
      if (name === 'addEventListener' && object !== null) {
        const event = stringOf(args[0])
        if (event !== null && KEY_EVENT.test(event) && isGlobalTarget(object, aliases)) {
          report(node, `${object.getText()}.addEventListener('${event}')`)
        }
      }
    }
    ts.forEachChild(node, visit)
  }
  visit(file)
  return found
}

/** The global key bindings a template holds, as `line: what`. */
export function globalKeyListenersInTemplate(source) {
  return source
    .split('\n')
    .flatMap((line, index) =>
      GLOBAL_KEY_BINDING.test(line) ? [`${index + 1}: ${line.trim()}`] : []
    )
}

function* sources(folder) {
  for (const entry of readdirSync(folder, { withFileTypes: true })) {
    const path = join(folder, entry.name)
    if (entry.isDirectory()) {
      if (entry.name !== 'generated' && entry.name !== 'testing') yield* sources(path)
    } else if (
      /\.html$/.test(entry.name) ||
      (/\.ts$/.test(entry.name) && !/\.spec\.ts$/.test(entry.name))
    ) {
      yield path
    }
  }
}

test('the checker finds the handlers the registry replaced', () => {
  assert.deepEqual(
    globalKeyListenersInScript(`
      @Component({ host: { '(document:keydown.escape)': 'onEscape($event)' } })
      class A {
        constructor() {
          onKey('/', event => this.focus(event), { target: document })
          onKey.prevent('k', () => {})
          listener(this.document, 'keydown', () => {})
          window.addEventListener('keyup', () => {})
          listener(inject(DOCUMENT), 'keydown', () => {})
        }
        @HostListener('window:keydown', ['$event']) onKey(event) {}
      }
    `),
    [
      '2: (document:keydown.escape)',
      '5: onKey on document',
      '6: onKey on the window',
      "7: listener(this.document, 'keydown')",
      "8: window.addEventListener('keyup')",
      "9: listener(inject(DOCUMENT), 'keydown')",
      "11: @HostListener('window:keydown')",
    ]
  )
  assert.deepEqual(
    globalKeyListenersInScript(
      '@Component({ template: \'<div (window:keyup)="x()"></div>\' }) class B {}'
    ),
    ['1: template']
  )
  assert.deepEqual(
    globalKeyListenersInTemplate('<p>\n  <i (document:keydown.enter)="go()"></i>\n</p>'),
    ['2: <i (document:keydown.enter)="go()"></i>']
  )
})

test('the checker follows the document or the window through a name given to it', () => {
  assert.deepEqual(
    globalKeyListenersInScript(`
      const doc = document
      const view = doc.defaultView
      class C {
        private readonly host = inject(DOCUMENT)
        constructor() {
          doc.addEventListener('keydown', () => {})
          listener(view, 'keyup', () => {})
          onKey('x', () => {}, { target: this.host })
          const same = this.host
          same.addEventListener('keydown', () => {})
        }
      }
    `),
    [
      "7: doc.addEventListener('keydown')",
      "8: listener(view, 'keyup')",
      '9: onKey on this.host',
      "11: same.addEventListener('keydown')",
    ]
  )
})

test('the checker knows the shortcut service by its path on Windows too', () => {
  const app = 'C:\\repo\\dashboard\\src\\app'
  assert.equal(isOwner(win32.join(app, 'services', 'shortcut.service.ts'), app, win32), true)
  assert.equal(isOwner(win32.join(app, 'services', 'other.service.ts'), app, win32), false)
  assert.equal(
    isOwner(posix.join('/repo/app', 'services', 'shortcut.service.ts'), '/repo/app', posix),
    true
  )
})

test('the checker leaves a widget its own keys', () => {
  assert.deepEqual(
    globalKeyListenersInScript(`
      @Component({ host: { '(keydown)': 'onKeydown($event)' }, template: '<ol (keydown.arrowdown)="next()"></ol>' })
      class Widget {
        constructor() {
          onKey('Enter', () => {}, { target: this.list })
          listener(document, 'contextmenu', () => {})
          listener(this.element, 'keydown', () => {})
          this.element.addEventListener('keydown', () => {})
        }
      }
    `),
    []
  )
  assert.deepEqual(globalKeyListenersInTemplate('<input (keydown.escape)="clear()" />'), [])
})

test('no key listener on the document or the window outside the shortcut service', () => {
  const found = []
  let checked = 0
  for (const path of sources(APP)) {
    if (isOwner(path)) continue
    const name = relative(APP, path)
    checked++
    const source = readFileSync(path, 'utf8')
    const listeners = path.endsWith('.html')
      ? globalKeyListenersInTemplate(source)
      : globalKeyListenersInScript(source, path)
    found.push(...listeners.map(where => `${name}:${where}`))
  }
  assert.ok(checked > 50, `only ${checked} files checked`)
  assert.deepEqual(
    found,
    [],
    'bind an entry of models/shortcuts.ts through ShortcutService instead'
  )
})

test('the shortcut service is the one that listens', () => {
  const source = readFileSync(join(APP, ...OWNER), 'utf8')
  assert.equal(globalKeyListenersInScript(source).length, 1)
})
