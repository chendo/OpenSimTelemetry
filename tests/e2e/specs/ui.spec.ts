import { test, expect } from '@playwright/test';
import { waitForPageReady } from './helpers';

test.describe('UI: Header', () => {
  test('all header buttons and elements present', async ({ page }) => {
    await page.goto('/');
    await waitForPageReady(page);

    // Title and logo
    await expect(page).toHaveTitle('OpenSimTelemetry');
    await expect(page.locator('.logo-open')).toHaveText('OPEN');
    await expect(page.locator('.logo-sim')).toHaveText('SIM');
    await expect(page.locator('.logo-tel')).toHaveText('TELEMETRY');

    // Header buttons
    for (const id of [
      '#data-btn', '#menu-add-graph',
      '#settings-btn', '#header-reset-layout', '#header-computed-metrics',
    ]) {
      await expect(page.locator(id)).toBeVisible();
    }
    // Pause button is in seek bar
    await expect(page.locator('#header-pause-btn')).toBeAttached();

    // Remote URL input
    await expect(page.locator('#remote-url')).toBeVisible();

    // Connection status
    await expect(page.locator('#header-conn')).toBeVisible();
  });
});

test.describe('UI: Default Widgets', () => {
  test('all default widgets present with expected structure', async ({ page }) => {
    await page.goto('/');
    await waitForPageReady(page);

    // Vehicle widget
    await expect(page.locator('[data-widget-id="vehicle"]')).toBeVisible();
    await expect(page.locator('#v-speed')).toBeVisible();
    await expect(page.locator('#v-gear')).toBeVisible();
    await expect(page.locator('#v-rpm')).toBeVisible();
    await expect(page.locator('#v-wheel-canvas')).toBeVisible();
    // Pedal bars (fill elements may have 0 height when no input, so check attached)
    for (const id of ['#v-thr-bar', '#v-brk-bar', '#v-clt-bar']) {
      await expect(page.locator(id)).toBeAttached();
    }

    // G-Force widget
    await expect(page.locator('[data-widget-id="gforce"]')).toBeVisible();
    await expect(page.locator('#gf-canvas')).toBeVisible();
    for (const id of ['#gf-lat-num', '#gf-long-num', '#gf-vert-num', '#gf-yaw-num']) {
      await expect(page.locator(id)).toBeVisible();
    }

    // Wheels widget
    await expect(page.locator('[data-widget-id="wheels"]')).toBeVisible();
    await expect(page.locator('.wheel-corner')).toHaveCount(4);

    // Lap Timing widget
    await expect(page.locator('[data-widget-id="laptiming"]')).toBeVisible();
    await expect(page.locator('#lt-cur')).toBeVisible();
    await expect(page.locator('#lt-num')).toBeVisible();

    // Session widget
    await expect(page.locator('[data-widget-id="session"]')).toBeVisible();
    await expect(page.locator('#ss-track')).toBeVisible();
    await expect(page.locator('#ss-car')).toBeVisible();

    // Metrics widget
    await expect(page.locator('[data-widget-id="allfields"]')).toBeVisible();
    await expect(page.locator('#af-filter')).toBeVisible();
    await expect(page.locator('#af-hide-nulls')).toBeVisible();
    await expect(page.locator('#af-show-range')).toBeVisible();
    await expect(page.locator('#af-rate-btn')).toBeVisible();

    // Output Sinks widget
    await expect(page.locator('[data-widget-id="sinks"]')).toBeVisible();
    await expect(page.locator('#sk-host')).toBeVisible();
    await expect(page.locator('#sk-port')).toBeVisible();
  });
});

test.describe('UI: Widget Actions', () => {
  test('add graph widget', async ({ page }) => {
    await page.goto('/');
    await waitForPageReady(page);
    const countBefore = await page.locator('.grid-stack-item').count();
    await page.evaluate(() => document.getElementById('menu-add-graph')!.click());
    await expect(page.locator('.grid-stack-item')).toHaveCount(countBefore + 1, { timeout: 5_000 });
  });
});

test.describe('UI: Settings Modal', () => {
  test('opens with all sections and closes via button', async ({ page }) => {
    await page.goto('/');
    await waitForPageReady(page);
    await page.evaluate(() => (globalThis as any).openSettingsModal());

    const modal = page.locator('#settings-modal');
    await expect(modal).toBeVisible({ timeout: 5_000 });
    await expect(modal.locator('.cm-modal-title')).toHaveText('Settings');

    // History section
    await expect(modal.locator('text=History')).toBeVisible();
    await expect(modal.locator('#settings-history-duration')).toBeVisible();

    // Auto-Save
    await expect(modal.locator('#settings-autosave')).toBeVisible();

    // Retention section
    await expect(modal.locator('text=Retention')).toBeVisible();
    await expect(modal.locator('#settings-retention-max-sessions')).toBeVisible();
    await expect(modal.locator('#settings-retention-max-age')).toBeVisible();

    // Dashboard Profiles
    await expect(modal.locator('text=Dashboard Profiles')).toBeVisible();

    // Units section
    await expect(modal.locator('text=Units')).toBeVisible();
    const unitSelects = modal.locator('.unit-pref-select');
    expect(await unitSelects.count()).toBeGreaterThanOrEqual(3);

    // Graph Presets
    await expect(modal.locator('text=Graph Presets')).toBeVisible();

    // Close
    await page.evaluate(() => document.getElementById('settings-close')!.click());
    await expect(modal).not.toBeVisible();
  });

  test('closes on overlay click', async ({ page }) => {
    await page.goto('/');
    await waitForPageReady(page);
    await page.evaluate(() => (globalThis as any).openSettingsModal());

    const modal = page.locator('#settings-modal');
    await expect(modal).toBeVisible({ timeout: 5_000 });

    // Click the overlay (not the inner modal)
    await page.locator('#settings-modal').click({ position: { x: 5, y: 5 } });
    await expect(modal).not.toBeVisible();
  });
});

test.describe('UI: Metrics Widget', () => {
  test('filter toggles and rate dropdown', async ({ page }) => {
    await page.goto('/');
    await waitForPageReady(page);

    // Hide nulls starts active
    await expect(page.locator('#af-hide-nulls')).toHaveClass(/active/);
    // Show range starts inactive
    await expect(page.locator('#af-show-range')).not.toHaveClass(/active/);

    // Toggle hide nulls off
    await page.evaluate(() => document.getElementById('af-hide-nulls')!.click());
    await expect(page.locator('#af-hide-nulls')).not.toHaveClass(/active/);

    // Toggle show range on
    await page.evaluate(() => document.getElementById('af-show-range')!.click());
    await expect(page.locator('#af-show-range')).toHaveClass(/active/);

    // Rate dropdown
    await page.evaluate(() => document.getElementById('af-rate-btn')!.click());
    await expect(page.locator('#af-rate-menu')).toHaveClass(/open/);

    // Click 10 Hz option
    await page.evaluate(() =>
      (document.querySelector('.metrics-rate-opt[data-hz="10"]') as HTMLElement).click(),
    );
    await expect(page.locator('#af-rate-btn')).toHaveText('10 Hz');
    await expect(page.locator('#af-rate-menu')).not.toHaveClass(/open/);
  });
});

test.describe('UI: Controls', () => {
  test('pause/resume button toggles', async ({ page }) => {
    await page.goto('/');
    await waitForPageReady(page);

    // Pause button is in the seek bar, uses CSS class for state
    await expect(page.locator('#header-pause-btn')).not.toHaveClass(/paused/);
    await page.evaluate(() => document.getElementById('header-pause-btn')!.click());
    await expect(page.locator('#header-pause-btn')).toHaveClass(/paused/);
    await page.evaluate(() => document.getElementById('header-pause-btn')!.click());
    await expect(page.locator('#header-pause-btn')).not.toHaveClass(/paused/);
  });

  test('seek bar visible in non-replay mode', async ({ page }) => {
    await page.goto('/');
    await waitForPageReady(page);

    await expect(page.locator('#seek-bar')).toBeVisible();
    await expect(page.locator('#seek-slider')).toBeVisible();
    await expect(page.locator('#seek-live-btn')).toHaveText('LIVE');
    await expect(page.locator('#replay-bar')).not.toHaveClass(/active/);
  });
});

test.describe('UI: API Docs', () => {
  test('page loads with correct heading', async ({ page }) => {
    await page.goto('/api/docs');
    await expect(page.locator('h1')).toContainText('OpenSimTelemetry API');
  });
});
