import { expect } from '@playwright/test';
import path from 'path';
import fs from 'fs';

export const IBT_FIXTURE = path.resolve(__dirname, '../../../fixtures/bmwm4gt3_bathurst 2026-02-20 21-45-59.ibt');
export const API_BASE = 'http://localhost:9100';

export function hasFixture(): boolean {
  return fs.existsSync(IBT_FIXTURE);
}

/** Wait for the page to be fully loaded and scripts executed */
export async function waitForPageReady(page: import('@playwright/test').Page) {
  await page.waitForLoadState('domcontentloaded');
  await page.waitForSelector('.grid-stack', { timeout: 10_000 });
}

/** Upload .ibt via API then reload the page so auto-detection enters replay mode */
export async function uploadAndEnterReplay(
  page: import('@playwright/test').Page,
  request: import('@playwright/test').APIRequestContext,
) {
  const fileBuffer = fs.readFileSync(IBT_FIXTURE);
  const resp = await request.post(`${API_BASE}/api/replay/upload`, {
    multipart: {
      file: {
        name: 'bmwm4gt3_bathurst.ibt',
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

/** Upload .ibt via API only (no browser interaction) */
export async function uploadReplayApi(request: import('@playwright/test').APIRequestContext) {
  const fileBuffer = fs.readFileSync(IBT_FIXTURE);
  return request.post(`${API_BASE}/api/replay/upload`, {
    multipart: {
      file: {
        name: 'bmwm4gt3_bathurst.ibt',
        mimeType: 'application/octet-stream',
        buffer: fileBuffer,
      },
    },
  });
}
