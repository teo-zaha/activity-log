import { refusedRange, retryable } from '../../services/collection-failure'
import type { CollectionStatus } from '../../services/types'

/** What decides why the Timeline's list is empty. */
export interface TimelineEmptyInput {
  /** The filter, trimmed. */
  readonly query: string
  readonly status: CollectionStatus
  /** Whether any source is usable or still being checked. */
  readonly hasSources: boolean
  /** Whether the source filter shows any connection. */
  readonly anyShown: boolean
}

/** What the empty state offers to do about it. */
export type TimelineEmptyAction = { readonly kind: 'clear-search' }

export interface TimelineEmptyState {
  readonly icon: string
  readonly title: string
  readonly message: string
  readonly action: TimelineEmptyAction | null
}

/** Why the Timeline's list is empty, and what to do about it. */
export function timelineEmptyState(input: TimelineEmptyInput): TimelineEmptyState {
  const { query, status } = input
  if (status.kind === 'failed') {
    const message = refusedRange(status)
      ? 'The backend refused to collect this range. Choose another one.'
      : !retryable(status)
        ? 'The backend refused to show its activity; check source status in Settings.'
        : 'The last collection did not finish. Retry above, or check source status in Settings.'
    return { icon: 'cloud_off', title: 'No activity loaded', message, action: null }
  }
  if (query) {
    return {
      icon: 'search_off',
      title: `No matches for “${query}”`,
      message: 'Try a different term, or filter by repo, branch, ticket key or author.',
      action: { kind: 'clear-search' },
    }
  }
  if (!input.hasSources) {
    return {
      icon: 'power_off',
      title: 'No sources are ready',
      message: 'Open Settings to see why each source is unavailable and configure it.',
      action: null,
    }
  }
  if (!input.anyShown) {
    return {
      icon: 'filter_alt_off',
      title: 'All sources are hidden',
      message: 'Turn on at least one source in the sidebar to see its activity.',
      action: null,
    }
  }
  return {
    icon: 'event_busy',
    title: 'Nothing recorded in this range',
    message: 'Pick a wider date range and collect again.',
    action: null,
  }
}
