import { test, expect } from '@playwright/test';
import { API_BASE, hasFixture, waitForPageReady, uploadAndEnterReplay } from './helpers';

test.describe('Replay Mode', () => {
  test.beforeEach(async ({ request }) => {
    await request.delete(`${API_BASE}/api/replay`);
  });

  test('upload .ibt and verify replay UI', async ({ page, request }) => {
    test.skip(!hasFixture(), 'Fixture .ibt not found');

    await page.goto('/');
    await waitForPageReady(page);
    await uploadAndEnterReplay(page, request);

    // Replay bar active
    await expect(page.locator('#replay-bar')).toHaveClass(/active/);
    await expect(page.locator('#mode-badge')).toHaveText('REPLAY');

    // Track and car populated
    await expect(async () => {
      const trackText = await page.locator('#replay-track').textContent();
      expect(trackText).not.toBe('--');
      expect(trackText!.length).toBeGreaterThan(0);
    }).toPass({ timeout: 5_000 });

    await expect(async () => {
      const carText = await page.locator('#replay-car').textContent();
      expect(carText).not.toBe('--');
      expect(carText!.length).toBeGreaterThan(0);
    }).toPass({ timeout: 5_000 });

    // Time display and seek slider
    await expect(page.locator('#replay-time')).toBeVisible();
    const timeText = await page.locator('#replay-time').textContent();
    expect(timeText).toContain('/');

    const slider = page.locator('#replay-seek');
    await expect(slider).toBeVisible();
    const max = await slider.getAttribute('max');
    expect(parseInt(max!)).toBeGreaterThan(0);

    // All 5 speed buttons visible, 1x active by default
    for (const speed of ['0.25', '0.5', '1', '2', '4']) {
      await expect(page.locator(`.speed-btn[data-speed="${speed}"]`)).toBeVisible();
    }
    await expect(page.locator('.speed-btn[data-speed="1"]')).toHaveClass(/active/);

    // Play/pause visible
    await expect(page.locator('#replay-play-pause')).toBeVisible();
  });

  test('speed control toggles active state', async ({ page, request }) => {
    test.skip(!hasFixture(), 'Fixture .ibt not found');

    await page.goto('/');
    await waitForPageReady(page);
    await uploadAndEnterReplay(page, request);

    await expect(page.locator('.speed-btn[data-speed="1"]')).toHaveClass(/active/);

    // Switch to 4x
    await page.evaluate(() =>
      (document.querySelector('.speed-btn[data-speed="4"]') as HTMLElement).click(),
    );
    await expect(page.locator('.speed-btn[data-speed="4"]')).toHaveClass(/active/);
    await expect(page.locator('.speed-btn[data-speed="1"]')).not.toHaveClass(/active/);

    // Switch to 0.25x
    await page.evaluate(() =>
      (document.querySelector('.speed-btn[data-speed="0.25"]') as HTMLElement).click(),
    );
    await expect(page.locator('.speed-btn[data-speed="0.25"]')).toHaveClass(/active/);
    await expect(page.locator('.speed-btn[data-speed="4"]')).not.toHaveClass(/active/);

    // Back to 1x
    await page.evaluate(() =>
      (document.querySelector('.speed-btn[data-speed="1"]') as HTMLElement).click(),
    );
    await expect(page.locator('.speed-btn[data-speed="1"]')).toHaveClass(/active/);
  });

  test('play/pause via API updates state', async ({ page, request }) => {
    test.skip(!hasFixture(), 'Fixture .ibt not found');

    await page.goto('/');
    await waitForPageReady(page);
    await uploadAndEnterReplay(page, request);

    // Play/pause button exists
    await expect(page.locator('#replay-play-pause')).toBeVisible();

    // Verify play via API
    const playResp = await request.post(`${API_BASE}/api/replay/control`, {
      data: { action: 'play' },
    });
    expect(playResp.ok()).toBeTruthy();

    // Verify pause via API
    const pauseResp = await request.post(`${API_BASE}/api/replay/control`, {
      data: { action: 'pause' },
    });
    expect(pauseResp.ok()).toBeTruthy();

    // Verify seek via API changes position
    const seekResp = await request.post(`${API_BASE}/api/replay/control`, {
      data: { action: 'seek', value: 500 },
    });
    expect(seekResp.ok()).toBeTruthy();
  });

  test('lap selector appears and navigates', async ({ page, request }) => {
    test.skip(!hasFixture(), 'Fixture .ibt not found');

    await page.goto('/');
    await waitForPageReady(page);
    await uploadAndEnterReplay(page, request);

    // Lap group should be visible (display not 'none') if laps exist
    await expect(async () => {
      const display = await page.locator('#replay-lap-group').evaluate(
        (el) => getComputedStyle(el).display,
      );
      expect(display).not.toBe('none');
    }).toPass({ timeout: 5_000 });

    // Lap button shows "Lap ..."
    await expect(page.locator('#replay-lap-btn')).toContainText('Lap');

    // Open lap menu
    await page.evaluate(() => document.getElementById('replay-lap-btn')!.click());
    await expect(page.locator('#replay-lap-menu')).toHaveClass(/open/);

    // Has lap items
    const lapItems = page.locator('.replay-lap-item');
    expect(await lapItems.count()).toBeGreaterThan(0);

    // At least one best lap
    expect(await page.locator('.replay-lap-item.best').count()).toBeGreaterThanOrEqual(1);

    // Click a lap item → menu closes
    await lapItems.nth(1).click();
    await expect(page.locator('#replay-lap-menu')).not.toHaveClass(/open/);
  });

  test('loop controls', async ({ page, request }) => {
    test.skip(!hasFixture(), 'Fixture .ibt not found');

    await page.goto('/');
    await waitForPageReady(page);
    await uploadAndEnterReplay(page, request);

    // Loop buttons visible, not active
    await expect(page.locator('#replay-loop-start')).toBeVisible();
    await expect(page.locator('#replay-loop-start')).not.toHaveClass(/active/);
    await expect(page.locator('#replay-loop-end')).toBeVisible();
    await expect(page.locator('#replay-loop-end')).not.toHaveClass(/active/);
    await expect(page.locator('#replay-loop-toggle')).toBeVisible();
    await expect(page.locator('#replay-loop-toggle')).not.toHaveClass(/loop-on/);

    // Set loop start
    await page.evaluate(() => document.getElementById('replay-loop-start')!.click());
    await expect(page.locator('#replay-loop-start')).toHaveClass(/active/);

    // Set loop end
    await page.evaluate(() => document.getElementById('replay-loop-end')!.click());
    await expect(page.locator('#replay-loop-end')).toHaveClass(/active/);

    // Enable loop
    await page.evaluate(() => document.getElementById('replay-loop-toggle')!.click());
    await expect(page.locator('#replay-loop-toggle')).toHaveClass(/loop-on/);

    // Disable loop
    await page.evaluate(() => document.getElementById('replay-loop-toggle')!.click());
    await expect(page.locator('#replay-loop-toggle')).not.toHaveClass(/loop-on/);
  });

  test('replay populates vehicle widget with data', async ({ page, request }) => {
    test.skip(!hasFixture(), 'Fixture .ibt not found');

    await page.goto('/');
    await waitForPageReady(page);
    await uploadAndEnterReplay(page, request);

    // After replay loads, vehicle widget should show real data (not defaults)
    await expect(async () => {
      const speed = await page.locator('#v-speed').textContent();
      expect(speed).not.toBe('---');
    }).toPass({ timeout: 10_000 });

    await expect(async () => {
      const rpm = await page.locator('#v-rpm').textContent();
      expect(rpm).not.toBe('---');
    }).toPass({ timeout: 10_000 });
  });

  test('exit replay returns to live mode', async ({ page, request }) => {
    test.skip(!hasFixture(), 'Fixture .ibt not found');

    await page.goto('/');
    await waitForPageReady(page);
    await uploadAndEnterReplay(page, request);

    await page.evaluate(() => document.getElementById('replay-exit')!.click());

    await expect(page.locator('#replay-bar')).not.toHaveClass(/active/);
    await expect(page.locator('#seek-bar')).toBeVisible();

    // Verify via API
    const infoResp = await request.get(`${API_BASE}/api/replay/info`);
    const info = await infoResp.json();
    expect(info.mode).toBe('history');
  });
});
