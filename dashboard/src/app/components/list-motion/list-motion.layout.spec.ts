import { afterEach, describe, expect, it } from 'vitest'
import { ListMotion, type ListMotionFrame, type PositionedItem } from './list-motion'

const motion = new ListMotion()
const host = document.createElement('div')
document.body.append(host)

afterEach(() => {
  motion.reset()
  host.replaceChildren()
})

function element(key: string): HTMLElement {
  const node = document.createElement('div')
  node.textContent = key
  host.append(node)
  return node
}

function items(rows: [string, HTMLElement, number][]): Map<string, PositionedItem> {
  return new Map(
    rows.map(([key, element, top]) => [
      key,
      { element, size: 20, topAt: (offset): number => top - offset },
    ])
  )
}

function frame(revision: number, mounted: ReadonlyMap<string, PositionedItem>): ListMotionFrame {
  return {
    revision,
    keys: mounted,
    items: mounted,
    beforeScroll: 0,
    scrollTop: 0,
    viewport: { top: 0, bottom: 200 },
    paused: false,
    pauseOn: '',
    ghost: (item): HTMLElement => {
      const clone = element(item.element.textContent ?? '')
      clone.className = 'test-ghost'
      return clone
    },
  }
}

function firstTransform(node: HTMLElement): string {
  const effect = node.getAnimations()[0]?.effect
  if (!(effect instanceof KeyframeEffect)) throw new Error('Expected a movement animation')
  return String(effect.getKeyframes()[0]?.['transform'])
}

describe('keyed list motion', () => {
  it('adds, reorders and removes items in an ordinary list', async () => {
    const a = element('a')
    const b = element('b')
    motion.update(
      frame(
        1,
        items([
          ['a', a, 0],
          ['b', b, 20],
        ])
      )
    )
    a.remove()
    const c = element('c')
    motion.update(
      frame(
        2,
        items([
          ['b', b, 0],
          ['c', c, 20],
        ])
      )
    )

    expect(firstTransform(b)).toBe('translateY(20px)')
    expect(c.getAnimations()).toHaveLength(1)
    const ghost = host.querySelector('.test-ghost')!
    expect(ghost.textContent).toBe('a')
    for (const animation of host.getAnimations({ subtree: true })) animation.finish()
    await expect.poll(() => ghost.isConnected).toBe(false)
  })

  it('ignores virtual mounts and animates only changes left after the host scroll correction', () => {
    const a = element('a')
    const b = element('b')
    const c = element('c')
    const keys = new Set(['a', 'b', 'c'])
    motion.update({
      ...frame(
        1,
        items([
          ['a', a, 0],
          ['b', b, 20],
        ])
      ),
      keys,
    })
    motion.update({
      ...frame(
        1,
        items([
          ['b', b, 20],
          ['c', c, 40],
        ])
      ),
      keys,
    })
    expect(host.getAnimations({ subtree: true })).toHaveLength(0)
    motion.update({
      ...frame(
        2,
        items([
          ['b', b, 40],
          ['c', c, 80],
        ])
      ),
      keys,
      scrollTop: 20,
      anchor: 'b',
    })
    expect(b.getAnimations()).toHaveLength(0)
    expect(firstTransform(c)).toBe('translateY(-20px)')
    expect(host.querySelector('.test-ghost')).toBeNull()
  })

  it('retargets an unfinished transform from its current screen position', () => {
    const node = element('a')
    motion.update(frame(1, items([['a', node, 30]])))
    motion.update(frame(2, items([['a', node, 70]])))
    const running = node.getAnimations()[0]!
    running.pause()
    running.currentTime = 90
    const progress = running.effect!.getComputedTiming().progress!
    motion.update(frame(3, items([['a', node, 50]])))
    const offset = Number.parseFloat(firstTransform(node).slice('translateY('.length))
    expect(offset).toBeCloseTo(70 - 40 * (1 - progress) - 50, 4)
  })

  it('starts from the current node when a virtual row has remounted', () => {
    const old = element('a')
    motion.update(frame(1, items([['a', old, 20]])))
    motion.update(frame(2, items([['a', old, 50]])))
    const running = old.getAnimations()[0]!
    running.pause()
    running.currentTime = 40
    old.remove()
    const current = element('a')
    motion.update(frame(2, items([['a', current, 50]])))
    expect(current.getAnimations()).toHaveLength(0)
    motion.update(frame(3, items([['a', current, 70]])))
    expect(firstTransform(current)).toBe('translateY(-20px)')
  })

  it('leaves a large replacement still', () => {
    const before = Array.from({ length: 25 }, (_, i): [string, HTMLElement, number] => [
      `old-${i}`,
      element(`old-${i}`),
      i * 20,
    ])
    motion.update(frame(1, items(before)))
    for (const [, node] of before) node.remove()
    const after = Array.from({ length: 25 }, (_, i): [string, HTMLElement, number] => [
      `new-${i}`,
      element(`new-${i}`),
      i * 20,
    ])
    motion.update(frame(2, items(after)))
    expect(host.getAnimations({ subtree: true })).toHaveLength(0)
    expect(host.querySelector('.test-ghost')).toBeNull()
  })

  it('cancels motion when paused or when the pause key changes', () => {
    const node = element('a')
    motion.update(frame(1, items([['a', node, 30]])))
    motion.update(frame(2, items([['a', node, 70]])))
    expect(node.getAnimations()).toHaveLength(1)
    motion.update({ ...frame(3, items([['a', node, 40]])), pauseOn: 'search' })
    expect(node.getAnimations()).toHaveLength(0)
    motion.update({ ...frame(4, items([['a', node, 50]])), pauseOn: 'search' })
    expect(node.getAnimations()).toHaveLength(1)
    motion.update({ ...frame(5, items([['a', node, 0]])), pauseOn: 'search', paused: true })
    expect(node.getAnimations()).toHaveLength(0)
  })
})
