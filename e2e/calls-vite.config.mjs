// Same Vue application and production build, with only the external LiveKit SDK
// replaced at its declared provider boundary. Never used by normal Web builds.
import base from './vite.config.ts';
import { fileURLToPath } from 'node:url';
export default env => {
  const config = base(env);
  return { ...config, resolve: { ...config.resolve, alias: {
    ...config.resolve?.alias,
    'livekit-client': fileURLToPath(new URL('./e2e-livekit-browser.mjs', import.meta.url)),
  } } };
};
