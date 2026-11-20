/** One server-sent event: its name (`message` when the stream names none) and its data. */
export interface SseEvent {
  readonly event: string
  readonly data: string
}

/**
 * Reads server-sent events out of text that arrives in chunks of any size, as the HTML
 * standard frames them: lines end in LF, CR or CRLF (also split across two chunks); a blank
 * line ends an event; `event:` names it, each `data:` line adds a line to its data; one space
 * after the colon is dropped; lines starting with `:` are comments; `id:`, `retry:` and fields
 * it does not know are ignored; an event without data is not dispatched.
 */
export class SseParser {
  /** The start of a line whose end has not come yet. */
  private line = ''
  /** The last chunk ended in CR: an LF opening the next one belongs to that line end. */
  private afterCr = false
  private event = ''
  private data: string[] = []

  /** The events `chunk` completes, in order. */
  push(chunk: string): SseEvent[] {
    const events: SseEvent[] = []
    let start = 0
    if (this.afterCr && chunk.startsWith('\n')) start = 1
    this.afterCr = false
    for (let index = start; index < chunk.length; index++) {
      const char = chunk[index]
      if (char !== '\n' && char !== '\r') continue
      this.take(this.line + chunk.slice(start, index), events)
      this.line = ''
      if (char === '\r') {
        if (index + 1 === chunk.length) this.afterCr = true
        else if (chunk[index + 1] === '\n') index++
      }
      start = index + 1
    }
    this.line += chunk.slice(start)
    return events
  }

  private take(line: string, events: SseEvent[]): void {
    if (line === '') return this.dispatch(events)
    if (line.startsWith(':')) return
    const colon = line.indexOf(':')
    const field = colon === -1 ? line : line.slice(0, colon)
    let value = colon === -1 ? '' : line.slice(colon + 1)
    if (value.startsWith(' ')) value = value.slice(1)
    if (field === 'event') this.event = value
    else if (field === 'data') this.data.push(value)
  }

  private dispatch(events: SseEvent[]): void {
    if (this.data.length > 0) {
      events.push({ event: this.event || 'message', data: this.data.join('\n') })
    }
    this.event = ''
    this.data = []
  }
}
