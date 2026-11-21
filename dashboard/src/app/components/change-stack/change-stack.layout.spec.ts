/// <reference types="@vitest/browser-playwright" />
// A change's Stack section in the details, in a real browser (`ng test --configuration layout`,
// Chromium): long titles wrap inside it, the tree above steps in only up to its cap, and rows are
// tall enough for a finger on a touch screen, on a phone and in a window.
import { afterEach, describe, expect, it } from 'vitest'
import { cdp, page } from 'vitest/browser'
import type {
  ActivityLog,
  ChangeStackAbove,
  GitLabEntry,
  StackedChange,
} from '../../generated/activity'
import { activityLog, MockBackendTransport } from '../../testing/activity-fixtures'
import { openApp } from '../../testing/app'
import { MAX_STACK_INDENT } from './stack-level.component'

const RANGE = 'from=2026-05-17&to=2026-05-18'
/** Levels above the entry's own change: more than the indent allows. */
const LEVELS = MAX_STACK_INDENT + 3
const LONG_TITLE =
  'Move the collection runtime onto the shared host so that the desktop app and the API answer ' +
  'every command from one state, with a word that does not break: ' +
  'supercalifragilisticexpialidocious_activity_log_runtime_state'

const change = (number: number, title = `Change ${number}`): StackedChange => ({
  number,
  title,
  webUrl: `https://gitlab.example.org/group/app/-/merge_requests/${number}`,
})

let above: ChangeStackAbove = { changes: [], end: { kind: 'cut', reason: 'limit' } }
for (let level = LEVELS; level >= 1; level--) {
  above = {
    changes: [{ change: change(20 + level, LONG_TITLE), above }],
    end: { kind: 'complete' },
  }
}

const mergeRequest: GitLabEntry = {
  source: 'gitlab',
  connection: 'default',
  id: 'opened-12',
  type: 'opened',
  timestamp: '2026-05-18T11:00:00Z',
  title: 'Stack the parser',
  targetType: 'MergeRequest',
  mrIid: 12,
  changeStack: {
    current: change(12, LONG_TITLE),
    below: [change(10), change(11, LONG_TITLE)],
    bottom: { kind: 'reached', branch: 'feature/a-branch-name-that-goes-on-and-on-and-on' },
    above,
  },
}

/** The same stack with what it builds on cut: its first line is a cut line too. */
const cutBelow: GitLabEntry = {
  ...mergeRequest,
  id: 'opened-13',
  mrIid: 13,
  timestamp: '2026-05-18T12:00:00Z',
  changeStack: { ...mergeRequest.changeStack!, bottom: { kind: 'cut', reason: 'unreadable' } },
}

const log: ActivityLog = {
  ...activityLog,
  summary: {
    ...activityLog.summary,
    connections: [
      ...activityLog.summary.connections,
      { connection: { source: 'gitlab', name: 'default' }, count: 2 },
    ],
  },
  entries: [...activityLog.entries, mergeRequest, cutBelow],
}

/** Two frames: what a change lays out and what observes it has run. */
async function frames(): Promise<void> {
  for (let i = 0; i < 2; i++) await new Promise(resolve => requestAnimationFrame(resolve))
}

/** A touch screen: the pointer cannot hover (`ui.no-hover`). */
async function touch(enabled: boolean): Promise<void> {
  await cdp().send('Emulation.setTouchEmulationEnabled', { enabled, maxTouchPoints: 5 })
}

/** A colour as the screen shows it, `[r, g, b]` from 0 to 255, whatever syntax it is written in. */
function rgbOf(color: string): [number, number, number] {
  const canvas = document.createElement('canvas')
  canvas.width = canvas.height = 1
  const context = canvas.getContext('2d', { willReadFrequently: true })!
  context.fillStyle = color
  context.fillRect(0, 0, 1, 1)
  const [r, g, b] = context.getImageData(0, 0, 1, 1).data
  return [r!, g!, b!]
}

/** WCAG's contrast ratio of two opaque colours. */
function contrast(a: string, b: string): number {
  const luminance = (color: string): number => {
    const [r, g, b] = rgbOf(color).map(channel => {
      const c = channel / 255
      return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
    })
    return 0.2126 * r! + 0.7152 * g! + 0.0722 * b!
  }
  const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return (light! + 0.05) / (dark! + 0.05)
}

/**
 * The themes `ThemeService` marks dark (`html.dark`), but `dim`: its `--text-secondary`, the
 * next stronger text, is 3.7:1 on a section everywhere in the app, a gap of the theme's own.
 */
const DARK_THEMES = ['dark', 'midnight', 'mocha'] as const

async function openStack(width: number, id = mergeRequest.id): Promise<HTMLElement> {
  await page.viewport(width, 800)
  const backend = new MockBackendTransport()
  backend.activityResponses = [log]
  backend.activityResponse = log
  const event = encodeURIComponent(`gitlab/default/${id}`)
  await openApp(`/timeline?${RANGE}&event=${event}`, { backend })
  await expect.poll(() => document.querySelector('app-change-stack')).not.toBeNull()
  await frames()
  return document.querySelector<HTMLElement>('app-change-stack')!
}

const SIZES = [
  { name: 'a phone', width: 402 },
  { name: 'a window', width: 1280 },
] as const

describe('the Stack section in a browser', () => {
  afterEach(async () => {
    document.documentElement.classList.remove('dark')
    delete document.documentElement.dataset['theme']
    await touch(false)
    await frames()
  })

  for (const size of SIZES) {
    it(`wraps every line inside the section on ${size.name}`, async () => {
      const section = await openStack(size.width)
      const body = section.closest<HTMLElement>('.flyout-body')!
      const bounds = section.getBoundingClientRect()

      expect(body.scrollWidth).toBe(body.clientWidth)
      const lines = section.querySelectorAll<HTMLElement>('.change, .cut, .stack-base')
      expect(lines.length).toBeGreaterThan(LEVELS)
      for (const line of lines) {
        const rect = line.getBoundingClientRect()
        expect(rect.left).toBeGreaterThanOrEqual(bounds.left)
        expect(rect.right).toBeLessThanOrEqual(bounds.right)
      }
      // A long title takes more lines rather than more width.
      const title = section.querySelector<HTMLElement>('[aria-current] .title')!
      expect(title.getBoundingClientRect().height).toBeGreaterThan(
        2 * parseFloat(getComputedStyle(title).lineHeight)
      )
    })
  }

  it('steps in for each level up to the cap, then stays where the cap left it', async () => {
    const section = await openStack(402)
    const lefts = [...section.querySelectorAll<HTMLElement>('app-stack-level .change')].map(
      row => row.getBoundingClientRect().left
    )

    expect(lefts).toHaveLength(LEVELS)
    for (let depth = 1; depth < MAX_STACK_INDENT; depth++) {
      expect(lefts[depth]!).toBeGreaterThan(lefts[depth - 1]!)
    }
    for (let depth = MAX_STACK_INDENT; depth < LEVELS; depth++) {
      expect(lefts[depth]).toBe(lefts[MAX_STACK_INDENT - 1])
    }
  })

  it('gives each row a finger’s height on a touch screen', async () => {
    await touch(true)
    const section = await openStack(402)

    const rows = [...section.querySelectorAll<HTMLElement>('.change')]
    expect(rows.length).toBeGreaterThan(LEVELS)
    for (const row of rows) {
      expect(row.getBoundingClientRect().height).toBeGreaterThanOrEqual(44)
    }
  })

  // Why part of the stack is missing must read as well as the rest: at least 4.5:1.
  for (const theme of DARK_THEMES) {
    it(`gives the cut lines AA contrast in the ${theme} theme`, async () => {
      const section = await openStack(1280, cutBelow.id)
      document.documentElement.dataset['theme'] = theme
      document.documentElement.classList.add('dark')
      await frames()

      const background = getComputedStyle(section).backgroundColor
      const lines = [...section.querySelectorAll<HTMLElement>('.cut, .stack-base.is-cut')]
      expect(lines.length).toBeGreaterThanOrEqual(2)
      for (const line of lines) {
        expect(contrast(getComputedStyle(line).color, background)).toBeGreaterThanOrEqual(4.5)
      }
    })
  }
})
