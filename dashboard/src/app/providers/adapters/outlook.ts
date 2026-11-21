import type { OutlookEntry } from '../../generated/activity'
import { meetingInit, type MeetingRole } from '../meeting'
import { EntryUiAdapter, ProviderUiAdapterDefinition } from '../types'

/** The user's role in the event; none when Outlook did not say. */
function roleOf(entry: OutlookEntry): MeetingRole | null {
  if (entry.isOrganizer === undefined) return null
  return entry.isOrganizer ? 'Organizer' : 'Attendee'
}

class OutlookEntryUiAdapter extends EntryUiAdapter<OutlookEntry> {
  constructor(provider: OutlookProviderAdapter, entry: OutlookEntry) {
    super(provider, entry, meetingInit(provider, entry, roleOf(entry)))
  }
}

class OutlookProviderAdapter extends ProviderUiAdapterDefinition<
  'outlook',
  OutlookEntry,
  OutlookEntryUiAdapter
> {
  readonly source = 'outlook' as const
  readonly label = 'Outlook'
  readonly providerIcon = 'event'

  create(entry: OutlookEntry): OutlookEntryUiAdapter {
    return new OutlookEntryUiAdapter(this, entry)
  }
}

export const outlookAdapter = new OutlookProviderAdapter()
