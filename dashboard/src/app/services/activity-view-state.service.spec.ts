import { TestBed } from '@angular/core/testing'
import { describe, expect, it } from 'vitest'
import { connectionSet, gitlabProvider as gitlab, providerInfo } from '../testing/activity-fixtures'
import { ActivityViewStateService } from './activity-view-state.service'

describe('ActivityViewStateService', () => {
  it('toggles active sources only for usable providers', () => {
    const service = TestBed.inject(ActivityViewStateService)
    service.activeConnections.set(connectionSet('git', 'jira'))

    service.toggleConnection('git/default', providerInfo)

    expect(service.activeConnections().has('git/default')).toBe(false)
    expect(service.activeConnections().has('jira/default')).toBe(true)
  })

  it('isolates a source and then restores all usable providers', () => {
    const service = TestBed.inject(ActivityViewStateService)
    service.activeConnections.set(connectionSet('git', 'jira'))

    service.isolateConnection('git/default', providerInfo)
    expect(Array.from(service.activeConnections())).toEqual(['git/default'])

    service.isolateConnection('git/default', providerInfo)
    expect(service.activeConnections()).toEqual(connectionSet('git', 'jira'))
  })

  it('shows a source while it is checked and drops it when its check fails', () => {
    const service = TestBed.inject(ActivityViewStateService)
    const [git, jira] = providerInfo
    if (!git || !jira) throw new Error('fixture has two providers')
    const checking = { ...jira, status: 'checking', usable: false } as const

    service.activateAllUsable([git, checking])
    expect(service.activeConnections()).toEqual(connectionSet('git', 'jira'))

    service.followProviders([git, checking], [git, checking])
    expect(service.activeConnections()).toEqual(connectionSet('git', 'jira'))

    const failed = { ...jira, status: 'unavailable', usable: false } as const
    service.followProviders([git, failed], [git, checking])
    expect(service.activeConnections()).toEqual(connectionSet('git'))

    // Usable again: shown again, like a source that is new.
    service.followProviders([git, jira], [git, failed])
    expect(service.activeConnections()).toEqual(connectionSet('git', 'jira'))
  })

  it('shows a new source and leaves switched-off sources off', () => {
    const service = TestBed.inject(ActivityViewStateService)
    const [git, jira] = providerInfo
    if (!git || !jira) throw new Error('fixture has two providers')
    service.activeConnections.set(connectionSet('jira'))

    service.followProviders([git, jira, gitlab], [git, jira])

    expect(service.activeConnections()).toEqual(connectionSet('jira', 'gitlab'))
  })

  it('clears search when switching views', () => {
    const service = TestBed.inject(ActivityViewStateService)
    service.searchQuery.set('ticket')

    service.setView('day')

    expect(service.currentView()).toBe('day')
    expect(service.searchQuery()).toBe('')
  })
})
