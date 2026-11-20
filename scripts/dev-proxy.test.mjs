// The dashboard dev server's proxy (`dashboard/proxy.conf.mjs`): when the API's answer breaks
// off (the API was killed), the browser's answer ends too, so an open `EventSource` sees its
// stream end and connects again. Run with `node --test scripts/`.
import assert from 'node:assert/strict'
import { EventEmitter } from 'node:events'
import { test } from 'node:test'
import config from '../dashboard/proxy.conf.mjs'

/** The proxy's view of one proxied request: the API's answer and the browser's. */
function proxied() {
  const proxy = new EventEmitter()
  config['/api'].configure(proxy, config['/api'])
  const upstream = Object.assign(new EventEmitter(), { complete: false })
  const downstream = {
    destroyed: 0,
    destroy() {
      this.destroyed++
    },
  }
  proxy.emit('proxyRes', upstream, {}, downstream)
  return { upstream, downstream }
}

test('an answer of the API that breaks off ends the browser’s answer', () => {
  const { upstream, downstream } = proxied()

  upstream.emit('close')

  assert.equal(downstream.destroyed, 1)
})

test('an answer of the API that ends as it should is left to end by itself', () => {
  const { upstream, downstream } = proxied()

  upstream.complete = true
  upstream.emit('close')

  assert.equal(downstream.destroyed, 0)
})

test('the proxy still sends /api to the dev API', () => {
  assert.equal(config['/api'].target, 'http://localhost:3001')
})
