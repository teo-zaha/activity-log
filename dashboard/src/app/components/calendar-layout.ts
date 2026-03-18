export interface LayoutItem<T> {
  kind: 'event'
  entry: T
  col: number
  cols: number
}

export interface OverflowItem<T> {
  kind: 'overflow'
  entries: T[]
  col: number
  cols: number
  top: number
  height: number
}

export type CalendarItem<T> = LayoutItem<T> | OverflowItem<T>

/**
 * Assigns non-overlapping columns to events so they can be rendered side-by-side.
 * Events that don't overlap take the full width (cols=1, col=0).
 */
export function layoutEvents<T>(
  entries: T[],
  getTop: (e: T) => number,
  getHeight: (e: T) => number,
  minHeight = 22,
): LayoutItem<T>[] {
  if (!entries.length) return []

  const items = entries.map(e => ({
    entry: e,
    top: getTop(e),
    end: getTop(e) + Math.max(getHeight(e), minHeight),
    col: 0,
    cols: 1,
  }))

  items.sort((a, b) => a.top - b.top)

  // Greedy column assignment: place each event in the first free column
  const colEnds: number[] = []
  for (const item of items) {
    const c = colEnds.findIndex(end => end <= item.top)
    item.col = c === -1 ? colEnds.length : c
    colEnds[item.col] = item.end
  }

  // Propagate cols count through transitive overlap clusters
  const n = items.length
  const visited = new Set<number>()

  const getCluster = (start: number): number[] => {
    const cluster: number[] = []
    const stack = [start]
    while (stack.length) {
      const idx = stack.pop()!
      if (visited.has(idx)) continue
      visited.add(idx)
      cluster.push(idx)
      for (let j = 0; j < n; j++) {
        if (!visited.has(j) && items[j].top < items[idx].end && items[j].end > items[idx].top) {
          stack.push(j)
        }
      }
    }
    return cluster
  }

  for (let i = 0; i < n; i++) {
    if (!visited.has(i)) {
      const cluster = getCluster(i)
      const maxCols = Math.max(...cluster.map(j => items[j].col)) + 1
      for (const j of cluster) items[j].cols = maxCols
    }
  }

  return items.map(({ entry, col, cols }) => ({ kind: 'event' as const, entry, col, cols }))
}

/**
 * Like layoutEvents but caps each overlap cluster at maxVisible columns.
 * Excess events are collapsed into a single OverflowItem at the last column.
 */
export function layoutEventsWithOverflow<T>(
  entries: T[],
  getTop: (e: T) => number,
  getHeight: (e: T) => number,
  minHeight = 22,
  maxVisible = 4,
): CalendarItem<T>[] {
  if (!entries.length) return []

  const all = entries.map(e => ({
    entry: e,
    top: getTop(e),
    end: getTop(e) + Math.max(getHeight(e), minHeight),
    col: 0,
    cols: 1,
  }))

  all.sort((a, b) => a.top - b.top)

  const colEnds: number[] = []
  for (const item of all) {
    const c = colEnds.findIndex(end => end <= item.top)
    item.col = c === -1 ? colEnds.length : c
    colEnds[item.col] = item.end
  }

  const n = all.length
  const visited = new Set<number>()
  const result: CalendarItem<T>[] = []

  const getCluster = (start: number): number[] => {
    const cluster: number[] = []
    const stack = [start]
    while (stack.length) {
      const idx = stack.pop()!
      if (visited.has(idx)) continue
      visited.add(idx)
      cluster.push(idx)
      for (let j = 0; j < n; j++) {
        if (!visited.has(j) && all[j].top < all[idx].end && all[j].end > all[idx].top) {
          stack.push(j)
        }
      }
    }
    return cluster
  }

  for (let i = 0; i < n; i++) {
    if (!visited.has(i)) {
      const cluster = getCluster(i)
      const rawCols = Math.max(...cluster.map(j => all[j].col)) + 1
      const caps = Math.min(rawCols, maxVisible)

      if (rawCols <= maxVisible) {
        for (const j of cluster) {
          result.push({ kind: 'event', entry: all[j].entry, col: all[j].col, cols: rawCols })
        }
      } else {
        const sorted = [...cluster].sort((a, b) => all[a].col - all[b].col)
        const visibleIdxs  = sorted.filter(j => all[j].col < caps - 1)
        const overflowIdxs = sorted.filter(j => all[j].col >= caps - 1)

        for (const j of visibleIdxs) {
          result.push({ kind: 'event', entry: all[j].entry, col: all[j].col, cols: caps })
        }

        const overflowEntries = overflowIdxs.map(j => all[j].entry)
        const tops   = overflowIdxs.map(j => all[j].top)
        const ends   = overflowIdxs.map(j => all[j].end)
        result.push({
          kind: 'overflow',
          entries: overflowEntries,
          col: caps - 1,
          cols: caps,
          top: Math.min(...tops),
          height: Math.max(...ends) - Math.min(...tops),
        })
      }
    }
  }

  return result
}
