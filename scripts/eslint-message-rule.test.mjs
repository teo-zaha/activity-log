// What the dashboard's message rule (`eslint.config.mjs`, `messageDecisions`) refuses and
// allows. Run with `node --test scripts/`.
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { ESLint } from 'eslint'

const eslint = new ESLint()

async function refused(code) {
  const [result] = await eslint.lintText(
    `export const probe = (e: Error, x: unknown): unknown => ${code}\n`,
    {
      filePath: 'dashboard/src/app/probe.ts',
    }
  )
  return result.messages.filter(message => message.ruleId === 'no-restricted-syntax').length
}

const decisions = [
  "e.message === 'x'",
  "e.message != 'x'",
  "'x' == e.message",
  "(e as Error | undefined)?.message === 'x'",
  "e['message'] === 'x'",
  '/busy/.test(e.message)',
  '/busy/.exec(e.message)',
  "e.message.includes('busy')",
  "e.message.startsWith('busy')",
  "e.message?.endsWith('busy')",
  "(e as Error | undefined)?.message.indexOf('busy')",
  "e['message'].search(/busy/)",
  "(() => { switch (e.message) { case 'x': return 1 } return 0 })()",
  "(() => { switch ((e as Error | undefined)?.message) { case 'x': return 1 } return 0 })()",
  "['busy'].includes(e.message)",
  "['busy'].indexOf((e as Error | undefined)?.message)",
]

const presentation = [
  "'Export failed: ' + e.message",
  '`Export failed: ${e.message}`',
  "[e.message, 'x'].join(' · ')",
  'e.message.trim()',
  'e.message.length > 0',
  "x === 'message'",
]

for (const code of decisions) {
  test(`refuses ${code}`, async () => assert.equal(await refused(code), 1))
}

for (const code of presentation) {
  test(`allows ${code}`, async () => assert.equal(await refused(code), 0))
}
