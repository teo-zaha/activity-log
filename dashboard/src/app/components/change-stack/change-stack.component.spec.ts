import { fireEvent, render, screen, within } from '@testing-library/angular'
import { describe, expect, it } from 'vitest'
import type {
  ChangeStack,
  ChangeStackAbove,
  ChangeStackNode,
  StackedChange,
} from '../../generated/activity'
import { changeStackVmOf, type ChangeStackVm } from '../../providers/change-stack'
import { MockBackendTransport, provideMockBackend } from '../../testing/activity-fixtures'
import { ChangeStackComponent, type StackHeadingLevel } from './change-stack.component'
import { MAX_STACK_INDENT } from './stack-level.component'

const url = (number: number): string =>
  `https://gitlab.example.org/group/app/-/merge_requests/${number}`
const change = (number: number, linked = true): StackedChange => ({
  number,
  title: `Change ${number}`,
  webUrl: linked ? url(number) : undefined,
})
const complete: ChangeStackAbove = { changes: [], end: { kind: 'complete' } }
const node = (number: number, above: ChangeStackAbove = complete): ChangeStackNode => ({
  change: change(number),
  above,
})

const stack: ChangeStack = {
  current: change(12),
  below: [change(10), change(11, false)],
  bottom: { kind: 'reached', branch: 'main' },
  above: {
    changes: [node(13, { changes: [node(15)], end: { kind: 'cut', reason: 'cycle' } }), node(14)],
    end: { kind: 'cut', reason: 'limit' },
  },
}

async function renderStack(
  vm: ChangeStackVm,
  inputs: { caption?: string; headingLevel?: StackHeadingLevel } = {}
): Promise<MockBackendTransport> {
  const backend = new MockBackendTransport()
  await render(ChangeStackComponent, {
    inputs: { stack: vm, ...inputs },
    providers: [provideMockBackend(backend)],
  })
  return backend
}

/** The list items directly in `list`, not those of the lists nested in them. */
const itemsOf = (list: HTMLElement): HTMLElement[] =>
  within(list)
    .getAllByRole('listitem')
    .filter(item => item.closest('ol') === list)

describe('ChangeStackComponent', () => {
  it('is a section titled Stack, as of the last collection', async () => {
    await renderStack(changeStackVmOf(stack, '!'))

    const section = screen.getByRole('region', { name: 'Stack' })
    expect(within(section).getByRole('heading', { name: 'Stack' })).toBeTruthy()
    expect(within(section).getByRole('heading', { level: 3 })).toBeTruthy()
    expect(section.textContent).toContain('As of the last collection')
  })

  it('takes how fresh the stack is and its heading’s level from where it shows', async () => {
    await renderStack(changeStackVmOf(stack, '!'), {
      caption: 'Looked up as it opened',
      headingLevel: 4,
    })

    const section = screen.getByRole('region', { name: 'Stack' })
    expect(within(section).getByRole('heading', { name: 'Stack', level: 4 })).toBeTruthy()
    expect(section.textContent).toContain('Looked up as it opened')
    expect(section.textContent).not.toContain('As of the last collection')
  })

  // WebKit presents a list without markers as no list: VoiceOver would read the tree flat.
  it('keeps every list a list, also without its markers', async () => {
    await renderStack(changeStackVmOf(stack, '!'))

    const lists = [...document.querySelectorAll('ol')]
    expect(lists).toHaveLength(3)
    expect(lists.map(list => list.getAttribute('role'))).toEqual(['list', 'list', 'list'])
  })

  it('lists the chain from the branch it stands on up, the entry’s own change marked', async () => {
    await renderStack(changeStackVmOf(stack, '!'))

    const section = screen.getByRole('region', { name: 'Stack' })
    const base = section.querySelector('.stack-base')
    expect(base?.textContent?.replace(/\s+/g, ' ').trim()).toBe('Builds on main')
    expect(base?.querySelector('code')?.textContent).toBe('main')
    const chain = within(section).getAllByRole('list')[0]!
    const rows = itemsOf(chain)
    expect(rows.map(row => row.querySelector('app-stack-change')?.textContent)).toEqual([
      '!10 Change 10',
      '!11 Change 11',
      '!12 Change 12 This change',
    ])
    // The row itself is current, not the item that also holds the changes built on it.
    expect(rows.map(row => row.getAttribute('aria-current'))).toEqual([null, null, null])
    expect(
      rows.map(row => row.querySelector('.change')?.getAttribute('aria-current') ?? null)
    ).toEqual([null, null, 'true'])
  })

  it('links every change to its page outside the app, the entry’s own too, but one without a page', async () => {
    const backend = await renderStack(changeStackVmOf(stack, '!'))

    const links = screen.getAllByRole('link')
    expect(links.map(link => link.textContent)).toEqual([
      '!10 Change 10',
      '!12 Change 12 This change',
      '!13 Change 13',
      '!15 Change 15',
      '!14 Change 14',
    ])
    expect(links.every(link => link.getAttribute('target') === '_blank')).toBe(true)
    expect(links.map(link => link.getAttribute('aria-current'))).toEqual([
      null,
      'true',
      null,
      null,
      null,
    ])
    fireEvent.click(links[1]!)
    fireEvent.click(links[2]!)
    expect(backend.openedUrls).toEqual([url(12), url(13)])
  })

  it('nests the tree above inside the entry’s own change, each level ending in its cut', async () => {
    const vm = changeStackVmOf(stack, '!')
    await renderStack(vm)

    const current = document.querySelector('[aria-current="true"]')!.closest('li')!
    const [above] = within(current).getAllByRole('list')
    const levelOne = itemsOf(above!)
    expect(levelOne.map(item => within(item).getAllByRole('link')[0]?.textContent)).toEqual([
      '!13 Change 13',
      '!14 Change 14',
    ])
    const levelTwo = within(levelOne[0]!).getByRole('list')
    expect(itemsOf(levelTwo)).toHaveLength(1)

    const cuts = [...current.querySelectorAll('.cut')].map(cut => cut.textContent?.trim())
    const ends = [vm.above.nodes[0]!.above.end, vm.above.end]
    expect(cuts).toEqual(ends.map(end => (end.kind === 'cut' ? end.text : null)))
    // The level's cut is its last line, after its list, inside the change it builds on.
    expect(levelOne[0]!.querySelector('.cut')?.textContent?.trim()).toBe(cuts[0])
  })

  it('says what the stack stands on when it is not a branch', async () => {
    const vm = changeStackVmOf({ ...stack, bottom: { kind: 'cut', reason: 'unreadable' } }, '!')
    await renderStack(vm)

    const base = screen.getByRole('region', { name: 'Stack' }).querySelector('.stack-base')
    expect(base?.querySelector('code')).toBeNull()
    expect(base?.textContent?.trim()).toBe(vm.base.kind === 'cut' ? vm.base.text : null)
  })

  it('shows nothing above a change that has nothing above it', async () => {
    await renderStack(changeStackVmOf({ ...stack, above: complete }, '!'))

    expect(screen.getAllByRole('list')).toHaveLength(1)
    expect(document.querySelector('.cut')).toBeNull()
  })

  it('steps in for each level up to the cap, then nests without moving', async () => {
    let above: ChangeStackAbove = complete
    for (let number = 30; number > 20; number--) {
      above = { changes: [node(number, above)], end: { kind: 'complete' } }
    }
    await renderStack(changeStackVmOf({ ...stack, above }, '!'))

    const levels = [...document.querySelectorAll('app-stack-level')]
    expect(levels.map(level => level.classList.contains('is-indented'))).toEqual(
      levels.map((_, index) => index + 1 <= MAX_STACK_INDENT)
    )
    expect(levels.filter(level => level.classList.contains('is-indented'))).toHaveLength(
      MAX_STACK_INDENT
    )
  })
})
