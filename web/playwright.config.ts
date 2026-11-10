import { defineConfig, devices } from '@playwright/test';
// Playwright's automatic failure DOM snapshot can contain bearer links and messages.
process.env.PLAYWRIGHT_NO_COPY_PROMPT = '1';
export default defineConfig({
  testDir: './tests', outputDir: './test-results', timeout: 90_000, expect: { timeout: 30_000 }, workers: 1, fullyParallel: false, forbidOnly: !!process.env.CI,
  reporter: 'line', retries: 0,
  use: { baseURL: process.env.SSC_WEB_TEST_URL || 'http://127.0.0.1:1430', trace: 'off', screenshot: 'off', video: 'off' },
  webServer: process.env.SSC_WEB_TEST_URL ? undefined : { command: 'npm run web:dev', url: 'http://127.0.0.1:1430', reuseExistingServer: !process.env.CI, timeout: 30_000 },
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'], launchOptions: { args: ['--allow-loopback-in-peer-connection', '--use-fake-device-for-media-stream', '--use-fake-ui-for-media-stream'] } } },
    ...(process.env.SSC_WEB_BROWSER_MATRIX === '1' ? [
      { name: 'firefox', use: { ...devices['Desktop Firefox'] } },
      { name: 'webkit', use: { ...devices['Desktop Safari'] } },
    ] : []),
  ],
});
