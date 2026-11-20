import { TestBed } from '@angular/core/testing'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { TOUCH_INPUT_CLASS, followTouchInput } from './touch-input'

/** A pointer event as the browser sends it, from a control inside the page. */
function pointer(type: 'pointerdown' | 'pointermove', pointerType: string): void {
  const control = document.body.appendChild(document.createElement('button'))
  // Stopped where it lands, as a widget may: the page still follows the pointer.
  control.addEventListener(type, event => event.stopPropagation())
  control.dispatchEvent(new PointerEvent(type, { pointerType, bubbles: true }))
  control.remove()
}

const touchInput = (): boolean => document.documentElement.classList.contains(TOUCH_INPUT_CLASS)

describe('followTouchInput', () => {
  beforeEach(() => {
    TestBed.runInInjectionContext(() => followTouchInput(document))
  })

  afterEach(() => {
    TestBed.resetTestingModule()
    document.documentElement.classList.remove(TOUCH_INPUT_CLASS)
  })

  it('turns hover off after a finger presses, until a mouse moves', () => {
    pointer('pointerdown', 'touch')
    expect(touchInput()).toBe(true)

    pointer('pointermove', 'touch')
    expect(touchInput()).toBe(true)

    pointer('pointermove', 'mouse')
    expect(touchInput()).toBe(false)
  })

  it('turns hover back on when a mouse or a pen presses', () => {
    pointer('pointerdown', 'touch')
    pointer('pointerdown', 'mouse')
    expect(touchInput()).toBe(false)

    pointer('pointerdown', 'touch')
    pointer('pointerdown', 'pen')
    expect(touchInput()).toBe(false)
  })

  it('stops following once its injector is gone', () => {
    TestBed.resetTestingModule()
    pointer('pointerdown', 'touch')
    expect(touchInput()).toBe(false)
  })
})
