import { type EnvironmentProviders, inject, provideAppInitializer, Service } from '@angular/core'
import { NavigationCancel, NavigationEnd, NavigationError, Router } from '@angular/router'
import { filter, firstValueFrom } from 'rxjs'
import { BackendTransport } from './backend/backend-transport'
import { DeepLinkService } from './deep-link.service'
import { failureWhile } from './failure'

/**
 * Hands the desktop app's `activity-log:` links to `DeepLinkService.open`: the one that launched
 * the app, then each one that arrives while it runs. The browser has none.
 */
@Service()
export class DeepLinkInboxService {
  private readonly transport = inject(BackendTransport)
  private readonly deepLinks = inject(DeepLinkService)
  private readonly router = inject(Router)

  /** Never rejects: without the listener the app runs on, and the log says why. */
  async start(): Promise<void> {
    try {
      await this.transport.listenForDeepLinks(link => void this.open(link))
    } catch (error) {
      const failed = failureWhile('Listening for activity-log: links', error)
      console.warn(failed.message, failed.cause)
    }
  }

  /** After the first navigation, so the launch URL's own route does not replace the link's. */
  private async open(link: string): Promise<void> {
    if (!this.router.navigated) {
      await firstValueFrom(
        this.router.events.pipe(
          filter(
            event =>
              event instanceof NavigationEnd ||
              event instanceof NavigationCancel ||
              event instanceof NavigationError
          )
        )
      )
    }
    await this.deepLinks.open(link)
  }
}

/** Starts listening for deep links when the app starts. */
export function provideDeepLinks(): EnvironmentProviders {
  return provideAppInitializer(() => {
    void inject(DeepLinkInboxService).start()
  })
}
