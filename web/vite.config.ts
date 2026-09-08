/// <reference types="vitest/config" />
import { fileURLToPath, URL } from 'node:url'
import { defineConfig, loadEnv } from 'vite'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'

// Repo-root .env holds these variables (D-013), not web/.env.
const repoRoot = fileURLToPath(new URL('..', import.meta.url))

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, repoRoot, '')

  const host = env.CRM_WEB_BIND_ADDR || '127.0.0.1'
  const port = Number(env.CRM_WEB_PORT || 5173)
  const proxyTarget = env.CRM_WEB_API_PROXY_TARGET || 'http://127.0.0.1:3000'
  const realtimeProxyTarget = env.CRM_WEB_REALTIME_PROXY_TARGET || 'http://127.0.0.1:8000'
  // Exact tunnel hostname only, never a wildcard. Unset locally: Vite's
  // built-in default already allows localhost/127.0.0.1.
  const allowedHosts = env.CRM_WEB_ALLOWED_HOSTS ? [env.CRM_WEB_ALLOWED_HOSTS] : undefined

  return {
    envDir: repoRoot,
    plugins: [vue(), tailwindcss()],
    server: {
      host,
      port,
      strictPort: true,
      allowedHosts,
      proxy: {
        '/api': {
          target: proxyTarget,
          changeOrigin: true,
        },
        // Centrifugo's WebSocket endpoint (SLICE_003 §10/§11). Loopback dev
        // only: the tunnel and production route this by hostname/path
        // instead (§11), which is why this proxy entry never needs a
        // conditional on hostname the way api/client.ts's app.* rule does.
        '/connection': {
          target: realtimeProxyTarget,
          ws: true,
          changeOrigin: true,
        },
      },
    },
    // vite preview (scripts/dev-web-prod) serves the production build on
    // the same port. `port` is the only CommonServerOptions field the
    // preview resolver does not fall back to `server` for (verified against
    // installed Vite 8.2.1's `resolvePreviewOptions`, node_modules/vite/dist/node/chunks/node.js):
    // host, strictPort, allowedHosts, https, open, proxy, cors and headers
    // are all inherited from `server` when `preview` omits them, so
    // duplicating them here would just be dead config that could drift.
    preview: {
      port,
    },
    test: {
      // A DOM stand-in, not a real browser: api/client.ts reads
      // `window.location` at module load, which every realtime test
      // transitively imports via api/queries.ts's `queryKeys`. Still
      // service-free (SLICE_003 §10/§13 criterion 8) — no network, no real
      // SDK; happy-dom never talks to anything.
      environment: 'happy-dom',
      setupFiles: ['./src/testSetup.ts'],
    },
  }
})
