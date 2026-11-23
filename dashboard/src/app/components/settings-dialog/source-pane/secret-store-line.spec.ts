import { describe, expect, it } from 'vitest'
import type { SecretStoreInfo } from '../../../generated/activity'
import { secretStoreLine } from './secret-store-line'

describe('secretStoreLine', () => {
  it('says nothing while the OS store answers', () => {
    expect(secretStoreLine({ store: 'os', state: 'ready' })).toBeNull()
  })

  it.each([
    [{ store: 'os', state: 'refused' }, 'warn'],
    [{ store: 'file', why: 'noOsStore' }, 'info'],
    [{ store: 'file', why: 'forced' }, 'info'],
    [{ store: 'file', why: 'devProfile' }, 'info'],
    [{ store: 'unavailable', why: 'noOsStore' }, 'warn'],
  ] satisfies [SecretStoreInfo, string][])('has a line for %o', (info, tone) => {
    expect(secretStoreLine(info)?.tone).toBe(tone)
  })
})
