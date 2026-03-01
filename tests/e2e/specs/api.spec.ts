import { test, expect } from '@playwright/test';
import { API_BASE, hasFixture, uploadReplayApi } from './helpers';

test.describe('API: Adapters', () => {
  test('lists adapters with expected shape', async ({ request }) => {
    const resp = await request.get(`${API_BASE}/api/adapters`);
    expect(resp.ok()).toBeTruthy();
    const adapters = await resp.json();
    expect(Array.isArray(adapters)).toBeTruthy();
    for (const a of adapters) {
      expect(a).toHaveProperty('key');
      expect(a).toHaveProperty('name');
      expect(typeof a.detected).toBe('boolean');
      expect(typeof a.active).toBe('boolean');
      expect(typeof a.enabled).toBe('boolean');
    }
  });
});

test.describe('API: Sinks', () => {
  test('full sink lifecycle: create, list, delete', async ({ request }) => {
    // Create
    const createResp = await request.post(`${API_BASE}/api/sinks`, {
      data: { id: '', host: '127.0.0.1', port: 9999, update_rate_hz: 10 },
    });
    expect(createResp.status()).toBe(201);
    const sink = await createResp.json();
    expect(sink.host).toBe('127.0.0.1');
    expect(sink.port).toBe(9999);
    expect(sink.id).toBeTruthy();

    // List — should contain our sink
    const listResp = await request.get(`${API_BASE}/api/sinks`);
    const sinks = await listResp.json();
    expect(sinks.some((s: any) => s.id === sink.id)).toBeTruthy();

    // Delete
    const delResp = await request.delete(`${API_BASE}/api/sinks/${sink.id}`);
    expect(delResp.status()).toBe(204);

    // Verify gone
    const listResp2 = await request.get(`${API_BASE}/api/sinks`);
    const sinks2 = await listResp2.json();
    expect(sinks2.some((s: any) => s.id === sink.id)).toBeFalsy();
  });

  test('create sink with metric mask', async ({ request }) => {
    const resp = await request.post(`${API_BASE}/api/sinks`, {
      data: { id: '', host: '127.0.0.1', port: 9998, metric_mask: 'rpm,speed' },
    });
    expect(resp.status()).toBe(201);
    const sink = await resp.json();
    expect(sink.metric_mask).toBe('rpm,speed');
    // Cleanup
    await request.delete(`${API_BASE}/api/sinks/${sink.id}`);
  });
});

test.describe('API: Replay', () => {
  test.beforeEach(async ({ request }) => {
    await request.delete(`${API_BASE}/api/replay`);
  });

  test('info returns history mode when no replay active', async ({ request }) => {
    const resp = await request.get(`${API_BASE}/api/replay/info`);
    expect(resp.ok()).toBeTruthy();
    const info = await resp.json();
    expect(info.mode).toBe('history');
  });

  test('full replay lifecycle: upload, info, frames, control, delete', async ({ request }) => {
    test.skip(!hasFixture(), 'Fixture .ibt not found');

    // Upload
    const uploadResp = await uploadReplayApi(request);
    expect(uploadResp.ok()).toBeTruthy();
    const upload = await uploadResp.json();
    expect(upload.status).toBe('ok');
    expect(upload.info.total_frames).toBeGreaterThan(0);
    expect(upload.info.track_name).toBeTruthy();
    expect(upload.info.car_name).toBeTruthy();
    expect(upload.info.laps.length).toBeGreaterThan(0);

    // Info
    const infoResp = await request.get(`${API_BASE}/api/replay/info`);
    const info = await infoResp.json();
    expect(info.mode).toBe('replay');
    expect(info.total_frames).toBeGreaterThan(0);

    // Frames
    const framesResp = await request.get(`${API_BASE}/api/replay/frames?start=0&count=10`);
    expect(framesResp.ok()).toBeTruthy();
    const frames = await framesResp.json();
    expect(frames.length).toBe(10);
    expect(frames[0]).toHaveProperty('i');
    expect(frames[0]).toHaveProperty('f');
    expect(frames[0].f).toHaveProperty('vehicle');

    // Control: pause
    const pauseResp = await request.post(`${API_BASE}/api/replay/control`, {
      data: { action: 'pause' },
    });
    expect(pauseResp.ok()).toBeTruthy();

    // Control: seek
    const seekResp = await request.post(`${API_BASE}/api/replay/control`, {
      data: { action: 'seek', value: 100 },
    });
    expect(seekResp.ok()).toBeTruthy();

    // Control: speed
    const speedResp = await request.post(`${API_BASE}/api/replay/control`, {
      data: { action: 'speed', value: 4 },
    });
    expect(speedResp.ok()).toBeTruthy();

    // Control: play
    const playResp = await request.post(`${API_BASE}/api/replay/control`, {
      data: { action: 'play' },
    });
    expect(playResp.ok()).toBeTruthy();

    // Delete
    const delResp = await request.delete(`${API_BASE}/api/replay`);
    expect(delResp.status()).toBe(204);

    // Verify deleted
    const infoResp2 = await request.get(`${API_BASE}/api/replay/info`);
    const info2 = await infoResp2.json();
    expect(info2.mode).toBe('history');
  });

  test('upload rejects non-.ibt file', async ({ request }) => {
    const resp = await request.post(`${API_BASE}/api/replay/upload`, {
      multipart: {
        file: {
          name: 'test.txt',
          mimeType: 'text/plain',
          buffer: Buffer.from('not an ibt file'),
        },
      },
    });
    expect(resp.status()).toBe(400);
  });
});

test.describe('API: History', () => {
  test('config resize buffer', async ({ request }) => {
    const resp = await request.post(`${API_BASE}/api/history/config`, {
      data: { max_duration_secs: 300 },
    });
    expect(resp.ok()).toBeTruthy();
    // Restore default
    await request.post(`${API_BASE}/api/history/config`, {
      data: { max_duration_secs: 600 },
    });
  });
});

test.describe('API: Persistence', () => {
  test('get config', async ({ request }) => {
    const resp = await request.get(`${API_BASE}/api/persistence/config`);
    expect(resp.ok()).toBeTruthy();
    const cfg = await resp.json();
    expect(cfg).toHaveProperty('enabled');
    expect(cfg).toHaveProperty('frequency_hz');
    expect(cfg).toHaveProperty('auto_save');
    expect(cfg).toHaveProperty('retention');
  });

  test('get stats', async ({ request }) => {
    const resp = await request.get(`${API_BASE}/api/persistence/stats`);
    expect(resp.ok()).toBeTruthy();
  });

  test('list files', async ({ request }) => {
    const resp = await request.get(`${API_BASE}/api/persistence/files`);
    expect(resp.ok()).toBeTruthy();
    const files = await resp.json();
    expect(Array.isArray(files)).toBeTruthy();
  });
});
