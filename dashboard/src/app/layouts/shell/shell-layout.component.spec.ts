import { Component } from '@angular/core'
import { TestBed } from '@angular/core/testing'
import { provideRouter } from '@angular/router'
import { fireEvent, screen, within } from '@testing-library/angular'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { loadView } from '../../app.routes'
import { createEntryAdapter } from '../../providers/provider-adapter'
import { ActivityService } from '../../services/activity.service'
import { activityLog } from '../../testing/activity-fixtures'
import { openApp as open } from '../../testing/app'
import { mockPhone } from '../../testing/media'
import { holdMotion } from '../../testing/motion'
import { ShellLayoutComponent } from './shell-layout.component'
import { ViewsLayoutComponent } from '../views/views-layout.component'

/**
 * For what loads its code on demand (the settings dialog), which takes seconds on a cold build
 * cache, and for the tests that wait on it.
 */
const SLOW = { timeout: 10_000 }
const SLOW_TEST = 30_000

const header = (): HTMLElement => screen.getByRole('banner')
const sidebar = (): HTMLElement => screen.getByRole('complementary', { name: 'Views and sources' })
const shellBody = (): Element | null => document.querySelector('.shell-body')
const [git, jira] = activityLog.entries.map(entry => createEntryAdapter(entry))
const key = (entry: { trackKey: string } | undefined): string =>
  encodeURIComponent(entry?.trackKey ?? '')
const classesOf = (element: Element | null | undefined): string[] => [...(element?.classList ?? [])]

describe('ShellLayoutComponent', () => {
  let phone: ReturnType<typeof mockPhone> | null = null
  let motion: ReturnType<typeof holdMotion> | null = null
  afterEach(() => {
    phone?.restore()
    phone = null
    motion?.restore()
    motion = null
    document.getElementById('splash')?.remove()
  })

  it('shows the views’ range in the header and their sources in the sidebar', async () => {
    await open('/day/2026-11-17')

    expect(within(header()).getByLabelText('From date')).toBeTruthy()
    expect(within(header()).getByRole('button', { name: 'Collect activity' })).toBeTruthy()
    await vi.waitFor(() =>
      expect(within(sidebar()).getByRole('button', { name: 'Add source' })).toBeTruthy()
    )
  })

  it('shows the phone’s Settings tab without the range and the chips, under the same brand and actions', async () => {
    phone = mockPhone(true)
    await open('/day/2026-11-17?settings=home')

    await vi.waitFor(() => expect(screen.getByRole('region', { name: 'Settings' })).toBeTruthy())
    expect(within(header()).queryByLabelText('From date')).toBeNull()
    expect(within(sidebar()).queryByRole('button', { name: 'Add source' })).toBeNull()
    expect(within(header()).getByText('Activity Log')).toBeTruthy()
    expect(within(header()).getByRole('button', { name: 'Share link to this view' })).toBeTruthy()
    expect(screen.getByRole('main').querySelector('.view')?.classList).toContain('under-settings')
    const tabs = within(screen.getByRole('navigation', { name: 'Views' }))
    expect(tabs.getByRole('button', { name: 'Settings' }).getAttribute('aria-current')).toBe('page')
  })

  it('puts the range and the chips back once the Settings tab closes', async () => {
    phone = mockPhone(true)
    const { router, settle } = await open('/day/2026-11-17?settings=home')
    expect(within(header()).queryByLabelText('From date')).toBeNull()

    await router.navigateByUrl('/day/2026-11-17')
    await settle()

    expect(within(header()).getByLabelText('From date')).toBeTruthy()
  })

  it('makes everything under the details inert, at every size, and focuses the event’s title', async () => {
    const { router, settle } = await open(`/day/2026-05-18?event=${key(git)}`)

    await vi.waitFor(() =>
      expect(document.activeElement).toBe(screen.getByRole('heading', { name: git?.title }))
    )
    for (const under of [header(), shellBody(), screen.getByText('Skip to activity')]) {
      expect(under?.closest('[inert]')).not.toBeNull()
    }

    await router.navigateByUrl('/day/2026-05-18')
    await settle()
    expect(header().closest('[inert]')).toBeNull()
    expect(shellBody()?.closest('[inert]')).toBeNull()
  })

  it('focuses a single event’s title on a phone, not its back arrow', async () => {
    phone = mockPhone(true)
    await open(`/day/2026-05-18?event=${key(git)}`)

    await vi.waitFor(() =>
      expect(document.activeElement).toBe(screen.getByRole('heading', { name: git?.title }))
    )
  })

  it('keeps the details on top of the phone’s Settings tab, and hands it focus when they close', async () => {
    phone = mockPhone(true)
    const { router, settle } = await open(`/day/2026-05-18?settings=logs&event=${key(git)}`)

    const settings = await vi.waitFor(() => screen.getByRole('region', { name: 'Settings' }))
    await vi.waitFor(() => expect(screen.getByRole('dialog', { name: git?.title })).toBeTruthy())
    expect(settings.closest('[inert]')).not.toBeNull()

    fireEvent.keyDown(document, { key: 'Escape' })
    await vi.waitFor(() => expect(router.url).toBe('/day/2026-05-18?settings=logs'))
    await settle()
    expect(settings.closest('[inert]')).toBeNull()
    await vi.waitFor(() =>
      expect(document.activeElement).toBe(screen.getByRole('heading', { name: 'Logs' }))
    )

    fireEvent.keyDown(document, { key: 'Escape' })
    await vi.waitFor(() => expect(router.url).toBe('/day/2026-05-18'))
  })

  it(
    'hands focus to the details when the settings dialog over them closes',
    async () => {
      phone = mockPhone(true)
      const { router, settle } = await open(`/day/2026-05-18?settings=logs&event=${key(git)}`)
      await vi.waitFor(() => expect(screen.getByRole('region', { name: 'Settings' })).toBeTruthy())

      // Focus on nothing (a click on the page): the dialog has nothing to give it back to.
      if (document.activeElement instanceof HTMLElement) document.activeElement.blur()
      // Wider: the settings move from the phone's tab into the dialog, over the details.
      phone.set(false)
      await vi.waitFor(
        () => expect(document.querySelector('mat-dialog-container')).not.toBeNull(),
        SLOW
      )
      fireEvent.keyDown(document.body, { key: 'Escape', keyCode: 27 })
      await vi.waitFor(() => expect(router.url).toBe(`/day/2026-05-18?event=${key(git)}`))
      await settle()

      await vi.waitFor(() =>
        expect(document.activeElement).toBe(screen.getByRole('heading', { name: git?.title }))
      )
    },
    SLOW_TEST
  )

  it(
    'keeps focus in the settings dialog while an update takes the open event away and brings it back',
    async () => {
      const group = `/day/2026-05-18?events=${key(git)}&events=${key(jira)}&event=${key(jira)}`
      const { router, settle, backend } = await open(`${group}&settings=general`)
      const dialog = await vi.waitFor(() => {
        const container = document.querySelector('mat-dialog-container')
        if (!(container instanceof HTMLElement)) throw new Error('no settings dialog yet')
        return container
      }, SLOW)
      const appearance = await within(dialog).findByRole('button', { name: /Appearance/ }, SLOW)
      appearance.focus()
      const details = document.querySelector('app-event-detail')
      const title = jira?.title
      if (!(details instanceof HTMLElement) || title === undefined) throw new Error('no details')
      expect(details.hasAttribute('inert')).toBe(true)

      backend.activityResponse = { ...activityLog, entries: activityLog.entries.slice(0, 1) }
      backend.publish()
      await screen.findByText('This event is not in the collected range', {}, SLOW)
      await settle()
      expect(document.activeElement).toBe(appearance)

      backend.activityResponse = activityLog
      backend.publish()
      // By its text: the dialog hides what lies under it from the accessibility tree, the details
      // too, so their heading has no role until the dialog closes.
      await within(details).findByText(title, { selector: '.flyout-title' }, SLOW)
      await settle()
      expect(document.activeElement).toBe(appearance)

      fireEvent.keyDown(document.body, { key: 'Escape', keyCode: 27 })
      await vi.waitFor(() => expect(router.parseUrl(router.url)).toEqual(router.parseUrl(group)))
      await settle()
      expect(details.hasAttribute('inert')).toBe(false)
      await vi.waitFor(() =>
        expect(document.activeElement).toBe(screen.getByRole('heading', { name: title }))
      )
    },
    SLOW_TEST
  )

  it('hands the splash off once its header is in the page', async () => {
    const splash = document.createElement('div')
    splash.id = 'splash'
    document.body.append(splash)

    await open('/timeline')

    expect(splash.hasAttribute('data-dismissing')).toBe(true)
  })
})

describe('Navigation motion, rendered', () => {
  let phone: ReturnType<typeof mockPhone> | null = null
  let motion: ReturnType<typeof holdMotion> | null = null
  afterEach(() => {
    phone?.restore()
    phone = null
    motion?.restore()
    motion = null
  })

  it('slides a group’s list out as one of its events comes in, and focuses the event', async () => {
    phone = mockPhone(true)
    const group = `/day/2026-05-18?events=${key(git)}&events=${key(jira)}`
    const { router, settle } = await open(group)
    const list = screen
      .getByRole('heading', { name: '2 overlapping events' })
      .closest('.flyout-page')
    motion = holdMotion()

    await router.navigateByUrl(`${group}&event=${key(jira)}`)
    await settle()

    const event = screen.getByRole('heading', { name: jira?.title }).closest('.flyout-page')
    expect(list?.isConnected).toBe(true)
    expect(list?.hasAttribute('inert')).toBe(true)
    expect(classesOf(list)).toEqual(expect.arrayContaining(['page-leave', 'motion-page-in']))
    expect(classesOf(event)).toEqual(expect.arrayContaining(['page-enter', 'motion-page-in']))
    await vi.waitFor(() =>
      expect(document.activeElement).toBe(screen.getByRole('heading', { name: jira?.title }))
    )

    motion.finish()
    await vi.waitFor(() => expect(list?.isConnected).toBe(false))
  })

  it('swaps the details in place when a collection takes their event away and brings it back', async () => {
    phone = mockPhone(true)
    const { router, settle, backend } = await open('/day/2026-05-18')
    await router.navigateByUrl(`/day/2026-05-18?event=${key(git)}`)
    await settle()
    const title = await screen.findByRole('heading', { name: git?.title })
    await vi.waitFor(() => expect(document.activeElement).toBe(title))
    motion = holdMotion()

    backend.activityResponse = { ...activityLog, entries: activityLog.entries.slice(1) }
    backend.publish()
    await screen.findByText('This event is not in the collected range')
    await settle()

    expect(title.isConnected).toBe(false)
    expect(document.querySelector('.page-enter, .page-leave')).toBeNull()
    expect(document.activeElement).not.toBe(document.body)
    expect(document.activeElement?.closest('.flyout')).not.toBeNull()

    backend.activityResponse = activityLog
    backend.publish()
    await screen.findByRole('heading', { name: git?.title })
    await settle()
    expect(document.querySelector('.page-enter, .page-leave')).toBeNull()
  })

  it('fades the calendar view out inert as another comes, and hands focus on from it', async () => {
    const { settle } = await open('/week/2026-05-18')
    const week = document.querySelector('app-week-view')
    const day = within(week instanceof HTMLElement ? week : document.body).getByRole('button', {
      name: /^Open day view for Mon/,
    })
    day.focus()
    motion = holdMotion()

    fireEvent.click(day)
    await vi.waitFor(() => expect(document.querySelector('app-day-view')).not.toBeNull())
    await settle()

    expect(week?.isConnected).toBe(true)
    expect(week?.hasAttribute('inert')).toBe(true)
    expect(classesOf(week)).toEqual(expect.arrayContaining(['view-leave', 'motion-switch']))
    expect(classesOf(document.querySelector('app-day-view'))).toEqual(
      expect.arrayContaining(['view-enter', 'motion-switch'])
    )
    expect(document.activeElement).toBe(screen.getByRole('main'))

    motion.finish()
    await vi.waitFor(() => expect(week?.isConnected).toBe(false))
  })
})

@Component({ template: 'day' })
class DayStub {}

describe('A view whose code does not load as the app starts', () => {
  it('still brings up the shell, the navigation and a way to reload, and lets the splash go', async () => {
    const splash = document.createElement('div')
    splash.id = 'splash'
    document.body.append(splash)
    const blocked = new TypeError('Failed to fetch dynamically imported module')
    const routes = provideRouter([
      {
        path: '',
        component: ShellLayoutComponent,
        children: [
          {
            path: '',
            component: ViewsLayoutComponent,
            children: [
              { path: 'timeline', loadComponent: loadView(() => Promise.reject(blocked)) },
              { path: 'day/:date', component: DayStub },
            ],
          },
        ],
      },
    ])
    try {
      const { router } = await open('/timeline', { router: routes })

      expect(router.url).toBe('/timeline')
      expect(screen.getByRole('button', { name: 'Reload the app' })).toBeTruthy()
      expect(TestBed.inject(ActivityService).notice()).toMatchObject({
        origin: 'view',
        failure: { cause: blocked },
      })
      expect(splash.hasAttribute('data-dismissing')).toBe(true)

      const views = within(screen.getByRole('navigation', { name: 'Views' }))
      fireEvent.click(views.getByRole('link', { name: 'Day' }))
      await screen.findByText('day')
    } finally {
      splash.remove()
    }
  })
})
