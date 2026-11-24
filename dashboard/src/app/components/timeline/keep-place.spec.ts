import { Component, type ElementRef, signal, viewChild } from '@angular/core'
import { render } from '@testing-library/angular'
import { expect, it, vi } from 'vitest'
import { keepPlace } from './keep-place'
import { timelineItems } from './timeline-items'

@Component({ template: '<div #scroller></div>' })
class ListWithMotion {
  readonly scroller = viewChild.required<ElementRef<HTMLElement>>('scroller')
  readonly layout = signal(timelineItems([]))
  readonly reducedMotion = signal(false)
  readonly coordinates = vi.fn(() => ({
    startOf: () => undefined,
    indexAt: () => undefined,
    maxScroll: 0,
    line: 0,
  }))
  readonly motion = vi.fn<(reduced: boolean) => void>()

  constructor() {
    keepPlace({
      scroller: this.scroller,
      layout: this.layout,
      coordinates: this.coordinates,
      placed: () => this.motion(this.reducedMotion()),
      scrolled: () => {},
    })
  }
}

it('runs motion after a layout change without tracking motion inputs in the correction', async () => {
  const { fixture } = await render(ListWithMotion)
  const list = fixture.componentInstance
  expect(list.coordinates).toHaveBeenCalled()
  expect(list.motion).toHaveBeenLastCalledWith(false)
  list.coordinates.mockClear()
  list.motion.mockClear()

  list.reducedMotion.set(true)
  fixture.detectChanges()
  expect(list.coordinates).not.toHaveBeenCalled()
  expect(list.motion).not.toHaveBeenCalled()

  list.layout.set(timelineItems([]))
  fixture.detectChanges()
  expect(list.coordinates).toHaveBeenCalled()
  expect(list.motion).toHaveBeenLastCalledWith(true)
})
