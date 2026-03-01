import { defineConfig } from '@playwright/test';

// When PLAYWRIGHT_WS_ENDPOINT is set, we connect to a remote browser (e.g. in Docker).
// The browser needs host.docker.internal to reach the server on the host.
const wsEndpoint = process.env.PLAYWRIGHT_WS_ENDPOINT;
const browserBaseURL = wsEndpoint
  ? 'http://host.docker.internal:9100'
  : 'http://localhost:9100';

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
  webServer: {
    command: 'cargo run --release -p ost-server',
    cwd: '../..',
    url: 'http://localhost:9100',
    reuseExistingServer: true,
    timeout: 120_000,
  },
});
