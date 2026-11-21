import { resource, type ResourceRef, Service } from '@angular/core'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { BackendOperations, type BackendOperation, type ReadOperation } from './backend-contract'
import { BackendFailure, tauriFailureOf, unsupportedIn } from './backend-failure'
import type { BackendTransport } from './backend-transport'
import { decodeRuntimeMessage } from './runtime-message'
import {
  deliver,
  epochOf,
  type RuntimeEventName,
  type RuntimeMessage,
  type RuntimeStream,
  type RuntimeStreamHandlers,
} from './runtime-stream'
import { DESKTOP_EVENTS, type ExportOutcome, type ExportRequest } from '../../generated/activity'

@Service()
export class TauriBackendTransport implements BackendTransport {
  resource<Res>(read: ReadOperation<Res>): ResourceRef<Res | undefined> {
    return resource({ loader: ({ abortSignal }) => this.call(read, undefined, abortSignal) })
  }

  /** Rejects with a `BackendFailure`, or with the signal's reason once the caller aborts. */
  call<Req, Res>(
    operation: BackendOperation<Req, Res>,
    request: NoInfer<Req>,
    signal?: AbortSignal
  ): Promise<Res> {
    const command = operation.tauriCommand
    if (signal?.aborted) return Promise.reject(signal.reason)
    if (command === null) return Promise.reject(unsupportedIn('the browser', operation.label))
    const args = request === undefined ? undefined : { request }
    const answer = invoke<Res>(command, args).catch((error: unknown) => {
      throw tauriFailureOf(error)
    })
    // An invoke cannot be cancelled: an abort only stops the caller waiting for it.
    return signal === undefined ? answer : Promise.race([answer, abortOf(signal)])
  }

  saveExport(request: ExportRequest): Promise<ExportOutcome> {
    return this.call(BackendOperations.saveExport, request)
  }

  async openUrl(url: string): Promise<void> {
    await this.call(BackendOperations.openUrl, url)
  }

  /**
   * Listens to the `state`, `activity` and `providers` events first, then reads the pair
   * (`get_runtime_state`) and delivers it as the stream's first `state`. Until then only the
   * latest message of each event is kept; once the pair is in they follow it, and then each
   * event as it comes, as long as it is of the pair's host (one queued before a restart is
   * dropped). Every payload, the pair's too, is checked (`decodeRuntimeMessage`); one that does
   * not fit, a listener or the read failing ends the stream. Every listener is removed when it
   * ends, also one that is registered only after, and what it kept is let go.
   */
  openRuntimeStream(handlers: RuntimeStreamHandlers): RuntimeStream {
    let open = true
    const unlisteners: UnlistenFn[] = []
    /** The host of the pair the stream started with; until it is known, `early` keeps messages. */
    let epoch: string | null = null
    const early = new Map<RuntimeEventName, RuntimeMessage>()
    const close = (): void => {
      open = false
      early.clear()
      for (const unlisten of unlisteners.splice(0)) unlisten()
    }
    const lose = (failure: BackendFailure): void => {
      if (!open) return
      close()
      handlers.lost(failure)
    }
    const unreadable = (name: RuntimeEventName): BackendFailure =>
      new BackendFailure('internal', `The desktop app sent a ${name} this dashboard cannot read`)
    const forward = (message: RuntimeMessage): void => {
      if (open && epochOf(message) === epoch) deliver(handlers, message)
    }
    const receive = (name: RuntimeEventName, payload: unknown): void => {
      if (!open) return
      const message = decodeRuntimeMessage(name, payload)
      if (message === null) return lose(unreadable(name))
      if (epoch === null) early.set(name, message)
      else forward(message)
    }
    const register = async (name: RuntimeEventName): Promise<void> => {
      const unlisten = await listen<unknown>(name, event => receive(name, event.payload))
      if (open) unlisteners.push(unlisten)
      else unlisten()
    }
    const start = async (): Promise<void> => {
      await Promise.all([register('state'), register('activity'), register('providers')]).catch(
        (error: unknown) => {
          throw tauriFailureOf(error)
        }
      )
      if (!open) return
      const read: unknown = await this.call(BackendOperations.runtimeState, undefined)
      if (!open) return
      const pair = decodeRuntimeMessage('state', read)
      if (pair?.event !== 'state') return lose(unreadable('state'))
      epoch = pair.payload.activity.epoch
      handlers.state(pair.payload)
      const kept = (['state', 'activity', 'providers'] as const).flatMap(name => {
        const message = early.get(name)
        return message === undefined ? [] : [message]
      })
      early.clear()
      for (const message of kept) forward(message)
    }
    start().catch((error: unknown) =>
      lose(
        error instanceof BackendFailure
          ? error
          : new BackendFailure('internal', 'Following the desktop app failed', { cause: error })
      )
    )
    return { close }
  }

  /** Listens first, so no link falls between the launch link and the first event. */
  async listenForDeepLinks(open: (link: string) => void): Promise<void> {
    await listen<string>(DESKTOP_EVENTS.deepLink, event => open(event.payload)).catch(
      (error: unknown) => {
        throw tauriFailureOf(error)
      }
    )
    const launch = await this.call(BackendOperations.takeLaunchDeepLink, undefined)
    if (launch !== null) open(launch)
  }
}

/**
 * Rejects once `signal` is aborted, with its reason. A plain listener: it is per call, outside
 * any injection context a Signality `listener` needs, and `once` removes it.
 */
function abortOf(signal: AbortSignal): Promise<never> {
  return new Promise((_, reject) => {
    if (signal.aborted) reject(signal.reason)
    else signal.addEventListener('abort', () => reject(signal.reason), { once: true })
  })
}
