/**
 * Whether two JSON values (as parsed from a response) are equal, comparing objects and arrays by
 * content. Cheaper than comparing their `JSON.stringify` output, and it stops at the first
 * difference.
 */
export function jsonEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true
  if (typeof a !== 'object' || typeof b !== 'object' || a === null || b === null) return false
  if (Array.isArray(a)) {
    if (!Array.isArray(b) || a.length !== b.length) return false
    for (let index = 0; index < a.length; index++) {
      if (!jsonEqual(a[index], b[index])) return false
    }
    return true
  }
  if (Array.isArray(b)) return false
  const left: [string, unknown][] = Object.entries(a)
  const right = new Map<string, unknown>(Object.entries(b))
  if (left.length !== right.size) return false
  for (const [key, value] of left) {
    if (!right.has(key) || !jsonEqual(value, right.get(key))) return false
  }
  return true
}
