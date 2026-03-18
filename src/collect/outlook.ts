import { spawnSync } from 'child_process'
import type { OutlookEntry, CollectOptions } from '../types.js'

// Reads calendar events directly from the locally installed Outlook desktop app
// via PowerShell COM automation — no Azure registration or tokens required.
function assertValidDate(d: string, name: string): void {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(d) || isNaN(Date.parse(d)))
    throw new Error(`[outlook] Invalid date for ${name}: ${JSON.stringify(d)}`)
}

export async function collectOutlook(opts: CollectOptions): Promise<OutlookEntry[]> {
  if (process.platform !== 'win32') {
    console.warn('  [outlook] skipped — Outlook COM automation is only available on Windows')
    return []
  }

  assertValidDate(opts.since, 'since')
  assertValidDate(opts.until, 'until')

  const ps = `
    try {
      $ol = New-Object -ComObject Outlook.Application -ErrorAction Stop
    } catch {
      Write-Error "Outlook is not installed or not running: $_"
      exit 1
    }
    $ns  = $ol.GetNamespace("MAPI")
    $cal = $ns.GetDefaultFolder(9)   # 9 = olFolderCalendar
    $items = $cal.Items
    $items.IncludeRecurrences = $true
    $items.Sort("[Start]")
    $start = [DateTime]::Parse("${opts.since}T00:00:00")
    $end   = [DateTime]::Parse("${opts.until}T23:59:59")
    $filter = "[Start] >= '" + $start.ToString("g") + "' AND [Start] <= '" + $end.ToString("g") + "'"
    $filtered = $items.Restrict($filter)
    $result = @()
    foreach ($ev in $filtered) {
      $result += [PSCustomObject]@{
        subject      = $ev.Subject
        start        = $ev.Start.ToUniversalTime().ToString("o")
        end          = $ev.End.ToUniversalTime().ToString("o")
        location     = $ev.Location
        organizer    = $ev.Organizer
        is_organizer = ($ev.MeetingStatus -eq 1 -or $ev.MeetingStatus -eq 0)
      }
    }
    if ($result.Count -gt 0) { $result | ConvertTo-Json -Compress } else { '[]' }
  `

  const result = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', ps], {
    encoding: 'utf8',
    maxBuffer: 10 * 1024 * 1024,
  })

  if (result.error) throw result.error
  if (result.status !== 0) {
    throw new Error(`PowerShell exited ${result.status}: ${result.stderr?.trim()}`)
  }

  const raw_output = result.stdout?.trim()
  if (!raw_output || raw_output === '[]') return []

  let raw: {
    subject: string
    start: string
    end: string
    location?: string
    organizer?: string
    is_organizer: boolean
  }[]

  try {
    const parsed = JSON.parse(raw_output)
    raw = Array.isArray(parsed) ? parsed : [parsed]
  } catch {
    throw new Error(`Could not parse PowerShell output: ${raw_output.slice(0, 200)}`)
  }

  return raw.map(ev => {
    const durationMs = new Date(ev.end).getTime() - new Date(ev.start).getTime()
    return {
      source: 'outlook' as const,
      type: 'event' as const,
      timestamp: ev.start,
      title: ev.subject ?? '(No title)',
      location: ev.location || undefined,
      organizer: ev.organizer || undefined,
      is_organizer: ev.is_organizer ?? false,
      duration_minutes: Math.round(durationMs / 60000),
    }
  })
}
