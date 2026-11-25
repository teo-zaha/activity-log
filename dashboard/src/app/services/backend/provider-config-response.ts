import { isOneOf, isRecord, isWebFixAction } from './runtime-message'
import type { ProviderConfigIssueKindDto } from '../../generated/activity'

const ISSUE_KINDS: Readonly<Record<ProviderConfigIssueKindDto, true>> = {
  invalid: true,
  changed: true,
  removed: true,
  replaced: true,
  unknownBase: true,
}

function isIssue(value: unknown): boolean {
  if (!isRecord(value) || !isOneOf(value['problem'], ISSUE_KINDS)) return false
  const field = value['field']
  if (typeof value['message'] !== 'string' || !isRecord(field)) return false
  switch (field['kind']) {
    case 'label':
    case 'connection':
      return true
    case 'value':
      return typeof field['key'] === 'string'
    case 'item':
      return (
        typeof field['key'] === 'string' &&
        typeof field['index'] === 'number' &&
        Number.isSafeInteger(field['index']) &&
        field['index'] >= 0
      )
    default:
      return false
  }
}

export function isSettingsCheck(value: unknown): boolean {
  if (!isRecord(value)) return false
  const issues = value['issues']
  const fix = value['fieldFix']
  return (
    Array.isArray(issues) &&
    issues.every(isIssue) &&
    (fix === undefined ||
      (isRecord(fix) && typeof fix['key'] === 'string' && isWebFixAction(fix['action'])))
  )
}

export function isSettingsSnapshot(value: unknown): boolean {
  if (!isRecord(value) || typeof value['revision'] !== 'string') return false
  const connections = value['connections']
  return (
    Array.isArray(value['kinds']) &&
    Array.isArray(connections) &&
    connections.every(
      (connection: unknown) =>
        isRecord(connection) &&
        Array.isArray(connection['fields']) &&
        connection['fields'].every(
          (field: unknown) =>
            isRecord(field) &&
            typeof field['key'] === 'string' &&
            (field['fixAction'] === undefined || isWebFixAction(field['fixAction']))
        )
    )
  )
}

export function hasSavedSettings(value: unknown): boolean {
  return isRecord(value) && isSettingsSnapshot(value['snapshot'])
}
