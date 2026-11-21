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

/** A change's number as its host writes it, such as `!12`. */
export function changeReference(prefix: ChangeReferencePrefix, number: number): string {
  return `${prefix}${number}`
}

/**
 * A change of a stack, as its row shows it: `!12 Title`, linked to its page where the host gave a
 * safe one. The entry's own change is `current`, marked as the one the details are about.
 */
export interface StackChangeVm {
  readonly kind: 'current' | 'member'
  readonly reference: string
  readonly title: string
  readonly href: string | null
}

/** The first line, under the lowest change: what the stack stands on. */
export type StackBaseVm =
  /** `lead` comes before the branch, which shows as code: "Builds on `main`". */
  | { readonly kind: 'branch'; readonly lead: string; readonly branch: string }
  | { readonly kind: 'ambiguous'; readonly text: string }
  | { readonly kind: 'cut'; readonly reason: StackCut; readonly text: string }

/** How a level of changes built on one ends: all of it read, or cut, which its last line says. */
export type StackEndVm =
  | { readonly kind: 'complete' }
  | { readonly kind: 'cut'; readonly reason: StackCut; readonly text: string }

/** The changes built on one change, each with the changes built on it in turn. */
export interface StackLevelVm {
  /** 1 for the changes built on the entry's own, 2 for those built on them, and so on. */
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
 * changes it builds on, the entry's own, then the tree of changes built on it. The backend
 * computes the stack; this only flattens it for the template and words each kind.
 */
export interface ChangeStackVm {
  readonly base: StackBaseVm
  /** The changes the entry's own builds on, from the base up, then the entry's own. */
  readonly chain: readonly StackChangeVm[]
  readonly above: StackLevelVm
}

// The words of every line that is no change. Each text is one line here, so changing one is too.

const BASE_TEXT = {
  branch: 'Builds on',
  ambiguous:
    'Builds on a branch that several open changes come from, so what lies below is not shown',
} as const

/** The first line when what the stack builds on was not read to its base. */
const BELOW_CUT: Record<StackCut, string> = {
  limit: "Not all of what this builds on was read: the collection's limit was reached",
  unreadable: "What this builds on is hidden from this connection's token",
  throttled: 'Not all of what this builds on was read: the server asked to slow down',
  failed: 'Not all of what this builds on was read: a request failed',
  cycle: 'The branches below lead back into this stack, so its base is unknown',
}

/** The last line of a level of changes built on one that was not read to its end. */
const ABOVE_CUT: Record<StackCut, string> = {
  limit: "More changes built on it were not read: the collection's limit was reached",
  unreadable: "More changes built on it are hidden from this connection's token",
  throttled: 'More changes built on it were not read: the server asked to slow down',
  failed: 'More changes built on it were not read: a request failed',
  cycle: 'A branch leads back into this stack',
}

export function changeStackVmOf(stack: ChangeStack, prefix: ChangeReferencePrefix): ChangeStackVm {
  const row = (change: StackedChange, kind: StackChangeVm['kind']): StackChangeVm => ({
    kind,
    reference: changeReference(prefix, change.number),
    title: change.title,
    href: change.webUrl ?? null,
  })
  const levelOf = (above: ChangeStackAbove, depth: number): StackLevelVm => ({
    depth,
    nodes: above.changes.map(node => ({
      change: row(node.change, 'member'),
      above: levelOf(node.above, depth + 1),
    })),
    end: endOf(above.end),
  })
  return {
    base: baseOf(stack.bottom),
    chain: [...stack.below.map(change => row(change, 'member')), row(stack.current, 'current')],
    above: levelOf(stack.above, 1),
  }
}

function baseOf(bottom: ChangeStackBottom): StackBaseVm {
  switch (bottom.kind) {
    case 'reached':
      return { kind: 'branch', lead: BASE_TEXT.branch, branch: bottom.branch }
    case 'ambiguous':
      return { kind: 'ambiguous', text: BASE_TEXT.ambiguous }
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
