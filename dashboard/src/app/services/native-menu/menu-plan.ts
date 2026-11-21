import {
  MENU_SUBMENUS,
  type MenuItemKey,
  type MenuItemState,
  type MenuKey,
  type MenuSpec,
  type MenuSubmenu,
} from '../../generated/activity'
import { NAV_GROUPS } from '../../models/nav'
import {
  type ShortcutEntry,
  type ShortcutId,
  VIEW_SHORTCUTS,
  chordsIn,
} from '../../models/shortcuts'
import type { ViewMode } from '../../utils/app-url'
import { type ChordKeys, type Os, menuKeyOf } from '../../utils/key-chord'

/**
 * What a chosen item runs: what its accelerator's chord resolves to here and now (several
 * entries may share it, the scope on screen deciding, as ⌘F does), or its one entry.
 */
export type MenuAction =
  | { readonly kind: 'chord'; readonly keys: ChordKeys }
  | { readonly kind: 'entry'; readonly id: ShortcutId }

/** One item of the macOS menu, made from the shortcut table. */
export interface MenuItemPlan {
  readonly key: MenuItemKey
  readonly accelerator: MenuKey | null
  readonly action: MenuAction
  /** The entries it stands for, in the table's order; the first names it when none resolves. */
  readonly entries: readonly [ShortcutEntry, ...ShortcutEntry[]]
  /** The view it shows, checked while that view is on screen. */
  readonly view: ViewMode | null
}

export interface MenuSectionPlan {
  readonly submenu: MenuSubmenu
  readonly items: readonly MenuItemPlan[]
}

/** What an item is made of before it has its place and key. */
interface Draft {
  readonly accelerator: MenuKey | null
  readonly action: MenuAction
  readonly entries: [ShortcutEntry, ...ShortcutEntry[]]
}

/**
 * The app's part of the macOS menu, from the shortcut table: every entry with a `menu`, one item
 * per accelerator its entries share and one per entry without one, in the table's order. The View
 * submenu lists the views as the navigation groups them (`NAV_GROUPS`), one section per group,
 * then its other entries. Throws for an accelerator the menu has no key for: the menu is then not
 * installed, and the page keeps every key.
 */
export function menuPlanOf(entries: readonly ShortcutEntry[], os: Os): readonly MenuSectionPlan[] {
  const drafts: Draft[] = []
  const byChord = new Map<ChordKeys, Draft>()
  for (const entry of entries) {
    if (entry.menu === undefined) continue
    const chord = chordsIn(entry, os, 'desktop').find(c => c.menuAccelerator)
    if (chord === undefined) {
      drafts.push({ accelerator: null, action: { kind: 'entry', id: entry.id }, entries: [entry] })
      continue
    }
    const shared = byChord.get(chord.keys)
    if (shared !== undefined) {
      shared.entries.push(entry)
      continue
    }
    const accelerator = menuKeyOf(chord.keys)
    if (accelerator === null) throw new Error(`${chord.keys} cannot be a menu accelerator`)
    const draft: Draft = {
      accelerator,
      action: { kind: 'chord', keys: chord.keys },
      entries: [entry],
    }
    byChord.set(chord.keys, draft)
    drafts.push(draft)
  }

  const viewOf = new Map<ShortcutId, ViewMode>(
    NAV_GROUPS.flatMap(group => group.items).map(nav => [VIEW_SHORTCUTS[nav.id], nav.id])
  )
  const items = drafts.map(draft => {
    const [first] = draft.entries
    const view = first.menu === 'view' && draft.entries.length === 1 ? viewOf.get(first.id) : null
    return { ...draft, submenu: first.menu, view: view ?? null }
  })
  type Placed = (typeof items)[number]
  const sections: { readonly submenu: MenuSubmenu; readonly items: readonly Placed[] }[] = []
  for (const submenu of MENU_SUBMENUS) {
    const listed = items.filter(item => item.submenu === submenu)
    if (submenu === 'view') {
      for (const group of NAV_GROUPS) {
        sections.push({
          submenu,
          items: group.items.flatMap(nav => listed.filter(item => item.view === nav.id)),
        })
      }
    }
    sections.push({ submenu, items: listed.filter(item => item.view === null) })
  }

  let next = 0
  return sections
    .filter(section => section.items.length > 0)
    .map(section => ({
      submenu: section.submenu,
      items: section.items.map(({ accelerator, action, entries, view }) => ({
        key: next++,
        accelerator,
        action,
        entries,
        view,
      })),
    }))
}

/** The spec `install_menu` takes: the plan, each item showing `stateOf` it. */
export function menuSpecOf(
  plan: readonly MenuSectionPlan[],
  stateOf: (item: MenuItemPlan) => MenuItemState
): MenuSpec {
  return {
    sections: plan.map(section => ({
      submenu: section.submenu,
      items: section.items.map(item => ({
        key: item.key,
        ...(item.accelerator === null ? {} : { accelerator: item.accelerator }),
        state: stateOf(item),
      })),
    })),
  }
}

/** Whether two states show the same. */
export function sameState(a: MenuItemState, b: MenuItemState): boolean {
  if (a.kind === 'check' && b.kind === 'check') {
    return a.title === b.title && a.enabled === b.enabled && a.checked === b.checked
  }
  return a.kind === b.kind && a.title === b.title && a.enabled === b.enabled
}
