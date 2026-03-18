export interface LayoutItem<T> {
  entry: T
  col: number
  cols: number
}

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

  return items.map(({ entry, col, cols }) => ({ entry, col, cols }))
}
