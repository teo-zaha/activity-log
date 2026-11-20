import { describe, expect, it } from 'vitest'
import { activityLog, collectingLog, MockBackendTransport } from '../../testing/activity-fixtures'
import { decodeRuntimeMessage } from './runtime-message'

const { activity, providers } = new MockBackendTransport().runtimeState()
const running = { ...activity, state: { kind: 'running', log: collectingLog } }
const ready = { ...activity, state: { kind: 'ready', log: activityLog } }
const withLog = (log: object): object => ({ ...activity, state: { kind: 'ready', log } })
const withList = (list: unknown): object => ({
  ...providers,
  providers: { currentOs: 'linux', providers: list },
})

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
  ])('refuses providers with %s', (_name, list) => {
    expect(decodeRuntimeMessage('providers', withList(list))).toBeNull()
    expect(decodeRuntimeMessage('state', { activity, providers: withList(list) })).toBeNull()
  })

  it('reads a source without recheckable, as a host older than the dashboard sends it, as not recheckable', () => {
    const older: Record<string, unknown> = { ...providers.providers.providers[0] }
    delete older['recheckable']
    const list = [older]

    const update = decodeRuntimeMessage('providers', withList(list))
    const pair = decodeRuntimeMessage('state', { activity, providers: withList(list) })

    expect(
      update?.event === 'providers' && update.payload.providers.providers[0]?.recheckable
    ).toBe(false)
    expect(
      pair?.event === 'state' && pair.payload.providers.providers.providers[0]?.recheckable
    ).toBe(false)
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
