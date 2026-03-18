import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'fs'
import { join } from 'path'

const CACHE_DIR = '.cache'
const TTL_MS = parseInt(process.env.CACHE_TTL_MINUTES ?? '60', 10) * 60 * 1000

interface CacheFile<T> {
  savedAt: number
  data: T
}

function cacheFilePath(key: string): string {
  // Whitelist: only alphanumerics, hyphens, underscores — prevents path traversal
  const safe = key.replace(/[^a-zA-Z0-9\-_]/g, '_')
  return join(CACHE_DIR, `${safe}.json`)
}

export function getCached<T>(key: string): T | null {
  const filePath = cacheFilePath(key)
  if (!existsSync(filePath)) return null
  try {
    const raw = readFileSync(filePath, 'utf-8')
    const cached = JSON.parse(raw) as CacheFile<T>
    if (Date.now() - cached.savedAt > TTL_MS) return null   // expired
    return cached.data
  } catch {
    return null
  }
}

export function setCached<T>(key: string, data: T): void {
  if (!existsSync(CACHE_DIR)) mkdirSync(CACHE_DIR, { recursive: true })
  const payload: CacheFile<T> = { savedAt: Date.now(), data }
  writeFileSync(cacheFilePath(key), JSON.stringify(payload))
}
