// The dev server (`pnpm dashboard:dev`) sends `/api` to the dev API (`pnpm api:dev`). When the
// API's answer breaks off before its end (the API was killed), the proxy would keep the
// browser's answer open, and an `EventSource` on `GET /api/events` would wait on a stream that
// is gone; ending the browser's answer too lets it connect again.
export default {
  '/api': {
    target: 'http://localhost:3001',
    secure: false,
    logLevel: 'info',
    configure: proxy => {
      proxy.on('proxyRes', (upstream, _request, downstream) => {
        upstream.on('close', () => {
          if (!upstream.complete) downstream.destroy()
        })
      })
    },
  },
}
