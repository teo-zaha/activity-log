import { DefaultUrlSerializer } from '@angular/router'
import { describe, expect, it } from 'vitest'
import {
  DEFAULT_LOCATION,
  deepLinkPath,
  parseAppUrl,
  parseSettingsParam,
  queryParamsOf,
  settingsParam,
  viewCommands,
} from './app-url'

const serializer = new DefaultUrlSerializer()
const parse = (url: string): ReturnType<typeof parseAppUrl> => parseAppUrl(serializer.parse(url))

describe('app URLs', () => {
  it('reads the views and their days', () => {
    expect(parse('/')).toEqual(DEFAULT_LOCATION)
    expect(parse('/timeline')).toEqual(DEFAULT_LOCATION)
    expect(parse('/day/2026-11-17')).toEqual({
      ...DEFAULT_LOCATION,
      view: 'day',
      day: '2026-11-17',
    })
    expect(parse('/workweek')).toEqual({ ...DEFAULT_LOCATION, view: 'workweek' })
    expect(parse('/week/2026-02-30')).toEqual({ ...DEFAULT_LOCATION, view: 'week' })
  })

  it('reads no view from other paths', () => {
    expect(parse('/settings')).toBeNull()
    expect(parse('/timeline/2026-11-17')).toBeNull()
    expect(parse('/day/2026-11-17/extra')).toBeNull()
    expect(parse('/timeline(side:panel)')).toBeNull()
  })

  it('reads the known query parameters and drops invalid values', () => {
    expect(parse('/timeline?q=fix%20bug&from=2026-11-01&to=2026-11-14&settings=logs&x=1')).toEqual({
      ...DEFAULT_LOCATION,
      q: 'fix bug',
      from: '2026-11-01',
      to: '2026-11-14',
      settings: 'logs',
    })
    expect(parse('/timeline?q=a&q=b&from=1.11.2026&settings=general%2Fx')).toEqual(DEFAULT_LOCATION)
  })

  it('round-trips a connection section through the settings parameter', () => {
    expect(settingsParam('connection:gitlab/oss')).toBe('gitlab/oss')
    expect(parseSettingsParam('gitlab/oss')).toBe('connection:gitlab/oss')
    expect(parseSettingsParam(settingsParam('connection:calendar/team-a'))).toBe(
      'connection:calendar/team-a'
    )
    expect(parseSettingsParam('add')).toBe('add')
    expect(parseSettingsParam('github/me')).toBeNull()
    expect(parseSettingsParam('gitlab/')).toBeNull()
    expect(parseSettingsParam(null)).toBeNull()
  })

  it('builds commands and query parameters', () => {
    expect(viewCommands('list', '2026-11-17')).toEqual(['/timeline'])
    expect(viewCommands('workweek', '2026-11-17')).toEqual(['/workweek', '2026-11-17'])
    expect(
      queryParamsOf({
        ...DEFAULT_LOCATION,
        from: '2026-11-01',
        settings: 'connection:jira/default',
      })
    ).toEqual({
      q: null,
      from: '2026-11-01',
      to: null,
      settings: 'jira/default',
      event: null,
      events: null,
    })
  })

  it('turns deep links into app paths', () => {
    expect(deepLinkPath('activity-log://day/2026-11-17?q=x')).toBe('/day/2026-11-17?q=x')
    expect(deepLinkPath('Activity-Log:/timeline')).toBe('/timeline')
    expect(deepLinkPath('activity-log://')).toBe('/')
    expect(deepLinkPath(' /week/2026-11-17 ')).toBe('/week/2026-11-17')
    expect(deepLinkPath('https://example.com/day/2026-11-17')).toBeNull()
    expect(deepLinkPath('day/2026-11-17')).toBeNull()
  })

  it('reads an open event, with slashes in its id', () => {
    expect(parse('/day/2026-11-17?event=calendar/team/uid/1@example.org')).toEqual({
      ...DEFAULT_LOCATION,
      view: 'day',
      day: '2026-11-17',
      event: 'calendar/team/uid/1@example.org',
    })
    expect(parse('/timeline?event=github/me/1')?.event).toBeNull()
    expect(parse('/timeline?event=git/work/')?.event).toBeNull()
    expect(parse('/timeline?event=git//1')?.event).toBeNull()
  })

  it('reads a group of overlapping events and the one open in it', () => {
    const group = '/day/2026-11-17?events=outlook/w/a&events=outlook/w/b%26c&events=outlook/w/a'
    expect(parse(group)).toMatchObject({ event: null, events: ['outlook/w/a', 'outlook/w/b&c'] })
    expect(parse(`${group}&event=outlook/w/b%26c`)).toMatchObject({
      event: 'outlook/w/b&c',
      events: ['outlook/w/a', 'outlook/w/b&c'],
    })
    // An event outside its group keeps the event; a group of one is just its event.
    expect(parse(`${group}&event=git/x/1`)).toMatchObject({ event: 'git/x/1', events: [] })
    expect(parse('/timeline?events=git/x/1')).toMatchObject({ event: 'git/x/1', events: [] })
  })

  it('writes a group as repeated parameters that read back the same', () => {
    const location = parse('/day/2026-11-17?events=outlook/w/a%26b&events=git/x/1&event=git/x/1')!
    const params = queryParamsOf(location)
    expect(params).toMatchObject({ event: 'git/x/1', events: ['outlook/w/a&b', 'git/x/1'] })
    const tree = serializer.parse('/day/2026-11-17')
    // As a navigation sets them: a null parameter is left out.
    tree.queryParams = Object.fromEntries(
      Object.entries(params).filter(([, value]) => value !== null)
    )
    expect(parse(serializer.serialize(tree))).toEqual(location)
  })
})
