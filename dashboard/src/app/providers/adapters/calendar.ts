import type { CalendarEntry } from '../../generated/activity'
import { meetingInit } from '../meeting'
import { EntryUiAdapter, ProviderUiAdapterDefinition } from '../types'

class CalendarEntryUiAdapter extends EntryUiAdapter<CalendarEntry> {
  constructor(provider: CalendarProviderAdapter, entry: CalendarEntry) {
    // A calendar file does not say whose part the user has.
    super(provider, entry, meetingInit(provider, entry, null))
  }
}

class CalendarProviderAdapter extends ProviderUiAdapterDefinition<
  'calendar',
  CalendarEntry,
  CalendarEntryUiAdapter
> {
  readonly source = 'calendar' as const
  readonly label = 'Calendar'
  readonly providerIcon = 'calendar_month'

  create(entry: CalendarEntry): CalendarEntryUiAdapter {
    return new CalendarEntryUiAdapter(this, entry)
  }
}

export const calendarAdapter = new CalendarProviderAdapter()
