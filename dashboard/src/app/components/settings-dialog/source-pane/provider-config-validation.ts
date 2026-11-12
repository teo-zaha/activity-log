import type { ProviderConfigValueTypeDto } from '../../../generated/activity'

export function validateProviderConfigValue(
  value: string,
  valueType: ProviderConfigValueTypeDto
): string | null {
  if (valueType === 'string' || valueType === 'password' || valueType === 'email') {
    return null
  }

  if (valueType === 'bool') {
    const normalized = value.trim().toLowerCase()
    return normalized === 'true' || normalized === 'false' ? null : 'Enter true/false.'
  }

  if (valueType === 'int') {
    return /^-?\d+$/.test(value.trim()) ? null : 'Enter a whole number.'
  }

  if (valueType === 'url') {
    try {
      const parsed = new URL(value)
      return parsed.protocol === 'http:' || parsed.protocol === 'https:'
        ? null
        : 'Enter a valid URL.'
    } catch {
      return 'Enter a valid URL.'
    }
  }

  return null
}
