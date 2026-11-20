import { fireEvent, render, screen } from '@testing-library/angular'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { DescriptionBlock, TextSegment } from '../../generated/activity'
import { MockBackendTransport, provideMockBackend } from '../../testing/activity-fixtures'
import { MeetingDescriptionComponent } from './meeting-description.component'

/** The callbacks of the `ResizeObserver`s created, to report a size change by hand. */
const resizeCallbacks: ResizeObserverCallback[] = []

class FakeResizeObserver {
  constructor(callback: ResizeObserverCallback) {
    resizeCallbacks.push(callback)
  }
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}

/** jsdom lays nothing out: pretend the clamped text is `scroll` px tall in a `client` px box. */
function stubLayout(scroll: number, client: number): void {
  vi.spyOn(HTMLElement.prototype, 'scrollHeight', 'get').mockReturnValue(scroll)
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(client)
}

const text = (value: string): TextSegment => ({ kind: 'text', text: value })
const paragraph = (...segments: TextSegment[]): DescriptionBlock => ({
  kind: 'paragraph',
  segments,
})

async function renderDescription(
  blocks: readonly DescriptionBlock[]
): Promise<MockBackendTransport> {
  const backend = new MockBackendTransport()
  await render(MeetingDescriptionComponent, {
    inputs: { blocks },
    providers: [provideMockBackend(backend)],
  })
  return backend
}

describe('MeetingDescriptionComponent', () => {
  beforeEach(() => {
    resizeCallbacks.length = 0
    vi.stubGlobal('ResizeObserver', FakeResizeObserver)
  })

  afterEach(() => {
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it('renders paragraphs, headings, lists, emphasis and links as nodes', async () => {
    const backend = await renderDescription([
      { kind: 'heading', segments: [text('Sprint review')] },
      paragraph(
        text('Line one\nSee '),
        { kind: 'link', text: 'the board', href: 'https://example.org/x' },
        { kind: 'text', text: ' now', strong: true },
        text(' <b>as text</b>')
      ),
      {
        kind: 'list',
        ordered: true,
        items: [
          { depth: 0, segments: [text('First')] },
          { depth: 1, segments: [text('Detail')] },
        ],
      },
    ])

    expect(screen.getByText('Sprint review').closest('p')?.classList).toContain('heading')
    const body = screen.getByText(/Line one/).closest('p')
    expect(body?.textContent).toBe('Line one\nSee the board now <b>as text</b>')
    expect(body?.querySelector('b')).toBeNull()
    expect(screen.getByText('now').classList).toContain('strong')
    const items = screen.getAllByRole('listitem')
    expect(items.map(item => [item.textContent?.trim(), item.dataset['depth']])).toEqual([
      ['First', '0'],
      ['Detail', '1'],
    ])
    expect(items[0].closest('ol')).toBeTruthy()

    fireEvent.click(screen.getByRole('link', { name: 'the board' }))
    expect(backend.openedUrls).toEqual(['https://example.org/x'])
  })

  it('renders images and the placeholders of images without their data', async () => {
    await renderDescription([
      { kind: 'image', src: 'https://example.org/team.jpg', alt: 'The team' },
      { kind: 'image', alt: 'Company Logo' },
      { kind: 'image' },
    ])

    const image = screen.getByRole('img', { name: 'The team' })
    expect(image.getAttribute('src')).toBe('https://example.org/team.jpg')
    expect(image.getAttribute('loading')).toBe('lazy')
    expect(image.getAttribute('decoding')).toBe('async')
    expect(image.getAttribute('referrerpolicy')).toBe('no-referrer')
    const placeholders = Array.from(document.querySelectorAll('.image-placeholder'))
    expect(placeholders.map(placeholder => placeholder.querySelector('span')?.textContent)).toEqual(
      ['Company Logo', 'Image']
    )
    expect(document.body.textContent).not.toContain('[')
  })

  it('names an image without alt text and keeps a decorative one silent', async () => {
    await renderDescription([
      { kind: 'image', src: 'https://example.org/photo.jpg' },
      { kind: 'image', src: 'https://example.org/rule.png', alt: '' },
    ])

    expect(screen.getByRole('img', { name: 'Image' }).getAttribute('src')).toBe(
      'https://example.org/photo.jpg'
    )
    const decorative = document.querySelector<HTMLImageElement>('img[src$="rule.png"]')
    expect(decorative?.alt).toBe('')

    fireEvent.error(decorative as HTMLImageElement)

    expect(await screen.findAllByRole('img')).toHaveLength(1)
    expect(document.querySelectorAll('.image-placeholder')).toHaveLength(0)
  })

  it('shows an image that fails to load as its placeholder', async () => {
    await renderDescription([{ kind: 'image', src: 'https://example.org/gone.png', alt: 'Map' }])

    fireEvent.error(screen.getByRole('img', { name: 'Map' }))

    expect(await screen.findByText('Map')).toBeTruthy()
    expect(screen.queryByRole('img', { name: 'Map' })).toBeNull()
  })

  it('measures the clamp again when an image loads', async () => {
    const scroll = vi.spyOn(HTMLElement.prototype, 'scrollHeight', 'get').mockReturnValue(100)
    vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(216)
    await renderDescription([
      paragraph(text('Short')),
      { kind: 'image', src: 'https://example.org/tall.png', alt: 'Chart' },
    ])
    expect(screen.queryByRole('button')).toBeNull()

    scroll.mockReturnValue(600)
    fireEvent.load(screen.getByRole('img', { name: 'Chart' }))

    expect(await screen.findByRole('button', { name: 'Show more' })).toBeTruthy()
  })

  it('measures the clamp again when its column changes size', async () => {
    const scroll = vi.spyOn(HTMLElement.prototype, 'scrollHeight', 'get').mockReturnValue(185)
    vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(234)
    await renderDescription([{ kind: 'image', src: 'https://example.org/wide.png', alt: 'Map' }])
    expect(screen.queryByRole('button')).toBeNull()
    expect(resizeCallbacks.length).toBeGreaterThan(0)

    scroll.mockReturnValue(320)
    const observer = new ResizeObserver(() => {})
    for (const callback of resizeCallbacks.slice(0, -1)) callback([], observer)

    expect(await screen.findByRole('button', { name: 'Show more' })).toBeTruthy()
  })

  it('offers no toggle when the text fits', async () => {
    stubLayout(100, 100)
    await renderDescription([paragraph(text('Short'))])

    expect(screen.queryByRole('button')).toBeNull()
  })

  it('clamps a long text and toggles between more and less', async () => {
    stubLayout(600, 216)
    await renderDescription([
      paragraph(text(Array.from({ length: 30 }, (_, line) => `Line ${line}`).join('\n'))),
    ])

    const description = screen.getByText(/Line 0/).closest('.description')
    const toggle = await screen.findByRole('button', { name: 'Show more' })
    expect(toggle.getAttribute('aria-expanded')).toBe('false')
    expect(description?.classList.contains('clamped')).toBe(true)

    fireEvent.click(toggle)
    const less = await screen.findByRole('button', { name: 'Show less' })
    expect(less.getAttribute('aria-expanded')).toBe('true')
    expect(description?.classList.contains('clamped')).toBe(false)

    fireEvent.click(less)
    expect(await screen.findByRole('button', { name: 'Show more' })).toBeTruthy()
  })

  it('measures an open description again when its column changes size', async () => {
    // As a browser lays it out: the clamp caps the box at 216 px, open it is as tall as its text.
    let textHeight = 600
    vi.spyOn(HTMLElement.prototype, 'scrollHeight', 'get').mockImplementation(() => textHeight)
    vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockImplementation(function (
      this: HTMLElement
    ) {
      return this.classList.contains('clamped') ? Math.min(textHeight, 216) : textHeight
    })
    const { fixture } = await render(MeetingDescriptionComponent, {
      inputs: {
        blocks: [
          paragraph(text(Array.from({ length: 30 }, (_, line) => `Line ${line}`).join('\n'))),
        ],
      },
      providers: [provideMockBackend(new MockBackendTransport())],
    })
    await fixture.whenStable()
    fireEvent.click(screen.getByRole('button', { name: 'Show more' }))
    await fixture.whenStable()
    expect(screen.getByRole('button', { name: 'Show less' })).toBeTruthy()
    const resized = async (): Promise<void> => {
      const observer = new ResizeObserver(() => {})
      for (const callback of resizeCallbacks.slice(0, -1)) callback([], observer)
      await fixture.whenStable()
    }

    // A wider column: all of it fits the clamp now, so there is nothing to show less of.
    textHeight = 200
    await resized()
    expect(screen.queryByRole('button')).toBeNull()

    textHeight = 600
    await resized()
    expect(screen.getByRole('button', { name: 'Show less' })).toBeTruthy()
  })
})
