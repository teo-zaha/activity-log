import { describe, expect, it } from 'vitest'
import { validateProviderConfigValue } from './provider-config-validation'

describe('validateProviderConfigValue', () => {
  it('validates booleans shallowly', () => {
    expect(validateProviderConfigValue('true', 'bool')).toBeNull()
    expect(validateProviderConfigValue('false', 'bool')).toBeNull()
    expect(validateProviderConfigValue('yes', 'bool')).toBe('Enter true/false.')
  })

  it('validates integer strings without numeric coercion surprises', () => {
    expect(validateProviderConfigValue('-42', 'int')).toBeNull()
    expect(validateProviderConfigValue('1.2', 'int')).toBe('Enter a whole number.')
  })

  it('accepts only http and https URLs', () => {
    expect(validateProviderConfigValue('https://example.test', 'url')).toBeNull()
    expect(validateProviderConfigValue('file:///tmp/test', 'url')).toBe('Enter a valid URL.')
  })

  it('leaves email, string, and password precision to the backend', () => {
    expect(validateProviderConfigValue('not an email', 'email')).toBeNull()
    expect(validateProviderConfigValue('anything', 'string')).toBeNull()
    expect(validateProviderConfigValue('secret', 'password')).toBeNull()
  })
})
