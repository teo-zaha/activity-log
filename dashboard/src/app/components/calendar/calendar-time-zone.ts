import { InjectionToken } from '@angular/core'

/**
 * The IANA zone the calendar views read wall clocks in; undefined for the browser's own, as
 * every other local day is read. Specs set one to draw daylight saving days wherever they run.
 */
export const CALENDAR_TIME_ZONE = new InjectionToken<string | undefined>('CALENDAR_TIME_ZONE', {
  providedIn: 'root',
  factory: (): string | undefined => undefined,
})
