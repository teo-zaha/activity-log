import { describe, expect, it } from 'vitest'
import { jsonEqual } from './json-equal'

describe('jsonEqual', () => {
  it('compares primitives, arrays and objects by content', () => {
    expect(jsonEqual(1, 1)).toBe(true)
    expect(jsonEqual('a', 'b')).toBe(false)
    expect(jsonEqual(null, {})).toBe(false)
    expect(jsonEqual([1, { a: [2] }], [1, { a: [2] }])).toBe(true)
    expect(jsonEqual([1, 2], [1, 2, 3])).toBe(false)
    expect(jsonEqual({ a: 1, b: { c: 'x' } }, { b: { c: 'x' }, a: 1 })).toBe(true)
    expect(jsonEqual({ a: 1, b: { c: 'x' } }, { a: 1, b: { c: 'y' } })).toBe(false)
  })

  it('tells a missing key from one that is undefined or added', () => {
    expect(jsonEqual({ a: 1 }, { a: 1, b: 2 })).toBe(false)
    expect(jsonEqual({ a: 1, b: undefined }, { a: 1, c: undefined })).toBe(false)
    expect(jsonEqual([], {})).toBe(false)
  })
})
