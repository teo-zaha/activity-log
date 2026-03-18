import { spawnSync } from 'child_process'
import type { SystemEntry, CollectOptions } from '../types.js'

// System log: power state events (no special permissions needed)
const SYSTEM_EVENT_MAP: Record<number, { type: SystemEntry['type']; label: string }> = {
  6005: { type: 'boot',                label: 'System started' },
  6006: { type: 'shutdown',            label: 'System shutdown' },
  6008: { type: 'unexpected_shutdown', label: 'Unexpected shutdown' },
  42:   { type: 'sleep',               label: 'System entering sleep' },
  107:  { type: 'wake',                label: 'System resumed from sleep' },
}

// Security log: precise lock/unlock (requires audit policy + admin/Event Log Readers group)
const SECURITY_EVENT_MAP: Record<number, { type: SystemEntry['type']; label: string }> = {
  4800: { type: 'lock',   label: 'Workstation locked' },
  4801: { type: 'unlock', label: 'Workstation unlocked' },
}

function assertValidDate(d: string, name: string): void {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(d) || isNaN(Date.parse(d)))
    throw new Error(`[system] Invalid date for ${name}: ${JSON.stringify(d)}`)
}

function queryLog(
  logName: string,
  ids: number[],
  since: string,
  until: string,
): { id: number; ts: string }[] {
  const ps = `
    $ids   = @(${ids.join(', ')})
    $start = [DateTime]'${since}T00:00:00'
    $end   = [DateTime]'${until}T23:59:59'
    $events = Get-WinEvent -FilterHashtable @{LogName='${logName}'; Id=$ids} -ErrorAction SilentlyContinue |
      Where-Object { $_.TimeCreated -ge $start -and $_.TimeCreated -le $end } |
      Select-Object @{n='id';e={$_.Id}}, @{n='ts';e={$_.TimeCreated.ToUniversalTime().ToString('o')}}
    if ($events) { $events | ConvertTo-Json -Compress } else { '[]' }
  `

  const result = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', ps], {
    encoding: 'utf8',
    maxBuffer: 10 * 1024 * 1024,
  })

  if (result.error) throw result.error
  if (result.status !== 0) {
    throw new Error(`PowerShell exited ${result.status}: ${result.stderr?.trim()}`)
  }

  const out = result.stdout?.trim()
  if (!out || out === '[]') return []

  const parsed = JSON.parse(out)
  return Array.isArray(parsed) ? parsed : [parsed]
}

export async function collectSystem(opts: CollectOptions): Promise<SystemEntry[]> {
  if (process.platform !== 'win32') {
    console.warn('  [system] skipped — Windows Event Log is only available on Windows')
    return []
  }

  assertValidDate(opts.since, 'since')
  assertValidDate(opts.until, 'until')

  // Query System log (always available)
  const systemRaw = queryLog(
    'System',
    Object.keys(SYSTEM_EVENT_MAP).map(Number),
    opts.since,
    opts.until,
  )

  // Also try Security log 4800/4801 for more precise timestamps (requires audit policy)
  let secRaw: { id: number; ts: string }[] = []
  try {
    secRaw = queryLog(
      'Security',
      Object.keys(SECURITY_EVENT_MAP).map(Number),
      opts.since,
      opts.until,
    )
  } catch (err) {
    console.warn(`  [system] Security log unavailable: ${err instanceof Error ? err.message : String(err)}`)
  }

  // Prefer Security events when available; fall back to TerminalServices
  const lockRaw = secRaw
  const lockMap  = SECURITY_EVENT_MAP
  const lockSrc  = secRaw.length > 0 ? 'security' : 'none'

  if (lockSrc === 'none') {
    console.warn(
      '  [system] lock/unlock events unavailable.\n' +
      '  To enable precise events, run PowerShell as admin:\n' +
      "    auditpol /set /subcategory:'Other Logon/Logoff Events' /success:enable",
    )
  } else {
    console.log(`  [system] lock/unlock via ${lockSrc} (${lockRaw.length} events)`)
  }

  return [
    ...systemRaw.map(ev => ({ id: ev.id, ts: ev.ts, map: SYSTEM_EVENT_MAP })),
    ...lockRaw.map(ev => ({ id: ev.id, ts: ev.ts, map: lockMap })),
  ]
    .map(({ id, ts, map }) => {
      const def = (map as typeof SYSTEM_EVENT_MAP)[id]
      return {
        source: 'system' as const,
        type:   def?.type  ?? 'boot',
        timestamp: ts,
        label: def?.label ?? `Event ${id}`,
      }
    })
    .sort((a, b) => a.timestamp.localeCompare(b.timestamp))
}
