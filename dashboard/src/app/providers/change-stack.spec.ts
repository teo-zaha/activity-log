import { describe, expect, it } from 'vitest'
import type { ChangeStack, ChangeStackAbove, StackCut, StackedChange } from '../generated/activity'
import { changeStackVmOf } from './change-stack'

const CUTS: readonly StackCut[] = ['limit', 'unreadable', 'throttled', 'failed', 'cycle']

const change = (number: number, webUrl?: string): StackedChange => ({
  number,
  title: `Change ${number}`,
  webUrl,
})
const complete: ChangeStackAbove = { changes: [], end: { kind: 'complete' } }

const stack: ChangeStack = {
  current: change(12, 'https://gitlab.example.org/group/app/-/merge_requests/12'),
  below: [change(10, 'https://gitlab.example.org/group/app/-/merge_requests/10'), change(11)],
  bottom: { kind: 'reached', branch: 'main' },
  above: complete,
}

describe('changeStackVmOf', () => {
  it('lists the chain from the base up, the entry’s own change last and unlinked', () => {
    const vm = changeStackVmOf(stack, '!')

    expect(vm.base).toEqual({ kind: 'branch', branch: 'main' })
    expect(vm.chain).toEqual([
      {
        kind: 'member',
        reference: '!10',
        title: 'Change 10',
        href: 'https://gitlab.example.org/group/app/-/merge_requests/10',
      },
      { kind: 'member', reference: '!11', title: 'Change 11', href: null },
      { kind: 'current', reference: '!12', title: 'Change 12' },
    ])
    expect(vm.above).toEqual({ depth: 1, nodes: [], end: { kind: 'complete' } })
  })

  it('writes numbers with the host’s prefix', () => {
    const vm = changeStackVmOf(stack, '#')

    expect(vm.chain.map(row => row.reference)).toEqual(['#10', '#11', '#12'])
  })

  it('shows the base change alone in the chain', () => {
    const vm = changeStackVmOf({ ...stack, below: [], bottom: { kind: 'ambiguous' } }, '!')

    expect(vm.chain).toEqual([{ kind: 'current', reference: '!12', title: 'Change 12' }])
  })

  it('tells several changes below apart from a cut', () => {
    const vm = changeStackVmOf({ ...stack, bottom: { kind: 'ambiguous' } }, '!')

    expect(vm.base.kind).toBe('ambiguous')
  })

  it.each(CUTS)('words a bottom cut for %s by its reason', reason => {
    const vm = changeStackVmOf({ ...stack, bottom: { kind: 'cut', reason } }, '!')

    expect(vm.base).toMatchObject({ kind: 'cut', reason })
  })

  it.each(CUTS)('words a level cut for %s by its reason', reason => {
    const vm = changeStackVmOf(
      { ...stack, above: { changes: [], end: { kind: 'cut', reason } } },
      '!'
    )

    expect(vm.above.end).toMatchObject({ kind: 'cut', reason })
  })

  it('gives every cut its own words, below and above apart', () => {
    const texts = CUTS.flatMap(reason => {
      const vm = changeStackVmOf(
        {
          ...stack,
          bottom: { kind: 'cut', reason },
          above: { changes: [], end: { kind: 'cut', reason } },
        },
        '!'
      )
      return [vm.base, vm.above.end].map(line => (line.kind === 'cut' ? line.text : ''))
    })
    const ambiguous = changeStackVmOf({ ...stack, bottom: { kind: 'ambiguous' } }, '!').base

    expect(ambiguous.kind === 'ambiguous' && ambiguous.text).toBeTruthy()
    expect(texts.every(text => text.length > 0)).toBe(true)
    expect(new Set(texts).size).toBe(CUTS.length * 2)
  })

  it('nests the tree above, each level with its depth and how it ends', () => {
    const vm = changeStackVmOf(
      {
        ...stack,
        above: {
          changes: [
            {
              change: change(13, 'https://gitlab.example.org/group/app/-/merge_requests/13'),
              above: {
                changes: [{ change: change(15), above: complete }],
                end: { kind: 'cut', reason: 'cycle' },
              },
            },
            { change: change(14), above: complete },
          ],
          end: { kind: 'cut', reason: 'limit' },
        },
      },
      '!'
    )

    expect(vm.above.depth).toBe(1)
    expect(vm.above.nodes.map(node => node.change.reference)).toEqual(['!13', '!14'])
    expect(vm.above.end).toMatchObject({ kind: 'cut', reason: 'limit' })
    const [first, second] = vm.above.nodes
    expect(first?.change).toEqual({
      kind: 'member',
      reference: '!13',
      title: 'Change 13',
      href: 'https://gitlab.example.org/group/app/-/merge_requests/13',
    })
    expect(first?.above.depth).toBe(2)
    expect(first?.above.nodes.map(node => node.change.reference)).toEqual(['!15'])
    expect(first?.above.end).toMatchObject({ kind: 'cut', reason: 'cycle' })
    expect(first?.above.nodes[0]?.above).toEqual({ depth: 3, nodes: [], end: { kind: 'complete' } })
    expect(second?.above.end).toEqual({ kind: 'complete' })
  })
})
