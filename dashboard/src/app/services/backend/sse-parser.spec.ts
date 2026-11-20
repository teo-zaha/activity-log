import { describe, expect, it } from 'vitest'
import { SseParser } from './sse-parser'

/** Every event `chunks` complete, fed to one parser in order. */
function parsed(...chunks: string[]): unknown[] {
  const parser = new SseParser()
  return chunks.flatMap(chunk => parser.push(chunk))
}

describe('SseParser', () => {
  it('reads named events and their data', () => {
    expect(parsed('event: state\ndata: {"a":1}\n\nevent: ping\ndata: {}\n\n')).toEqual([
      { event: 'state', data: '{"a":1}' },
      { event: 'ping', data: '{}' },
    ])
  })

  it('joins data lines with LF and names an unnamed event message', () => {
    expect(parsed('data: one\ndata:two\ndata\n\n')).toEqual([
      { event: 'message', data: 'one\ntwo\n' },
    ])
  })

  it('takes CRLF and CR line ends, also split across chunks', () => {
    expect(parsed('event: a\r\ndata: 1\r\n\r', '\nevent: b\rdata: 2\r', '\r')).toEqual([
      { event: 'a', data: '1' },
      { event: 'b', data: '2' },
    ])
  })

  it('takes an event split anywhere across chunks', () => {
    const text = 'event: activity\ndata: {"revision":4}\n\n'
    const chunks = [...text]

    expect(parsed(...chunks)).toEqual([{ event: 'activity', data: '{"revision":4}' }])
  })

  it('skips comments, ignores id, retry and unknown fields, and drops an event without data', () => {
    expect(
      parsed(':\n: heartbeat\nid: 7\nretry: 1000\nfoo: bar\n\nevent: x\n\ndata: kept\n\n')
    ).toEqual([{ event: 'message', data: 'kept' }])
  })

  it('keeps an event that has not ended for the next chunk, and only one space', () => {
    const parser = new SseParser()

    expect(parser.push('event:  spaced\ndata: half')).toEqual([])
    expect(parser.push('\n\n')).toEqual([{ event: ' spaced', data: 'half' }])
  })
})
