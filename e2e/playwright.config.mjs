import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './families',
  testMatch: process.env.E2E_FAMILY + '.spec.mjs',
  fullyParallel: false,
  workers: 1,
  retries: 0, // Only the host runner may retry: it creates a fresh whole family.
  timeout: process.env.E2E_FAMILY === 'migration' ? 300_000 : 180_000,
  expect: { timeout: 8_000 },
  reporter: [['line'], ['json', { outputFile: '/artifacts/playwright.json' }],
    ['html', { outputFolder: '/artifacts/html', open: 'never' }]],
  outputDir: '/artifacts/test-results',
  use: { baseURL: 'http://web:8080', browserName: 'chromium', channel: 'chromium',
    timezoneId: 'America/Los_Angeles', locale: 'en-US',
    // Docker DNS isn't a loopback hostname. Enable the same secure-context APIs
    // that localhost development has (crypto.randomUUID); this is not TLS evidence.
    // Full Chromium honors this flag; the headless-shell binary does not.
    launchOptions: { args: ['--unsafely-treat-insecure-origin-as-secure=http://web:8080'] },
    headless: true, actionTimeout: 8_000, navigationTimeout: 15_000 },
});
