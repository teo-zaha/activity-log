import 'dotenv/config'
import express from 'express'
import { fileURLToPath } from 'url'
import { dirname, join } from 'path'
import { existsSync } from 'fs'
import { collectGit } from './collect/git.js'
import { collectGitLab } from './collect/gitlab.js'
import { collectJira } from './collect/jira.js'
import { collectOutlook } from './collect/outlook.js'
import { collectSystem } from './collect/system.js'
import { today, daysAgo } from './utils.js'
import type { ActivityLog } from './types.js'

const app = express()
const PORT = Number(process.env.PORT ?? 3000)
const __dir = dirname(fileURLToPath(import.meta.url))
const staticDir = join(__dir, '../dashboard/dist/dashboard/browser')

app.use(express.json())

if (existsSync(staticDir)) {
  app.use(express.static(staticDir))
}

async function runCollect(since: string, until: string): Promise<ActivityLog> {
  const opts = { since, until, output: '' }
  console.log(`\n📊 Collecting ${since} → ${until} ...`)
  const [gitEntries, gitlabEntries, jiraEntries, outlookEntries, systemEntries] = await Promise.all([
    collectGit(opts).catch(e => { console.error('  [git]', (e as Error).message); return [] }),
    collectGitLab(opts).catch(e => { console.error('  [gitlab]', (e as Error).message); return [] }),
    collectJira(opts).catch(e => { console.error('  [jira]', (e as Error).message); return [] }),
    collectOutlook(opts).catch(e => { console.error('  [outlook]', (e as Error).message); return [] }),
    collectSystem(opts).catch(e => { console.error('  [system]', (e as Error).message); return [] }),
  ])
  const entries = [...gitEntries, ...gitlabEntries, ...jiraEntries, ...outlookEntries, ...systemEntries]
    .sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime())
  console.log(`  ✅ ${entries.length} total entries\n`)
  return { meta: { generated: new Date().toISOString(), since, until }, entries }
}

let cachedLog: ActivityLog | null = null
let isCollecting = false

app.get('/api/activity', (_req, res) => {
  if (isCollecting) { res.status(202).json({ collecting: true }); return }
  if (!cachedLog) { res.status(503).json({ error: 'No data yet' }); return }
  res.json(cachedLog)
})

const DATE_RE = /^\d{4}-\d{2}-\d{2}$/

app.post('/api/collect', async (req, res) => {
  const { since, until } = req.body as { since: string; until: string }
  if (!since || !until) { res.status(400).json({ error: 'since and until are required' }); return }
  if (!DATE_RE.test(since) || isNaN(Date.parse(since)) ||
      !DATE_RE.test(until) || isNaN(Date.parse(until))) {
    res.status(400).json({ error: 'since and until must be valid dates in YYYY-MM-DD format' })
    return
  }
  try {
    isCollecting = true
    cachedLog = await runCollect(since, until)
    res.json(cachedLog)
  } catch (e) {
    console.error('Collection error:', e)
    res.status(500).json({ error: 'Collection failed' })
  } finally {
    isCollecting = false
  }
})

app.get('*', (_req, res) => {
  const index = join(staticDir, 'index.html')
  if (existsSync(index)) {
    res.sendFile(index)
  } else {
    res.status(404).send(
      'Dashboard not built yet.\nRun: pnpm install:dashboard && pnpm build:dashboard\nThen restart the server.'
    )
  }
})

app.listen(PORT, async () => {
  console.log(`\n🚀 Activity Log running at http://localhost:${PORT}\n`)
  isCollecting = true
  try {
    cachedLog = await runCollect(daysAgo(30), today())
  } catch (e) {
    console.error('Initial collection failed:', e)
  } finally {
    isCollecting = false
  }
})
