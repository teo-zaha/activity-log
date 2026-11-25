import { describe, expect, it } from 'vitest'
import type { ProviderFixAction } from '../../generated/activity'
import { activityLog, collectingLog, MockBackendTransport } from '../../testing/activity-fixtures'
import { decodeRuntimeMessage } from './runtime-message'

const { activity, providers } = new MockBackendTransport().runtimeState()
const running = { ...activity, state: { kind: 'running', log: collectingLog } }
const ready = { ...activity, state: { kind: 'ready', log: activityLog } }
const withLog = (log: object): object => ({ ...activity, state: { kind: 'ready', log } })
const withList = (list: unknown): object => ({
  ...providers,
  providers: { currentOs: 'linux', secretStore: { store: 'os', state: 'ready' }, providers: list },
})
const withoutRecheckable = (): object => {
  const provider: Record<string, unknown> = { ...providers.providers.providers[0] }
  delete provider['recheckable']
  return provider
}

describe('decodeRuntimeMessage', () => {
  it('takes the messages the host sends', () => {
    expect(decodeRuntimeMessage('state', { activity, providers })).not.toBeNull()
    expect(decodeRuntimeMessage('activity', running)).toEqual({
      event: 'activity',
      payload: running,
    })
    expect(decodeRuntimeMessage('activity', ready)).not.toBeNull()
    expect(decodeRuntimeMessage('providers', providers)).not.toBeNull()
  })

  it.each([
    ['a null provider', [null]],
    ['a provider that is no object', ['git']],
    [
      'a provider without its connection',
      [{ ...providers.providers.providers[0], connection: null }],
    ],
    ['a provider without usable', [{ ...providers.providers.providers[0], usable: 'yes' }]],
    [
      'a recheckable that is no boolean',
      [{ ...providers.providers.providers[0], recheckable: 'false' }],
    ],
    ['a provider without recheckable', [withoutRecheckable()]],
  ])('refuses providers with %s', (_name, list) => {
    expect(decodeRuntimeMessage('providers', withList(list))).toBeNull()
    expect(decodeRuntimeMessage('state', { activity, providers: withList(list) })).toBeNull()
  })

  it.each([
    { kind: 'local', target: { kind: 'systemSettings', pane: 'automation' } },
    { kind: 'local', target: { kind: 'systemSettings', pane: 'filesAndFolders' } },
    { kind: 'local', target: { kind: 'openApp', app: 'outlook' } },
    {
      kind: 'web',
      action: {
        kind: 'createToken',
        url: 'https://example.org/tokens/new',
        instructions: 'Select read access.',
        classicUrl: 'https://example.org/classic-tokens/new',
      },
    },
    {
      kind: 'web',
      action: {
        kind: 'feedHelp',
        guides: [{ service: 'iCloud', label: 'iCloud', instructions: 'Share the calendar.' }],
        recovery: 'Check the sharing settings.',
      },
    },
  ] satisfies ProviderFixAction[])('reads a valid provider recovery action', fixAction => {
    const update = withList([{ ...providers.providers.providers[0], fixAction }])
    expect(decodeRuntimeMessage('providers', update)).toEqual({
      event: 'providers',
      payload: update,
    })
    expect(decodeRuntimeMessage('state', { activity, providers: update })).not.toBeNull()
  })

  it.each([
    ['null', null],
    ['unknown provider action', { kind: 'follow' }],
    ['unknown web action', { kind: 'web', action: { kind: 'follow' } }],
    ['unknown local target', { kind: 'local', target: { kind: 'openFile' } }],
    ['unknown app', { kind: 'local', target: { kind: 'openApp', app: 'calendar' } }],
    ['unknown pane', { kind: 'local', target: { kind: 'systemSettings', pane: 'privacy' } }],
    [
      'invalid token URL',
      {
        kind: 'web',
        action: { kind: 'createToken', url: 'file:///example', instructions: 'Read access' },
      },
    ],
    [
      'invalid classic URL',
      {
        kind: 'web',
        action: {
          kind: 'createToken',
          url: 'https://example.org',
          instructions: 'Read access',
          classicUrl: 1,
        },
      },
    ],
    [
      'missing instructions',
      { kind: 'web', action: { kind: 'createToken', url: 'https://example.org' } },
    ],
    [
      'invalid service',
      {
        kind: 'web',
        action: {
          kind: 'feedHelp',
          guides: [{ service: 'unknown', label: 'Unknown', instructions: 'Share the calendar.' }],
        },
      },
    ],
    [
      'invalid guide URL',
      {
        kind: 'web',
        action: {
          kind: 'feedHelp',
          guides: [
            {
              service: 'fastmail',
              label: 'Fastmail',
              instructions: 'Share the calendar.',
              url: 'relative',
            },
          ],
        },
      },
    ],
    [
      'invalid recovery text',
      { kind: 'web', action: { kind: 'feedHelp', guides: [], recovery: true } },
    ],
  ])('refuses a provider recovery action with %s', (_name, fixAction) => {
    const update = withList([{ ...providers.providers.providers[0], fixAction }])
    expect(decodeRuntimeMessage('providers', update)).toBeNull()
    expect(decodeRuntimeMessage('state', { activity, providers: update })).toBeNull()
  })

  it.each([
    ['no secret store', undefined],
    ['a store of no known kind', { store: 'keychain' }],
    ['the OS store in no known state', { store: 'os', state: 'locked' }],
    ['the key file for no known reason', { store: 'file', why: 'osStore' }],
    ['no store for no known reason', { store: 'unavailable', why: 'forced' }],
  ])('refuses providers with %s', (_name, secretStore) => {
    const withStore = { ...providers, providers: { ...providers.providers, secretStore } }
    expect(decodeRuntimeMessage('providers', withStore)).toBeNull()
  })

  it.each([
    ['connections that are no list', { ...activityLog.summary, connections: {} }],
    ['a count that is no object', { ...activityLog.summary, connections: [null] }],
    ['a count without its connection', { ...activityLog.summary, connections: [{ count: 1 }] }],
  ])('refuses a summary with %s', (_name, summary) => {
    expect(decodeRuntimeMessage('activity', withLog({ ...activityLog, summary }))).toBeNull()
  })

  it.each([
    ['no list', {}],
    ['an item that is no connection', [{ source: 'jira' }]],
  ])('refuses pending connections that are %s', (_name, pending) => {
    const meta = { ...activityLog.meta, pending }
    expect(decodeRuntimeMessage('activity', withLog({ ...activityLog, meta }))).toBeNull()
  })

  it('refuses a connection of a source this dashboard does not know', () => {
    const later = { source: 'later', name: 'default' }
    const [first] = providers.providers.providers
    const summary = { ...activityLog.summary, connections: [{ connection: later, count: 1 }] }
    const meta = { ...activityLog.meta, pending: [later] }

    expect(
      decodeRuntimeMessage('providers', withList([{ ...first, connection: later }]))
    ).toBeNull()
    expect(decodeRuntimeMessage('activity', withLog({ ...activityLog, summary }))).toBeNull()
    expect(decodeRuntimeMessage('activity', withLog({ ...activityLog, meta }))).toBeNull()
  })
})
