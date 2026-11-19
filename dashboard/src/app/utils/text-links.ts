/** An absolute http(s) link with a host; the only kind the dashboard opens. */
export function isWebLink(value: string): boolean {
  if (!URL.canParse(value)) return false
  const url = new URL(value)
  return (url.protocol === 'https:' || url.protocol === 'http:') && url.hostname !== ''
}
