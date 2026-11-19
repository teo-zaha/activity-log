// Zoom levels and scroll anchoring for the Day and Week grids, in px per hour.

export const DEFAULT_HOUR_HEIGHT = 64
/** Button steps, about a quarter apart, so each press changes the scale by a similar amount. */
export const ZOOM_LEVELS = [32, 40, 48, 64, 80, 96, 128, 160, 192] as const
export const MIN_HOUR_HEIGHT = ZOOM_LEVELS[0]
export const MAX_HOUR_HEIGHT = ZOOM_LEVELS[ZOOM_LEVELS.length - 1]
export function clampHourHeight(height: number): number {
  return Math.min(MAX_HOUR_HEIGHT, Math.max(MIN_HOUR_HEIGHT, height))
}

/** Stores the zoom as a number; a stored value that is no number gives the default. */
export const hourHeightSerializer = {
  read: (stored: string): number => {
    const height = Number(stored)
    return Number.isFinite(height) ? clampHourHeight(height) : DEFAULT_HOUR_HEIGHT
  },
  write: (height: number): string => String(height),
}

/** The next button level above (`1`) or below (`-1`) `height`, which may lie between levels. */
export function stepHourHeight(height: number, direction: 1 | -1): number {
  const next =
    direction > 0
      ? ZOOM_LEVELS.find(level => level > height + 0.5)
      : [...ZOOM_LEVELS].reverse().find(level => level < height - 0.5)
  return next ?? (direction > 0 ? MAX_HOUR_HEIGHT : MIN_HOUR_HEIGHT)
}

export function zoomPercent(height: number): string {
  return `${Math.round((height / DEFAULT_HOUR_HEIGHT) * 100)}%`
}

/**
 * The scroll position that keeps the time at `anchorY` (px below the scroller's top edge) in
 * place when hours change from `from` to `to` px. `gridTop` is where the hour grid starts in
 * the scrolled content; whatever sits above it (the all-day row) does not scale.
 */
export function anchoredScrollTop(
  scrollTop: number,
  gridTop: number,
  anchorY: number,
  from: number,
  to: number
): number {
  const hours = (scrollTop + anchorY - gridTop) / from
  return Math.max(0, gridTop + hours * to - anchorY)
}
