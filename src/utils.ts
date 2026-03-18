export function today(): string {
  return new Date().toISOString().slice(0, 10)
}

export function daysAgo(n: number): string {
  const d = new Date()
  d.setDate(d.getDate() - n)
  return d.toISOString().slice(0, 10)
}

export function resolveRange(range: string): { since: string; until: string } {
  const now = new Date()
  const fmt = (d: Date): string => d.toISOString().slice(0, 10)
  switch (range) {
    case 'today':
      return { since: fmt(now), until: fmt(now) }
    case 'yesterday': {
      const y = new Date(now); y.setDate(y.getDate() - 1)
      return { since: fmt(y), until: fmt(y) }
    }
    case 'week': {
      const s = new Date(now); s.setDate(now.getDate() - now.getDay())
      return { since: fmt(s), until: fmt(now) }
    }
    case 'lastweek': {
      const e = new Date(now); e.setDate(now.getDate() - now.getDay() - 1)
      const s = new Date(e); s.setDate(e.getDate() - 6)
      return { since: fmt(s), until: fmt(e) }
    }
    case 'month':
      return { since: fmt(new Date(now.getFullYear(), now.getMonth(), 1)), until: fmt(now) }
    case 'lastmonth': {
      const s = new Date(now.getFullYear(), now.getMonth() - 1, 1)
      const e = new Date(now.getFullYear(), now.getMonth(), 0)
      return { since: fmt(s), until: fmt(e) }
    }
    default:
      throw new Error(`Unknown range: "${range}". Use today|yesterday|week|lastweek|month|lastmonth`)
  }
}
