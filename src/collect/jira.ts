import https from 'node:https'
import http from 'node:http'
import type { JiraEntry, CollectOptions } from '../types.js'
import { getCached, setCached } from '../cache.js'

interface JiraIssue {
  key: string
  fields: {
    summary: string
    status: { name: string }
    issuetype: { name: string }
    updated: string
  }
}

/** Perform an HTTP/HTTPS request without touching global TLS settings. */
function jiraFetch(url: string, headers: Record<string, string>, insecure: boolean): Promise<{ ok: boolean; status: number; text(): Promise<string>; json(): Promise<unknown> }> {
  return new Promise((resolve, reject) => {
    const parsed = new URL(url)
    const isHttps = parsed.protocol === 'https:'
    const agent = isHttps
      ? new https.Agent({ rejectUnauthorized: !insecure })
      : new http.Agent()

    const options = {
      hostname: parsed.hostname,
      port: parsed.port || (isHttps ? 443 : 80),
      path: parsed.pathname + parsed.search,
      method: 'GET',
      headers,
      agent,
    }

    const req = (isHttps ? https : http).request(options, res => {
      const chunks: Buffer[] = []
      res.on('data', (chunk: Buffer) => chunks.push(chunk))
      res.on('end', () => {
        const body = Buffer.concat(chunks).toString('utf8')
        const status = res.statusCode ?? 0
        resolve({
          ok: status >= 200 && status < 300,
          status,
          text: () => Promise.resolve(body),
          json: () => Promise.resolve(JSON.parse(body)),
        })
      })
    })

    req.on('error', (err: NodeJS.ErrnoException) => {
      const code = err.code ?? ''
      const isTls = code.includes('CERT') || code.includes('SSL') ||
                    err.message.includes('certificate') || err.message.includes('TLS')
      reject(new Error(
        `[jira] fetch failed — ${code}: ${err.message}` +
        (isTls ? '\n  → Add JIRA_INSECURE=true to your .env to bypass TLS verification' : ''),
      ))
    })

    req.end()
  })
}

export async function collectJira(opts: CollectOptions): Promise<JiraEntry[]> {
  const { JIRA_URL, JIRA_EMAIL, JIRA_TOKEN, JIRA_INSECURE } = process.env
  if (!JIRA_URL || !JIRA_EMAIL || !JIRA_TOKEN) {
    console.warn('  [jira] skipped — JIRA_URL, JIRA_EMAIL or JIRA_TOKEN not set')
    return []
  }

  const insecure = JIRA_INSECURE === 'true'

  const cacheKey = `jira-${opts.since}-${opts.until}`
  const cached = getCached<JiraEntry[]>(cacheKey)
  if (cached) return cached

  // Jira DC PATs use Bearer auth; set JIRA_AUTH_TYPE=basic to use Basic instead
  const authType = (process.env.JIRA_AUTH_TYPE ?? 'bearer').toLowerCase()
  const authHeader = authType === 'basic'
    ? `Basic ${Buffer.from(`${JIRA_EMAIL}:${JIRA_TOKEN}`).toString('base64')}`
    : `Bearer ${JIRA_TOKEN}`
  const headers = { Authorization: authHeader, 'Content-Type': 'application/json' }
  const entries: JiraEntry[] = []
  let startAt = 0
  const maxResults = 50

  while (true) {
    const jql = encodeURIComponent(
      `updatedDate >= "${opts.since}" AND updatedDate <= "${opts.until}" AND (assignee = currentUser() OR reporter = currentUser())`
    )
    const url = `${JIRA_URL}/rest/api/2/search?jql=${jql}&startAt=${startAt}&maxResults=${maxResults}&fields=summary,status,issuetype,updated`
    const res = await jiraFetch(url, headers, insecure)
    if (!res.ok) throw new Error(`[jira] API error ${res.status}: ${await res.text()}`)
    const data = (await res.json()) as { issues: JiraIssue[] }
    const issues = data.issues ?? []
    if (!issues.length) break
    for (const issue of issues) {
      entries.push({
        source: 'jira', type: 'issue_updated',
        timestamp: issue.fields.updated,
        key: issue.key, title: issue.fields.summary,
        status: issue.fields.status.name,
        issue_type: issue.fields.issuetype.name,
      })
    }
    if (issues.length < maxResults) break
    startAt += maxResults
  }

  setCached(cacheKey, entries)
  return entries
}
