import { ElementRef, type Signal, afterRenderEffect, computed, effect, inject } from '@angular/core'
import { listener } from '@signality/core/browser/listener'
import { windowSize } from '@signality/core/browser/window-size'
import { ActivityDataService } from '../../services/activity-data.service'
import { ActivityViewStateService } from '../../services/activity-view-state.service'
import { ListMotion, type ListMotionFrame } from '../list-motion/list-motion'
import { measurePositionedElements } from '../list-motion/positioned-elements'

/** One ListMotion for the whole calendar so its change limit is shared by the columns and bars. */
export function followCalendarMotion(scroller: Signal<ElementRef<HTMLElement>>): void {
  const view = inject(ActivityViewStateService)
  const data = inject(ActivityDataService)
  const host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement
  const motion = new ListMotion(host)
  const viewport = windowSize()
  const scene = computed(() => {
    const meta = data.meta()
    return JSON.stringify([
      view.currentView(),
      view.selectedDay(),
      view.dateFrom(),
      view.dateTo(),
      view.searchQuery(),
      [...view.activeConnections()].sort(),
      view.hourHeight(),
      viewport.width(),
      viewport.height(),
      meta?.start,
      meta?.end,
    ])
  })
  let previousScene: string | undefined
  let previousItems: ListMotionFrame['items'] | undefined
  let settledOnce = false
  let lastScroll = 0
  let focused: HTMLElement | undefined

  // A keyed repeater can detach the same button briefly when its order changes.
  effect(() => {
    data.allEntries()
    const active = host.ownerDocument.activeElement
    focused =
      active instanceof HTMLElement &&
      active.matches('[data-calendar-block]') &&
      host.contains(active)
        ? active
        : undefined
  })
  listener.passive(scroller, 'scroll', () => {
    if (scroller().nativeElement.scrollTop !== lastScroll) motion.dropGhosts()
  })

  afterRenderEffect({
    earlyRead: () => {
      const root = scroller().nativeElement
      const sceneKey = scene()
      const revision = data.allEntries()
      const ready = data.meta() !== null && !data.isLoading()
      const elements = [...root.querySelectorAll<HTMLElement>('[data-calendar-block]')].map(
        element => ({
          key: element.getAttribute('data-calendar-block')!,
          element,
          pinned: element.classList.contains('cal-allday-bar'),
        })
      )
      const layout = measurePositionedElements(root, elements)
      const ghosts = root.querySelector<HTMLElement>('.cal-block-ghosts')
      return { sceneKey, revision, ready, layout, ghosts }
    },
    write: sample => {
      const { sceneKey, revision, ready, layout, ghosts } = sample()
      const { items, scrollTop } = layout
      const sameScene = sceneKey === previousScene
      const replaced =
        previousItems?.size && items.size && ![...items.keys()].some(key => previousItems?.has(key))
      if (!sameScene || !settledOnce || replaced) motion.reset()
      previousScene = sceneKey
      previousItems = items
      settledOnce ||= ready
      lastScroll = scrollTop

      if (
        sameScene &&
        focused?.isConnected &&
        host.ownerDocument.activeElement === host.ownerDocument.body
      )
        focused.focus({ preventScroll: true })
      focused = undefined

      motion.update({
        ...layout,
        revision,
        ghost: (item, top) => {
          if (!ghosts || !item.across) return null
          const clone = item.element.cloneNode(true)
          if (!(clone instanceof HTMLElement)) return null
          clone.removeAttribute('data-calendar-block')
          clone.style.top = `${top + scrollTop}px`
          clone.style.left = `${item.across.left}px`
          clone.style.width = `${item.across.width}px`
          clone.style.height = `${item.size}px`
          ghosts.append(clone)
          return clone
        },
      })
    },
  })
}
