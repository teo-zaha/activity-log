export type Source = 'git' | 'gitlab' | 'jira' | 'outlook' | 'system'

export interface BaseEntry {
  source: Source
  type: string
  timestamp: string
}

export interface GitEntry extends BaseEntry {
  source: 'git'
  type: 'commit'
  repo: string
  hash: string
  message: string
  author: string
  branch: string
}

export interface GitLabEntry extends BaseEntry {
  source: 'gitlab'
  type: string
  title: string
  target_type: string
  raw: Record<string, unknown>
}

export interface JiraEntry extends BaseEntry {
  source: 'jira'
  type: 'issue_updated'
  key: string
  title: string
  status: string
  issue_type: string
}

export interface OutlookEntry extends BaseEntry {
  source: 'outlook'
  type: 'event'
  title: string
  location?: string
  organizer?: string
  is_organizer: boolean
  duration_minutes: number
}

export interface SystemEntry extends BaseEntry {
  source: 'system'
  type: 'boot' | 'shutdown' | 'sleep' | 'wake' | 'unexpected_shutdown' | 'lock' | 'unlock'
  label: string
}

export type ActivityEntry = GitEntry | GitLabEntry | JiraEntry | OutlookEntry | SystemEntry

export interface ActivityLog {
  meta: {
    generated: string
    since: string
    until: string
  }
  entries: ActivityEntry[]
}

export interface CollectOptions {
  since: string
  until: string
  output: string
  gitDirs?: string[]
}
