import { defineConfig } from '@playwright/test';

// Env vars for flexible deployment:
//   PLAYWRIGHT_WS_ENDPOINT — connect to remote browser (e.g. Docker)
//   BROWSER_BASE_URL — URL the browser uses to reach the server (Docker network name)
//   API_BASE_URL — URL the test runner uses to reach the server (host port)
const wsEndpoint = process.env.PLAYWRIGHT_WS_ENDPOINT;
const browserBaseURL = process.env.BROWSER_BASE_URL
  || (wsEndpoint ? 'http://host.docker.internal:9100' : 'http://localhost:9100');
const externalServer = !!process.env.API_BASE_URL;

export default defineConfig({
  testDir: './specs',
  globalSetup: './global-setup.ts',
  timeout: 60_000,
  expect: { timeout: 10_000 },
  fullyParallel: false,
  workers: 1,
  retries: 1,
  use: {
    baseURL: browserBaseURL,
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
    ...(wsEndpoint
      ? { connectOptions: { wsEndpoint } }
      : { headless: true }),
  },
  projects: [
    { name: 'chromium', use: { browserName: 'chromium' } },
  ],
  // Skip webServer when using an external server (e.g. Docker Compose)
  ...(!externalServer && {
    webServer: {
      command: 'cargo run --release -p ost-server',
      cwd: '../..',
      url: 'http://localhost:9100',
      reuseExistingServer: true,
      timeout: 120_000,
    },
  }),
});
