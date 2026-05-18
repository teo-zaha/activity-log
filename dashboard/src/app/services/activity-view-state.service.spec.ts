import { TestBed } from '@angular/core/testing'
import { describe, expect, it } from 'vitest'
import { providerInfo, sourceSet } from '../testing/activity-fixtures'
import { ActivityViewStateService } from './activity-view-state.service'

describe('ActivityViewStateService', () => {
  it('toggles active sources only for usable providers', () => {
    const service = TestBed.inject(ActivityViewStateService)
    service.activeSources.set(sourceSet('git', 'jira'))

    service.toggleSource('git', providerInfo)

    expect(service.activeSources().has('git')).toBe(false)
    expect(service.activeSources().has('jira')).toBe(true)
  })

  it('isolates a source and then restores all usable providers', () => {
    const service = TestBed.inject(ActivityViewStateService)
    service.activeSources.set(sourceSet('git', 'jira'))

    service.isolateSource('git', providerInfo)
    expect(Array.from(service.activeSources())).toEqual(['git'])

    service.isolateSource('git', providerInfo)
    expect(service.activeSources()).toEqual(sourceSet('git', 'jira'))
  })

  it('clears search when switching views', () => {
    const service = TestBed.inject(ActivityViewStateService)
    service.searchQuery.set('ticket')

    service.setView('day')

    expect(service.currentView()).toBe('day')
    expect(service.searchQuery()).toBe('')
  })
})
