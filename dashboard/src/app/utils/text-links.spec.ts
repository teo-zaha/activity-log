import { describe, expect, it } from 'vitest'
import { isWebLink } from './text-links'

describe('isWebLink', () => {
  it('accepts only absolute http(s) links', () => {
    expect(isWebLink('https://example.org')).toBe(true)
    expect(isWebLink('http://example.org/x')).toBe(true)
    expect(isWebLink('javascript:alert(1)')).toBe(false)
    expect(isWebLink('file:///etc/passwd')).toBe(false)
    expect(isWebLink('/relative')).toBe(false)
    expect(isWebLink('https://')).toBe(false)
  })
})
