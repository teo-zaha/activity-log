// State
let allEntries = []
let activeSources = new Set(['git', 'gitlab', 'jira', 'outlook', 'system'])

// File loading
document.getElementById('file-input').addEventListener('change', e => {
  const f = e.target.files[0]
  if (!f) return
  const r = new FileReader()
  r.onload = ev => {
    try {
      const d = JSON.parse(ev.target.result)
      allEntries = d.entries || d
      if (d.meta) {
        const s = d.meta.since?.slice(0, 10)
        const u = d.meta.until?.slice(0, 10)
        if (s) document.getElementById('date-from').value = s
        if (u) document.getElementById('date-to').value = u
      }
      applyFilters()
    } catch (err) {
      alert('Could not parse JSON: ' + err.message)
    }
  }
  r.readAsText(f)
})

// Helpers
function fmt(d) {
  return d.toISOString().slice(0, 10)
}

function esc(s) {
  return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

// Source chip toggle
function toggleSource(el) {
  const s = el.dataset.src
  if (activeSources.has(s)) activeSources.delete(s)
  else activeSources.add(s)
  el.classList.toggle('active')
  applyFilters()
}

// Filtering
function applyFilters() {
  const from = document.getElementById('date-from').value
  const to = document.getElementById('date-to').value
  const q = document.getElementById('search').value.toLowerCase()

  const filtered = allEntries.filter(e => {
    const ts = e.timestamp?.slice(0, 10)
    if (from && ts < from) return false
    if (to && ts > to) return false
    if (!activeSources.has(e.source)) return false
    if (q) {
      const haystack = [
        e.message,
        e.title,
        e.key,
        e.type,
        e.branch,
        e.repo,
        e.location,
        e.organizer,
      ]
        .filter(Boolean)
        .join(' ')
        .toLowerCase()
      if (!haystack.includes(q)) return false
    }
    return true
  })

  updateStats(filtered)
  renderTimeline(filtered)
}

// Stats
function updateStats(entries) {
  document.getElementById('s-total').textContent = entries.length
  document.getElementById('s-git').textContent = entries.filter(x => x.source === 'git').length
  document.getElementById('s-gitlab').textContent = entries.filter(
    x => x.source === 'gitlab'
  ).length
  document.getElementById('s-jira').textContent = entries.filter(x => x.source === 'jira').length
  document.getElementById('s-outlook').textContent = entries.filter(
    x => x.source === 'outlook'
  ).length
  document.getElementById('s-system').textContent = entries.filter(
    x => x.source === 'system'
  ).length
}

// Timeline rendering
function renderTimeline(entries) {
  const tl = document.getElementById('timeline')
  if (!entries.length) {
    tl.innerHTML = '<div class="empty"><p>No events found. Try adjusting the filters.</p></div>'
    return
  }

  // Group by calendar day
  const byDay = {}
  for (const e of entries) {
    const d = e.timestamp?.slice(0, 10) || 'unknown'
    ;(byDay[d] = byDay[d] || []).push(e)
  }

  tl.innerHTML = Object.keys(byDay)
    .sort()
    .reverse()
    .map(d => {
      const label = new Date(d + 'T12:00:00Z').toLocaleDateString('en-GB', {
        weekday: 'long',
        year: 'numeric',
        month: 'long',
        day: 'numeric',
      })
      return `<div class="day-group">
        <div class="day-header">${label} <span style="font-weight:400;color:#bbb">(${byDay[d].length})</span></div>
        ${byDay[d].map(entryHtml).join('')}
      </div>`
    })
    .join('')
}

function entryHtml(e) {
  const time = e.timestamp
    ? new Date(e.timestamp).toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' })
    : ''

  let title = '',
    meta = ''

  if (e.source === 'git') {
    title = esc(e.message || '')
    meta = [e.repo, e.branch ? `branch: ${e.branch}` : '', e.hash?.slice(0, 7)]
      .filter(Boolean)
      .join(' · ')
  } else if (e.source === 'gitlab') {
    title = esc(e.title || e.type || '')
    meta = [e.type, e.target_type].filter(Boolean).join(' · ')
  } else if (e.source === 'jira') {
    title = `<span style="color:#0052cc;font-size:11px;margin-right:6px">${esc(e.key || '')}</span>${esc(e.title || '')}`
    meta = [e.issue_type, e.status].filter(Boolean).join(' · ')
  } else if (e.source === 'outlook') {
    title = esc(e.title || '')
    const dur =
      e.duration_minutes >= 60
        ? `${Math.floor(e.duration_minutes / 60)}h${e.duration_minutes % 60 ? ` ${e.duration_minutes % 60}m` : ''}`
        : e.duration_minutes + 'm'
    meta = [
      dur,
      e.location,
      e.is_organizer ? 'organizer' : 'attendee',
      e.organizer && !e.is_organizer ? `org: ${e.organizer}` : '',
    ]
      .filter(Boolean)
      .join(' · ')
  } else if (e.source === 'system') {
    title = esc(e.label || e.type || '')
    meta = e.type
  }

  return `<div class="entry">
    <span class="badge ${e.source}">${e.source}</span>
    <div class="entry-body">
      <div class="entry-title">${title}</div>
      ${meta ? `<div class="entry-meta">${esc(meta)}</div>` : ''}
    </div>
    <span class="entry-time">${time}</span>
  </div>`
}

// Export summary
function exportSummary() {
  const from = document.getElementById('date-from').value || 'all'
  const to = document.getElementById('date-to').value || 'all'
  const q = document.getElementById('search').value.toLowerCase()

  const filtered = allEntries.filter(e => {
    const ts = e.timestamp?.slice(0, 10)
    if (from !== 'all' && ts < from) return false
    if (to !== 'all' && ts > to) return false
    if (!activeSources.has(e.source)) return false
    if (q) {
      const haystack = [
        e.message,
        e.title,
        e.key,
        e.type,
        e.branch,
        e.repo,
        e.location,
        e.organizer,
      ]
        .filter(Boolean)
        .join(' ')
        .toLowerCase()
      if (!haystack.includes(q)) return false
    }
    return true
  })

  let md = `# Activity Summary — ${from} to ${to}\n\nTotal: ${filtered.length} events\n\n`

  const byDay = {}
  for (const e of filtered) {
    const d = e.timestamp?.slice(0, 10) || 'unknown'
    ;(byDay[d] = byDay[d] || []).push(e)
  }

  for (const d of Object.keys(byDay).sort().reverse()) {
    md += `## ${new Date(d + 'T12:00:00Z').toLocaleDateString('en-GB', {
      weekday: 'long',
      year: 'numeric',
      month: 'long',
      day: 'numeric',
    })}\n\n`

    for (const e of byDay[d]) {
      const t = e.timestamp
        ? new Date(e.timestamp).toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' })
        : ''

      if (e.source === 'git') {
        md += `- **[git]** \`${e.repo}\` ${e.message} (${t})\n`
      } else if (e.source === 'gitlab') {
        md += `- **[gitlab]** ${e.type}: ${e.title || ''} (${t})\n`
      } else if (e.source === 'jira') {
        md += `- **[jira]** ${e.key}: ${e.title} [${e.status}] (${t})\n`
      } else if (e.source === 'outlook') {
        const dur =
          e.duration_minutes >= 60
            ? `${Math.floor(e.duration_minutes / 60)}h${e.duration_minutes % 60 ? ` ${e.duration_minutes % 60}m` : ''}`
            : e.duration_minutes + 'm'
        md += `- **[outlook]** ${e.title}${e.location ? ` @ ${e.location}` : ''} (${t}, ${dur})\n`
      } else if (e.source === 'system') {
        md += `- **[system]** ${e.label} (${t})\n`
      }
    }
    md += '\n'
  }

  const blob = new Blob([md], { type: 'text/markdown' })
  const a = document.createElement('a')
  a.href = URL.createObjectURL(blob)
  a.download = `activity-${from}-${to}.md`
  a.click()
}
