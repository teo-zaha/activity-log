import type {
  ChangeStack,
  ChangeStackAbove,
  ChangeStackBottom,
  ChangeStackEnd,
  StackCut,
  StackedChange,
} from '../generated/activity'

/** How a host writes a change's number: `!12` for a GitLab merge request, `#12` on GitHub. */
export type ChangeReferencePrefix = '!' | '#'

/** A change of a stack, as its row shows it: `!12 Title`. */
export type StackChangeVm =
  /** The entry's own change: marked, and no link, since it is the page on screen. */
  | { readonly kind: 'current'; readonly reference: string; readonly title: string }
  /** Another change, linked to its page where the host gave a safe one. */
  | {
      readonly kind: 'member'
      readonly reference: string
      readonly title: string
      readonly href: string | null
    }

/** The line under the lowest change: what the stack stands on. */
export type StackBaseVm =
  | { readonly kind: 'branch'; readonly branch: string }
  | { readonly kind: 'ambiguous'; readonly text: string }
  | { readonly kind: 'cut'; readonly reason: StackCut; readonly text: string }

/** How a level above a change ends: all of it read, or cut, which its last line says. */
export type StackEndVm =
  | { readonly kind: 'complete' }
  | { readonly kind: 'cut'; readonly reason: StackCut; readonly text: string }

/** The changes that build on one change, each with what builds on it in turn. */
export interface StackLevelVm {
  /** 1 for the changes on the entry's own, 2 for those on them, and so on. */
  readonly depth: number
  readonly nodes: readonly StackNodeVm[]
  readonly end: StackEndVm
}

export interface StackNodeVm {
  readonly change: StackChangeVm
  readonly above: StackLevelVm
}

/**
 * A change stack laid out for the Stack section, from its base up: what it stands on, the
 * changes below, the entry's own, then the tree of changes above it. The backend computes the
 * stack; this only flattens it for the template and words each kind.
 */
export interface ChangeStackVm {
  readonly base: StackBaseVm
  /** The changes below, from the base up, then the entry's own. */
  readonly chain: readonly StackChangeVm[]
  readonly above: StackLevelVm
}

const BELOW_CUT: Record<StackCut, string> = {
  limit: 'Changes below not read: the lookup reached its limit',
  unreadable: 'Changes below cannot be read with this access',
  throttled: 'Changes below not read: the server asked to slow down',
  failed: 'Changes below not read: the request failed',
  cycle: 'Its branches lead back into this stack',
}

const ABOVE_CUT: Record<StackCut, string> = {
  limit: 'More changes above not read: the lookup reached its limit',
  unreadable: 'Changes above cannot be read with this access',
  throttled: 'Changes above not read: the server asked to slow down',
  failed: 'Changes above not read: the request failed',
  cycle: 'A branch leads back into this stack',
}

const AMBIGUOUS = 'On a branch that several changes come from'

export function changeStackVmOf(stack: ChangeStack, prefix: ChangeReferencePrefix): ChangeStackVm {
  const member = (change: StackedChange): StackChangeVm => ({
    kind: 'member',
    reference: `${prefix}${change.number}`,
    title: change.title,
    href: change.webUrl ?? null,
  })
  const levelOf = (above: ChangeStackAbove, depth: number): StackLevelVm => ({
    depth,
    nodes: above.changes.map(node => ({
      change: member(node.change),
      above: levelOf(node.above, depth + 1),
    })),
    end: endOf(above.end),
  })
  return {
    base: baseOf(stack.bottom),
    chain: [
      ...stack.below.map(member),
      {
        kind: 'current',
        reference: `${prefix}${stack.current.number}`,
        title: stack.current.title,
      },
    ],
    above: levelOf(stack.above, 1),
  }
}

function baseOf(bottom: ChangeStackBottom): StackBaseVm {
  switch (bottom.kind) {
    case 'reached':
      return { kind: 'branch', branch: bottom.branch }
    case 'ambiguous':
      return { kind: 'ambiguous', text: AMBIGUOUS }
    case 'cut':
      return { kind: 'cut', reason: bottom.reason, text: BELOW_CUT[bottom.reason] }
  }
}

function endOf(end: ChangeStackEnd): StackEndVm {
  switch (end.kind) {
    case 'complete':
      return { kind: 'complete' }
    case 'cut':
      return { kind: 'cut', reason: end.reason, text: ABOVE_CUT[end.reason] }
  }
}
