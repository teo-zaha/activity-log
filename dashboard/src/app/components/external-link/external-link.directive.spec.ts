import { Component } from '@angular/core'
import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { ActivityBackendClient } from '../../services/backend/backend-client'
import { BackendFailure } from '../../services/backend/backend-failure'
import type { Failure } from '../../services/failure'
import { ToastService } from '../../services/toast.service'
import { ExternalLinkDirective } from './external-link.directive'

@Component({
  imports: [ExternalLinkDirective],
  template: `<a appExternalLink="https://example.org/issue/1">The issue</a>`,
})
class LinkHost {}

async function setup(openUrl: (url: string) => Promise<void>): Promise<{
  failure: ReturnType<typeof vi.fn<(failure: Failure) => void>>
}> {
  const failure = vi.fn<(failure: Failure) => void>()
  await render(LinkHost, {
    providers: [
      { provide: ActivityBackendClient, useValue: { openUrl } },
      { provide: ToastService, useValue: { failure } },
    ],
  })
  return { failure }
}

describe('ExternalLinkDirective', () => {
  it('opens the link through the backend', async () => {
    const openUrl = vi.fn((): Promise<void> => Promise.resolve())
    const { failure } = await setup(openUrl)

    await userEvent.click(screen.getByRole('link', { name: 'The issue' }))

    expect(openUrl).toHaveBeenCalledWith('https://example.org/issue/1')
    await Promise.resolve()
    expect(failure).not.toHaveBeenCalled()
  })

  it('says so when the link does not open, the error kept as its cause', async () => {
    const refused = new BackendFailure('unsupported', 'no browser to open it in')
    const { failure } = await setup(() => Promise.reject(refused))

    await userEvent.click(screen.getByRole('link', { name: 'The issue' }))

    await vi.waitFor(() => expect(failure).toHaveBeenCalledTimes(1))
    expect(failure.mock.calls[0][0]).toMatchObject({ kind: 'backend', cause: refused })
  })
})
