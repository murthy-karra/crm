// Realtime connection setup (SLICE_003 §6, §10, §11; D-023). Two concerns
// live here: resolving the Centrifugo WebSocket URL for the current page,
// and the production `RealtimeClientFactory` that wraps the real `centrifuge`
// SDK. useRealtime.ts never imports the SDK's `Centrifuge` class itself —
// only this module does — so its Vitest tests stay service-free by injecting
// a fake factory instead (SLICE_003 §10: "Client factory injected (fake
// client in tests, no SDK)").
import { Centrifuge } from 'centrifuge'
import type { RealtimeClient, RealtimeClientFactory } from './useRealtime'

/**
 * Resolves the Centrifugo WebSocket endpoint for the current page (SLICE_003
 * §10). Mirrors api/client.ts's `resolveApiBaseUrl` app.* rule: an
 * "app.<domain>" tunnel/production hostname talks to Centrifugo on its own
 * "api.<domain>" hostname over wss (D-023 §4's path-routed ingress); every
 * other hostname (loopback dev, a bare custom domain) uses the page's own
 * host and protocol, proxied by Vite in dev (vite.config.ts's `/connection`
 * proxy). Parameterized on `Location` so it is unit-testable without a real
 * browser (client.test.ts).
 */
export function resolveRealtimeUrl(location: Location): string {
  const { hostname, protocol, host } = location
  if (hostname.startsWith('app.')) {
    return `wss://api.${hostname.slice('app.'.length)}/connection/websocket`
  }
  const wsProtocol = protocol === 'https:' ? 'wss:' : 'ws:'
  return `${wsProtocol}//${host}/connection/websocket`
}

/**
 * Production `RealtimeClientFactory` (SLICE_003 §10). Not wired to any view
 * yet — step 2 (Today view + AppShell.vue) is the first caller. `getToken`
 * is passed straight through: useRealtime.ts's wrapper already implements
 * the §9 error mapping (401 → the SDK's `UnauthorizedError` + invalidate
 * `me`; anything else rethrown so the SDK retries with backoff).
 */
export const createRealtimeClient: RealtimeClientFactory = ({ url, getToken }) =>
  new Centrifuge(url, { getToken }) as unknown as RealtimeClient
