import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests/browser',
  fullyParallel: true,
  workers: 2,
  forbidOnly: Boolean(process.env.CI),
  retries: 0,
  timeout: 30_000,
  use: { browserName: 'chromium', baseURL: 'http://127.0.0.1:8437', trace: 'retain-on-failure' },
  webServer: {
    command: 'python3 -m http.server 8437 --bind 127.0.0.1 --directory .',
    url: 'http://127.0.0.1:8437', reuseExistingServer: false,
    stderr: 'ignore',
  },
});
