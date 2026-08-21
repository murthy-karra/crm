import { fileURLToPath, URL } from 'node:url'
import { defineConfig, loadEnv } from 'vite'
import vue from '@vitejs/plugin-vue'

// Repo-root .env holds these variables (D-013), not web/.env.
const repoRoot = fileURLToPath(new URL('..', import.meta.url))

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, repoRoot, '')

  const host = env.CRM_WEB_BIND_ADDR || '127.0.0.1'
  const port = Number(env.CRM_WEB_PORT || 5173)
  const proxyTarget = env.CRM_WEB_API_PROXY_TARGET || 'http://127.0.0.1:3000'
  // Exact tunnel hostname only, never a wildcard. Unset locally: Vite's
  // built-in default already allows localhost/127.0.0.1.
  const allowedHosts = env.CRM_WEB_ALLOWED_HOSTS ? [env.CRM_WEB_ALLOWED_HOSTS] : undefined

  return {
    envDir: repoRoot,
    plugins: [vue()],
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
      },
    },
  }
})
