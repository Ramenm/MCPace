import { defineConfig } from '@playwright/test';

const chromiumExecutable = process.env.MCPACE_PLAYWRIGHT_CHROMIUM || undefined;
const workers = Number.parseInt(process.env.MCPACE_PLAYWRIGHT_WORKERS || '2', 10);

export default defineConfig({
  testDir: '.',
  testMatch: /dashboard(?:\.[a-z]+)?\.playwright\.spec\.mjs$/,
  timeout: 60_000,
  expect: { timeout: 5_000 },
  retries: 0,
  fullyParallel: true,
  workers: Number.isSafeInteger(workers) && workers > 0 ? workers : 2,
  reporter: process.env.MCPACE_PLAYWRIGHT_REPORTER || 'list',
  use: {
    browserName: 'chromium',
    headless: true,
    trace: 'off',
    video: 'off',
    screenshot: 'only-on-failure',
    launchOptions: {
      executablePath: chromiumExecutable,
      args: ['--no-sandbox', '--disable-dev-shm-usage', '--disable-gpu']
    }
  }
});
