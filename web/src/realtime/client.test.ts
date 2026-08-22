import { describe, expect, it } from 'vitest'
import { resolveRealtimeUrl } from './client'

function location(overrides: Partial<Pick<Location, 'hostname' | 'protocol' | 'host'>>): Location {
  return {
    hostname: '',
    protocol: 'http:',
    host: '',
    ...overrides,
  } as unknown as Location
}

describe('resolveRealtimeUrl', () => {
  it('resolves an app.* tunnel/production hostname to wss://api.<rest>', () => {
    const url = resolveRealtimeUrl(
      location({ hostname: 'app.tarams.org', protocol: 'https:', host: 'app.tarams.org' }),
    )
    expect(url).toBe('wss://api.tarams.org/connection/websocket')
  })

  it('resolves loopback http to a relative ws:// URL (Vite-proxied)', () => {
    const url = resolveRealtimeUrl(location({ hostname: '127.0.0.1', protocol: 'http:', host: '127.0.0.1:5173' }))
    expect(url).toBe('ws://127.0.0.1:5173/connection/websocket')
  })

  it('resolves a non-app.* https hostname to wss:// on the same host', () => {
    const url = resolveRealtimeUrl(location({ hostname: 'crm.example.com', protocol: 'https:', host: 'crm.example.com' }))
    expect(url).toBe('wss://crm.example.com/connection/websocket')
  })
})
