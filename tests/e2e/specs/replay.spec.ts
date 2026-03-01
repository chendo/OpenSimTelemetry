import { test, expect } from '@playwright/test';
import path from 'path';
import fs from 'fs';

const IBT_FIXTURE = path.resolve(__dirname, '../../../fixtures/race.ibt');
const API_BASE = 'http://localhost:9100';

/** Wait for the page to be fully loaded and scripts executed */
async function waitForPageReady(page: import('@playwright/test').Page) {
  await page.waitForLoadState('domcontentloaded');
  await page.waitForSelector('.grid-stack', { timeout: 10_000 });
}

/** Upload .ibt via API then reload the page so auto-detection enters replay mode */
async function uploadAndEnterReplay(
  page: import('@playwright/test').Page,
  request: import('@playwright/test').APIRequestContext,
) {
  // Upload via API from host
  const fileBuffer = fs.readFileSync(IBT_FIXTURE);
  const resp = await request.post(`${API_BASE}/api/replay/upload`, {
    multipart: {
      file: {
        name: 'race.ibt',
        mimeType: 'application/octet-stream',
        buffer: fileBuffer,
      },
    },
  });
  expect(resp.ok()).toBeTruthy();

  // Verify the server has replay loaded
  const infoResp = await request.get(`${API_BASE}/api/replay/info`);
  expect(infoResp.ok()).toBeTruthy();
  const info = await infoResp.json();
  expect(info.mode).toBe('replay');

  // Reload the page — checkReplayOnLoad() runs automatically and enters replay mode
  await page.reload();
  await waitForPageReady(page);

  // Wait for replay bar to become active
  await expect(page.locator('#replay-bar.active')).toBeVisible({ timeout: 10_000 });
}

test.describe('OpenSimTelemetry UI', () => {
  test('page loads with correct title and header', async ({ page }) => {
    await page.goto('/');
    await waitForPageReady(page);
    await expect(page).toHaveTitle('OpenSimTelemetry');
    await expect(page.locator('.logo-open')).toHaveText('OPEN');
    await expect(page.locator('.logo-sim')).toHaveText('SIM');
    await expect(page.locator('.logo-tel')).toHaveText('TELEMETRY');

    await expect(page.locator('#header-add-graph')).toBeVisible();
    await expect(page.locator('#header-add-gauge')).toBeVisible();
    await expect(page.locator('#settings-btn')).toBeVisible();
    await expect(page.locator('#header-load-ibt')).toBeVisible();
  });

  test('API docs page accessible', async ({ page }) => {
    await page.goto('/api/docs');
    await expect(page.locator('h1')).toContainText('OpenSimTelemetry API');
  });

  test('settings modal opens and closes', async ({ page }) => {
    await page.goto('/');
    await waitForPageReady(page);

    await page.evaluate(() => (globalThis as any).openSettingsModal());

    const modal = page.locator('#settings-modal');
    await expect(modal).toBeVisible({ timeout: 5_000 });
    await expect(modal.locator('.cm-modal-title')).toHaveText('Settings');
    await expect(modal.locator('text=History')).toBeVisible();
    await expect(modal.locator('text=Retention')).toBeVisible();

    await page.evaluate(() => document.getElementById('settings-close')!.click());
    await expect(modal).not.toBeVisible();
  });

  test('add graph widget', async ({ page }) => {
    await page.goto('/');
    await waitForPageReady(page);

    const countBefore = await page.locator('.grid-stack-item').count();
    await page.evaluate(() => document.getElementById('header-add-graph')!.click());
    await expect(page.locator('.grid-stack-item')).toHaveCount(countBefore + 1, { timeout: 5_000 });
  });

  test('add gauge widget', async ({ page }) => {
    await page.goto('/');
    await waitForPageReady(page);

    const countBefore = await page.locator('.grid-stack-item').count();
    await page.evaluate(() => document.getElementById('header-add-gauge')!.click());
    await expect(page.locator('.grid-stack-item')).toHaveCount(countBefore + 1, { timeout: 5_000 });
  });
});

test.describe('Replay functionality', () => {
  test.beforeEach(async ({ request }) => {
    await request.delete(`${API_BASE}/api/replay`);
  });

  test('upload .ibt file and verify replay mode', async ({ page, request }) => {
    test.skip(!fs.existsSync(IBT_FIXTURE), 'Fixture race.ibt not found');

    await page.goto('/');
    await waitForPageReady(page);
    await uploadAndEnterReplay(page, request);

    // Track name should be displayed
    await expect(async () => {
      const trackText = await page.locator('#replay-track').textContent();
      expect(trackText).not.toBe('--');
      expect(trackText!.length).toBeGreaterThan(0);
    }).toPass({ timeout: 5_000 });

    await expect(page.locator('#replay-time')).toBeVisible();
    await expect(page.locator('.speed-btn[data-speed="1"]')).toBeVisible();
    await expect(page.locator('.speed-btn[data-speed="2"]')).toBeVisible();
    await expect(page.locator('.speed-btn[data-speed="4"]')).toBeVisible();
    await expect(page.locator('#mode-badge')).toHaveText('REPLAY');
  });

  test('replay speed control', async ({ page, request }) => {
    test.skip(!fs.existsSync(IBT_FIXTURE), 'Fixture race.ibt not found');

    await page.goto('/');
    await waitForPageReady(page);
    await uploadAndEnterReplay(page, request);

    await expect(page.locator('.speed-btn[data-speed="1"]')).toHaveClass(/active/);

    await page.evaluate(() =>
      (document.querySelector('.speed-btn[data-speed="4"]') as HTMLElement).click(),
    );
    await expect(page.locator('.speed-btn[data-speed="4"]')).toHaveClass(/active/);
    await expect(page.locator('.speed-btn[data-speed="1"]')).not.toHaveClass(/active/);

    await page.evaluate(() =>
      (document.querySelector('.speed-btn[data-speed="1"]') as HTMLElement).click(),
    );
    await expect(page.locator('.speed-btn[data-speed="1"]')).toHaveClass(/active/);
    await expect(page.locator('.speed-btn[data-speed="4"]')).not.toHaveClass(/active/);
  });

  test('replay seek slider and controls', async ({ page, request }) => {
    test.skip(!fs.existsSync(IBT_FIXTURE), 'Fixture race.ibt not found');

    await page.goto('/');
    await waitForPageReady(page);
    await uploadAndEnterReplay(page, request);

    // Verify seek slider is present with valid range
    const slider = page.locator('#replay-seek');
    await expect(slider).toBeVisible();
    const max = await slider.getAttribute('max');
    expect(parseInt(max!)).toBeGreaterThan(0);

    // Verify play/pause button is present
    await expect(page.locator('#replay-play-pause')).toBeVisible();

    // Verify time display shows duration
    const timeText = await page.locator('#replay-time').textContent();
    expect(timeText).toContain('/');
  });

  test('exit replay returns to live mode', async ({ page, request }) => {
    test.skip(!fs.existsSync(IBT_FIXTURE), 'Fixture race.ibt not found');

    await page.goto('/');
    await waitForPageReady(page);
    await uploadAndEnterReplay(page, request);

    await page.evaluate(() => document.getElementById('replay-exit')!.click());

    await expect(page.locator('#replay-bar')).not.toHaveClass(/active/);
    await expect(page.locator('#seek-bar')).toBeVisible();
  });
});
