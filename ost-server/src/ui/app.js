/* ==================== Initialization ==================== */
const store = new TelemetryStore();
const computedMetrics = new ComputedMetricsManager();
const replayBuf = new ReplayBuffer();
const historyBuf = new ReplayBuffer();
let historyMode = false;
let historyInfo = null;
const grid = new DashboardGrid(document.getElementById('dashboard-grid'));

// Global function for GraphWidget to call when config changes
function dashboardSaveGraphs() { grid.saveGraphConfigs(); }

// SSE connection + status line
const connEl = document.getElementById('header-conn');
let sseConnected = false;
let sseEverConnected = false;

function updateStatus() {
    let text, dotClass;
    if (!sseConnected) {
        text = sseEverConnected ? 'Disconnected' : 'Connecting...';
        dotClass = 'dot-inactive';
    } else if (replayBuf.count > 0) {
        text = 'Viewing replay';
        dotClass = 'dot-active';
    } else if (historyMode) {
        text = 'Viewing history';
        dotClass = 'dot-detected';
    } else if (streamPaused) {
        text = 'Paused';
        dotClass = 'dot-detected';
    } else {
        const active = store.adapters.find(a => a.active);
        if (active) {
            const now = performance.now();
            const hasRecentFrames = _frameTimestamps.some(t => now - t < 2000);
            if (hasRecentFrames) {
                text = `Receiving from ${active.name}`;
                dotClass = 'dot-active';
            } else {
                text = 'Waiting for sim session';
                dotClass = 'dot-detected';
            }
        } else {
            text = 'Waiting for data';
            dotClass = 'dot-detected';
        }
    }
    connEl.innerHTML = `<span class="status-dot ${dotClass}"></span><span>${text}</span>`;
    _updateRemoteState();
}

// Telemetry throughput tracking
const throughputEl = document.getElementById('header-throughput');
let _frameTimestamps = [];  // recent SSE frame arrival times
let _expectedTickRate = 60; // default; updated from frame data
let _throughputInterval = null;

function updateThroughput() {
    const now = performance.now();
    // Keep only last 2 seconds of timestamps
    _frameTimestamps = _frameTimestamps.filter(t => now - t < 2000);
    updateStatus();  // re-evaluate receiving vs waiting
    if (_frameTimestamps.length < 2) {
        throughputEl.textContent = '';
        return;
    }
    // During replay, expected rate = tickRate * playbackSpeed; otherwise use live tick rate
    const expectedRate = replayBuf.count > 0
        ? (replayBuf.tickRate || 60) * (replayBuf.playbackSpeed || 1)
        : _expectedTickRate;
    const elapsed = (now - _frameTimestamps[0]) / 1000;
    const fps = (_frameTimestamps.length - 1) / elapsed;
    const pct = Math.round((fps / expectedRate) * 100);
    throughputEl.textContent = `${pct}%`;
    throughputEl.className = 'header-throughput' + (pct < 98 ? ' throughput-warn' : '');
}

// Remote OST instance support
const DEFAULT_ENDPOINT = 'http://localhost:9100';
let remoteBase = localStorage.getItem('ost-remote-base') || '';
const remoteInput = document.getElementById('remote-url');
const remoteConnectBtn = document.getElementById('remote-connect');
const remoteLabel = document.getElementById('remote-label');
remoteInput.value = remoteBase || DEFAULT_ENDPOINT;

function apiBase() { return remoteBase || ''; }

function _updateRemoteState() {
    remoteInput.classList.remove('remote-connected', 'remote-error', 'remote-connecting');
    if (sseConnected) {
        remoteInput.classList.add('remote-connected');
        remoteLabel.textContent = 'Connected to';
    } else if (sseEverConnected) {
        remoteInput.classList.add('remote-error');
        remoteLabel.textContent = 'Disconnected from';
    } else {
        remoteInput.classList.add('remote-connecting');
        remoteLabel.textContent = 'Connecting to';
    }
}

function _normalizeUrl(val) {
    val = val.trim().replace(/\/+$/, '');
    if (val && !val.startsWith('http')) val = 'http://' + val;
    return val;
}

function _connectToEndpoint(val) {
    val = _normalizeUrl(val);
    // Treat default endpoint as local (empty remoteBase)
    const isLocal = !val || val === DEFAULT_ENDPOINT;
    remoteBase = isLocal ? '' : val;
    if (isLocal) {
        localStorage.removeItem('ost-remote-base');
    } else {
        localStorage.setItem('ost-remote-base', val);
    }
    remoteConnectBtn.style.display = 'none';
    sseEverConnected = false;
    connectSSE();
}

// Show Connect button when input differs from current connection
remoteInput.addEventListener('input', () => {
    const inputVal = _normalizeUrl(remoteInput.value);
    const currentVal = remoteBase || DEFAULT_ENDPOINT;
    remoteConnectBtn.style.display = inputVal !== currentVal ? '' : 'none';
});

remoteInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
        _connectToEndpoint(remoteInput.value);
        remoteInput.blur();
    }
    if (e.key === 'Escape') {
        remoteInput.value = remoteBase || DEFAULT_ENDPOINT;
        remoteConnectBtn.style.display = 'none';
        remoteInput.blur();
    }
});

remoteConnectBtn.addEventListener('click', () => {
    _connectToEndpoint(remoteInput.value);
});

_updateRemoteState();

// Single unified SSE connection (avoids exhausting HTTP/1.1 connection slots)
let sseSource = null;
let _fullFrame = null; // Accumulated frame state for delta merging
function connectSSE() {
    if (sseSource) sseSource.close();
    sseConnected = false;
    _fullFrame = null;
    _updateRemoteState();
    const es = new EventSource(apiBase() + '/api/stream');
    sseSource = es;
    es.onopen = () => {
        sseConnected = true; sseEverConnected = true; updateStatus();
        if (!_throughputInterval) _throughputInterval = setInterval(updateThroughput, 500);
    };
    es.onerror = () => {
        sseConnected = false; updateStatus(); es.close();
        _frameTimestamps = [];
        _fullFrame = null;
        updateThroughput();
        setTimeout(connectSSE, 5000);
    };
    es.addEventListener('frame', (e) => {
        try {
            const data = JSON.parse(e.data);
            let frame;
            if (data._delta && _fullFrame) {
                // Merge delta into accumulated state
                frame = {};
                for (const key of Object.keys(_fullFrame)) {
                    frame[key] = _fullFrame[key];
                }
                for (const [key, value] of Object.entries(data)) {
                    if (key === '_delta') continue;
                    if (value === null) {
                        delete frame[key];
                    } else {
                        frame[key] = value;
                    }
                }
            } else {
                frame = data;
            }
            _fullFrame = frame;
            if (!streamPaused) store.pushFrame(frame);
            const wasEmpty = _frameTimestamps.length === 0;
            _frameTimestamps.push(performance.now());
            if (wasEmpty) updateStatus();
        }
        catch (err) { console.error('Parse error:', err); }
    });
    es.addEventListener('status', (e) => {
        try { store.adapters = JSON.parse(e.data); updateHeaderAdapters(); updateStatus(); }
        catch (err) { console.error('Parse error:', err); }
    });
    es.addEventListener('sinks', (e) => {
        try { store.sinks = JSON.parse(e.data); store._sinksVersion = (store._sinksVersion || 0) + 1; requestRedraw(); }
        catch (err) { console.error('Parse error:', err); }
    });
}

// Create all widgets in a batch to avoid excessive change events
grid.gs.batchUpdate();

const staticWidgets = [
    new VehicleWidget(),
    new GForceWidget(),
    new WheelsWidget(),
    new AllMetricsWidget(),
];
staticWidgets.forEach(w => { w.init(); grid.addWidget(w); });

// Create graph widgets (restore from saved config, or create default)
const savedGraphs = grid.restoreGraphConfigs();
if (savedGraphs && savedGraphs.length > 0) {
    for (const cfg of savedGraphs) {
        const gw = new GraphWidget(cfg.id, null, cfg.enabledMetrics);
        gw.init();
        gw.applyConfig(cfg);
        grid.addWidget(gw);
    }
} else {
    const pedals = new GraphWidget('graph-pedals', { col: 1, row: 9, width: 12, height: 9 },
        ['speed', 'rpm', 'throttle', 'brake', 'clutch', 'abs_active']);
    pedals.init();
    pedals.setTitle('Pedals and Speed');
    grid.addWidget(pedals);
    const steering = new GraphWidget('graph-steering', { col: 1, row: 18, width: 12, height: 9 },
        ['steering', 'yaw_rate', 'lat_g']);
    steering.init();
    steering.setTitle('Steering');
    grid.addWidget(steering);
}

grid.gs.batchUpdate(false);
grid.restoreLayout();

// Header dropdown menus — generic toggle logic
function setupDropdown(btnId, menuId) {
    const btn = document.getElementById(btnId);
    const menu = document.getElementById(menuId);
    btn.addEventListener('click', (e) => {
        e.stopPropagation();
        // Close other dropdowns first
        document.querySelectorAll('.header-dropdown-menu.open').forEach(m => {
            if (m !== menu) m.classList.remove('open');
        });
        menu.classList.toggle('open');
    });
    menu.addEventListener('click', (e) => e.stopPropagation());
}
setupDropdown('data-btn', 'data-menu');
// Close all dropdowns on outside click
document.addEventListener('click', () => {
    document.querySelectorAll('.header-dropdown-menu.open').forEach(m => m.classList.remove('open'));
});

// Add Graph button
let graphCounter = Date.now();
document.getElementById('menu-add-graph').addEventListener('click', () => {
    const id = 'graph-' + (graphCounter++);
    let maxBottom = 0;
    for (const node of grid.gs.getGridItems()) {
        const n = node.gridstackNode;
        if (n) maxBottom = Math.max(maxBottom, (n.y || 0) + (n.h || 0));
    }
    const gw = new GraphWidget(id, { col: 1, row: maxBottom, width: 12, height: 9 });
    gw.init();
    grid.addWidget(gw);
    grid.saveLayout();
    grid.saveGraphConfigs();
});

// Sources section within Data dropdown
const sourcesMenu = document.getElementById('sources-menu');

function updateHeaderAdapters() {
    sourcesMenu.innerHTML = store.adapters.map(a => {
        const dotClass = a.active ? 'dot-active' : a.detected ? 'dot-detected' : 'dot-inactive';
        const checked = a.enabled ? 'checked' : '';
        return `<label class="sources-item" data-key="${a.key}">
            <span class="status-dot ${dotClass}"></span>
            <span class="sources-name">${a.name}</span>
            <input type="checkbox" class="sources-toggle" ${checked}>
            <span class="sources-switch"></span>
        </label>`;
    }).join('');
    // Attach toggle handlers
    sourcesMenu.querySelectorAll('.sources-toggle').forEach(cb => {
        cb.addEventListener('change', async () => {
            const key = cb.closest('.sources-item').dataset.key;
            try {
                await fetch(`${apiBase()}/api/adapters/${key}/toggle`, { method: 'POST' });
                // Status SSE will push the updated adapter list
            } catch (e) { console.error('Toggle adapter failed:', e); }
        });
    });
    // Update Data dropdown button to show active source count
    const activeCount = store.adapters.filter(a => a.active).length;
    const dataBtn = document.getElementById('data-btn');
    dataBtn.innerHTML = activeCount > 0 ? `Data (${activeCount}) &#9662;` : 'Data &#9662;';
}

// Pause/resume streaming button (in seek bar)
const pauseBtn = document.getElementById('header-pause-btn');
pauseBtn.addEventListener('click', () => {
    streamPaused = !streamPaused;
    pauseBtn.innerHTML = streamPaused ? '&#9654;' : '&#9646;&#9646;';
    pauseBtn.classList.toggle('paused', streamPaused);
    // Sync pause state to server history buffer
    fetch(apiBase() + '/api/replay/control', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ action: streamPaused ? 'pause' : 'play' })
    }).catch(() => {});
    updateStatus();
});

// Reset layout button
document.getElementById('header-reset-layout').addEventListener('click', () => grid.resetLayout());

// Computed metrics button
document.getElementById('header-computed-metrics').addEventListener('click', () => computedMetrics.openListModal());

// Load .ibt menu item
const ibtFileInput = document.getElementById('ibt-file-input');
document.getElementById('menu-load-ibt').addEventListener('click', () => {
    ibtFileInput.click();
    document.getElementById('data-menu').classList.remove('open');
});
ibtFileInput.addEventListener('change', () => {
    const file = ibtFileInput.files[0];
    if (file && file.name.toLowerCase().endsWith('.ibt')) replayPlayer.upload(file);
    ibtFileInput.value = '';
});

// Browse saved replays
document.getElementById('menu-browse-replays').addEventListener('click', async () => {
    document.getElementById('data-menu').classList.remove('open');
    if (document.getElementById('replays-modal')) return;

    const overlay = document.createElement('div');
    overlay.id = 'replays-modal';
    overlay.className = 'cm-overlay';

    const modal = document.createElement('div');
    modal.className = 'cm-modal';
    modal.style.width = '520px';

    const title = document.createElement('div');
    title.className = 'cm-modal-title';
    title.textContent = 'Saved Replays';
    modal.appendChild(title);

    const list = document.createElement('div');
    list.className = 'cm-list';
    list.innerHTML = '<div class="no-data">Loading...</div>';
    modal.appendChild(list);

    const btnRow = document.createElement('div');
    btnRow.className = 'cm-btn-row';
    const closeBtn = document.createElement('button');
    closeBtn.className = 'cm-btn cm-btn-cancel';
    closeBtn.textContent = 'Close';
    closeBtn.addEventListener('click', () => overlay.remove());
    btnRow.appendChild(closeBtn);
    modal.appendChild(btnRow);

    overlay.appendChild(modal);
    overlay.addEventListener('click', (e) => { if (e.target === overlay) overlay.remove(); });
    document.body.appendChild(overlay);

    // Fetch file list
    try {
        const resp = await fetch(apiBase() + '/api/persistence/files');
        const files = await resp.json();
        list.innerHTML = '';
        if (files.length === 0) {
            list.innerHTML = '<div class="no-data">No saved replays found</div>';
        }
        for (const f of files) {
            const item = document.createElement('div');
            item.className = 'cm-list-item';
            item.style.cursor = 'pointer';

            const info = document.createElement('div');
            info.style.flex = '1';
            info.style.minWidth = '0';
            // Parse filename for display: YYYY-MM-DD_HH-MM-SS_track_car.ost.ndjson.zstd
            const parts = f.name.replace('.ost.ndjson.zstd', '').split('_');
            const date = parts.length >= 2 ? parts[0] : '';
            const time = parts.length >= 2 ? parts[1].replace(/-/g, ':') : '';
            const rest = parts.slice(2).join(' ');
            info.innerHTML = `<div style="font-size:0.75rem;font-weight:600;color:var(--text-primary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${rest || f.name}</div>
                <div style="font-size:0.6rem;color:var(--text-muted)">${date} ${time} &middot; ${(f.size / 1024 / 1024).toFixed(1)} MB</div>`;

            const btnWrap = document.createElement('div');
            btnWrap.style.cssText = 'display:flex;gap:4px;flex-shrink:0';

            const loadBtn = document.createElement('button');
            loadBtn.className = 'cm-btn cm-btn-save';
            loadBtn.textContent = 'Load';
            loadBtn.addEventListener('click', async (e) => {
                e.stopPropagation();
                loadBtn.textContent = '...';
                loadBtn.disabled = true;
                try {
                    const r = await fetch(apiBase() + '/api/persistence/load', {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ filename: f.name })
                    });
                    if (r.ok) {
                        const data = await r.json();
                        if (data.info) {
                            replayPlayer.info = data.info;
                            replayPlayer.active = true;
                            replayPlayer.currentSpeed = data.info.playback_speed;
                            await replayPlayer.enterReplayMode();
                        }
                        overlay.remove();
                    } else {
                        const err = await r.text();
                        loadBtn.textContent = 'Error';
                        console.error('Load replay failed:', err);
                    }
                } catch (err) {
                    loadBtn.textContent = 'Error';
                    console.error('Load replay failed:', err);
                }
            });

            const delBtn = document.createElement('button');
            delBtn.className = 'cm-btn-del';
            delBtn.textContent = 'Del';
            delBtn.addEventListener('click', async (e) => {
                e.stopPropagation();
                if (!confirm(`Delete ${f.name}?`)) return;
                try {
                    const r = await fetch(apiBase() + `/api/persistence/files/${encodeURIComponent(f.name)}`, { method: 'DELETE' });
                    if (r.ok) {
                        item.remove();
                        if (list.children.length === 0) {
                            list.innerHTML = '<div class="no-data">No saved replays found</div>';
                        }
                    }
                } catch (err) { console.error('Delete failed:', err); }
            });

            btnWrap.appendChild(loadBtn);
            btnWrap.appendChild(delBtn);
            item.appendChild(info);
            item.appendChild(btnWrap);
            list.appendChild(item);
        }
    } catch (e) {
        list.innerHTML = '<div class="no-data">Failed to load file list</div>';
    }
});

// Export Data (standalone modal from Data dropdown)
document.getElementById('menu-export-data').addEventListener('click', () => {
    document.getElementById('data-menu').classList.remove('open');
    if (document.getElementById('export-modal')) return;

    const overlay = document.createElement('div');
    overlay.id = 'export-modal';
    overlay.className = 'cm-overlay';

    const modal = document.createElement('div');
    modal.className = 'cm-modal';
    modal.style.width = '360px';
    modal.innerHTML = `
        <div class="cm-modal-title">Export Data</div>
        <div class="settings-row">
            <span class="settings-label">Last N seconds</span>
            <input type="number" class="cm-form-input" id="export-duration" value="60" min="1" max="3600" style="width:80px">
        </div>
        <div class="settings-row">
            <span class="settings-label">Format</span>
            <select class="settings-select" id="export-format">
                <option value="csv">CSV</option>
                <option value="json">JSON</option>
            </select>
        </div>
        <div class="cm-btn-row">
            <button class="cm-btn cm-btn-cancel" id="export-cancel">Cancel</button>
            <button class="cm-btn cm-btn-save" id="export-go">Export</button>
        </div>`;

    overlay.appendChild(modal);
    overlay.addEventListener('click', (e) => { if (e.target === overlay) overlay.remove(); });
    document.body.appendChild(overlay);

    modal.querySelector('#export-cancel').addEventListener('click', () => overlay.remove());
    modal.querySelector('#export-go').addEventListener('click', () => {
        const duration = parseInt(modal.querySelector('#export-duration').value) || 60;
        const format = modal.querySelector('#export-format').value;
        const count = store._count;
        if (count === 0) { alert('No data in buffer'); return; }

        const now = performance.now();
        const cutoff = now - duration * 1000;
        const frames = [];
        for (let i = 0; i < count; i++) {
            const idx = (store._head - count + i + store._ring.length) % store._ring.length;
            const entry = store._ring[idx];
            if (entry && entry.t >= cutoff && entry._frame) frames.push(entry._frame);
        }
        if (frames.length === 0) { alert('No data in the selected time range'); return; }

        let blob, filename;
        if (format === 'csv') {
            const paths = [];
            function collectPaths(obj, prefix) {
                for (const [k, v] of Object.entries(obj)) {
                    const p = prefix ? prefix + '.' + k : k;
                    if (typeof v === 'number') paths.push(p);
                    else if (v && typeof v === 'object' && !Array.isArray(v)) collectPaths(v, p);
                }
            }
            collectPaths(frames[0], '');
            const header = ['timestamp', ...paths].join(',');
            const rows = frames.map(f => {
                const vals = paths.map(p => {
                    const parts = p.split('.');
                    let cur = f;
                    for (const part of parts) { cur = cur?.[part]; }
                    return typeof cur === 'number' ? cur : '';
                });
                return [f.meta?.timestamp || '', ...vals].join(',');
            });
            blob = new Blob([header + '\n' + rows.join('\n')], { type: 'text/csv' });
            filename = `telemetry_${new Date().toISOString().replace(/[:.]/g, '-')}.csv`;
        } else {
            blob = new Blob([JSON.stringify(frames, null, 2)], { type: 'application/json' });
            filename = `telemetry_${new Date().toISOString().replace(/[:.]/g, '-')}.json`;
        }

        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = filename;
        document.body.appendChild(a);
        a.click();
        a.remove();
        URL.revokeObjectURL(url);
        overlay.remove();
    });
});

// ==================== Session Status Bar ====================
const sessionBar = document.getElementById('session-bar');
const sessionBarInfo = document.getElementById('session-bar-info');
const sbCur = document.getElementById('sb-cur');
const sbBest = document.getElementById('sb-best');
const sbLast = document.getElementById('sb-last');
const sbLap = document.getElementById('sb-lap');

function _fmtLapTime(s) {
    if (s == null || isNaN(s) || s <= 0) return '--:--.---';
    const m = Math.floor(s / 60);
    return `${m}:${(s % 60).toFixed(3).padStart(6, '0')}`;
}

function _fmtDuration(s) {
    if (s == null || isNaN(s) || s < 0) return null;
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    const sec = Math.floor(s % 60);
    if (h > 0) return `${h}:${String(m).padStart(2, '0')}:${String(sec).padStart(2, '0')}`;
    return `${m}:${String(sec).padStart(2, '0')}`;
}

function updateSessionBar() {
    const f = store.currentFrame;
    if (!f) {
        sessionBarInfo.innerHTML = '<span style="color:var(--text-muted)">Waiting for data...</span>';
        sbCur.textContent = '--:--.---';
        sbBest.textContent = '--:--.---';
        sbLast.textContent = '--:--.---';
        sbLap.textContent = '--';
        return;
    }

    const s = f.session;
    const w = f.weather;
    const t = f.timing;
    const d = f.driver;
    const v = f.vehicle;

    // Build info string: "<Driver> | <Car> @ <Track> | <Weather> | State: <status> <duration>"
    let parts = [];
    if (d?.name) parts.push(`<strong>${d.name}</strong>`);
    const car = v?.car_name || '--';
    const track = s?.track_name || '--';
    parts.push(`<strong>${car}</strong> @ <strong>${track}</strong>`);
    // Weather conditions
    const weatherParts = [];
    if (w?.air_temp != null) weatherParts.push(`Air ${w.air_temp.toFixed(0)}\u00B0C`);
    if (w?.track_temp != null) weatherParts.push(`Track ${w.track_temp.toFixed(0)}\u00B0C`);
    if (w?.track_wetness) weatherParts.push(w.track_wetness);
    if (weatherParts.length > 0) parts.push(weatherParts.join(', '));
    // Session type/state + duration
    const stateParts = [];
    if (s?.session_type) stateParts.push(s.session_type);
    if (s?.session_state) stateParts.push(s.session_state);
    const dur = _fmtDuration(s?.session_time);
    if (dur) stateParts.push(dur);
    if (stateParts.length > 0) parts.push('State: ' + stateParts.join(' \u2014 '));
    sessionBarInfo.innerHTML = parts.join(' | ');

    // Lap timing
    sbCur.textContent = _fmtLapTime(t?.current_lap_time);
    sbBest.textContent = _fmtLapTime(t?.best_lap_time);
    sbLast.textContent = _fmtLapTime(t?.last_lap_time);
    sbLap.textContent = t?.lap_number ?? '--';
}

// Render loop (decoupled from SSE; also redraws on UI interactions like hover/toggle)
let _lastPrefetchTime = 0;
function renderLoop() {
    const now = performance.now();
    const activeReplay = replayBuf.count > 0 ? replayBuf : null;
    const activeHistory = !activeReplay && historyMode && historyBuf.count > 0 ? historyBuf : null;
    const activeBuf = activeReplay || activeHistory;

    // Advance client-side cursor and sync store for all widgets
    if (activeReplay) {
        replayBuf.advancePlayback(now);
        // Skip fetch during scrubbing — seek handlers manage it to avoid abort conflicts
        if (!replayBuf.scrubbing) {
            if (replayBuf.needsFetch()) {
                replayBuf.ensureLoaded(buildReplayMetricMask());
            } else if (replayBuf.playing && now - _lastPrefetchTime > 1000) {
                _lastPrefetchTime = now;
                replayBuf.ensureLoaded(buildReplayMetricMask());
            }
        }
        // Update replay controls (slider, time)
        if (typeof replayPlayer !== 'undefined' && replayPlayer.active) {
            replayPlayer.updateControlsFromBuf();
        }
    }

    if (activeHistory) {
        // Skip fetch during scrubbing — seek handler manages it
        if (!_histScrubbing && historyBuf.needsFetch()) {
            historyBuf.ensureLoaded(buildReplayMetricMask());
        }
    }

    if (activeBuf) {
        const frame = activeBuf.currentFrame();
        if (frame) {
            store.currentFrame = frame;
            store._dirty = true;
        }
    }

    // Update bars
    updateSeekBar();
    updateSessionBar();

    if (store._dirty || _uiDirty || (activeBuf && activeBuf._dirty)) {
        store._dirty = false;
        _uiDirty = false;
        if (activeBuf) activeBuf._dirty = false;
        for (const w of grid.widgets.values()) {
            if (w._visible) w.update(store, now, activeBuf);
        }
    }
    requestAnimationFrame(renderLoop);
}

// ==================== History Seek Bar ====================
const seekBar = document.getElementById('seek-bar');
const seekSlider = document.getElementById('seek-slider');
const seekTime = document.getElementById('seek-time');
const seekLiveBtn = document.getElementById('seek-live-btn');
const seekLaps = document.getElementById('seek-laps');
const seekTrackWrap = document.getElementById('seek-track-wrap');
let _seekThrottleTime = 0;
let _histScrubbing = false;

function enterHistoryMode() {
    if (historyMode) return;
    historyMode = true;
    if (historyInfo) {
        historyBuf.totalFrames = historyInfo.total_frames;
        historyBuf.tickRate = historyInfo.tick_rate || 60;
        historyBuf._chunkSize = historyBuf.tickRate * 5;
        historyBuf._maxCacheFrames = historyBuf.tickRate * 180;
        historyBuf.cursor = parseInt(seekSlider.value);
        historyBuf.playing = false;
        historyBuf.replayId = null;
    }
    seekLiveBtn.classList.remove('active');
    const badge = document.getElementById('mode-badge');
    badge.textContent = 'HISTORY';
    badge.className = 'mode-badge mode-history';
    badge.style.display = '';
    historyBuf.ensureLoaded(buildReplayMetricMask());
    updateStatus();
}

function exitHistoryMode() {
    if (!historyMode) return;
    historyMode = false;
    historyBuf.reset();
    seekLiveBtn.classList.add('active');
    const badge = document.getElementById('mode-badge');
    badge.style.display = 'none';
    if (historyInfo) {
        seekSlider.value = seekSlider.max;
    }
    updateSeekTimeDisplay();
    updateStatus();
    requestRedraw();
}

function updateLiveScrollBadge() {
    const badge = document.getElementById('mode-badge');
    if (store.liveScrollOffsetMs > 0) {
        const secs = (store.liveScrollOffsetMs / 1000).toFixed(1);
        badge.textContent = 'OFFSET -' + secs + 's';
        badge.className = 'mode-badge mode-history';
        badge.style.display = '';
        seekLiveBtn.classList.remove('active');
    } else {
        if (!historyMode) {
            badge.style.display = 'none';
            seekLiveBtn.classList.add('active');
        }
    }
}

function resetLiveScrollOffset() {
    store.liveScrollOffsetMs = 0;
    updateLiveScrollBadge();
    store._dirty = true;
    requestRedraw();
}

// Called by GraphWidget horizontal scroll to move cursor
function graphScrollCursor(delta) {
    // Replay mode
    if (replayBuf.count > 0) {
        replayBuf.cursor = Math.max(0, Math.min(replayBuf.totalFrames - 1, replayBuf.cursor + delta));
        replayBuf._dirty = true;
        replayBuf.ensureLoaded(buildReplayMetricMask());
        requestRedraw();
        return;
    }
    // Live mode: adjust scroll offset (negative delta = scroll back in time)
    const frameDurationMs = 1000 / (historyInfo?.tick_rate || 60);
    store.liveScrollOffsetMs = Math.max(0,
        Math.min(store.maxScrollOffsetMs(), store.liveScrollOffsetMs - delta * frameDurationMs));
    updateLiveScrollBadge();
    store._dirty = true;
    requestRedraw();
}

// Called by GraphWidget click to seek to a specific time
function graphSeekToTime(timeMs, isBuffered) {
    if (isBuffered) {
        // Time is simTimeMs — convert to frame
        const buf = replayBuf.count > 0 ? replayBuf : historyBuf;
        const frame = Math.max(0, Math.min(buf.totalFrames - 1,
            Math.round(timeMs / 1000 * buf.tickRate)));
        buf.cursor = frame;
        buf._dirty = true;
        if (buf === historyBuf) {
            seekSlider.value = frame;
            updateSeekTimeDisplay();
        }
        buf.ensureLoaded(buildReplayMetricMask());
        requestRedraw();
        return;
    }
    // Live mode: timeMs is performance.now() timestamp — set scroll offset
    const offsetMs = performance.now() - timeMs;
    store.liveScrollOffsetMs = Math.max(0, Math.min(store.maxScrollOffsetMs(), offsetMs));
    updateLiveScrollBadge();
    store._dirty = true;
    requestRedraw();
}

seekSlider.addEventListener('input', (e) => {
    const frame = parseInt(e.target.value);
    if (!historyMode) enterHistoryMode();
    _histScrubbing = true;
    historyBuf.cursor = frame;
    historyBuf._dirty = true;
    updateSeekTimeDisplay();
    // Throttle fetch during scrubbing
    const now = performance.now();
    if (now - _seekThrottleTime >= 250) {
        _seekThrottleTime = now;
        if (historyBuf._abortController) historyBuf._abortController.abort();
        historyBuf.ensureLoaded(buildReplayMetricMask());
    }
    requestRedraw();
});

seekSlider.addEventListener('change', (e) => {
    if (!historyMode) return;
    _histScrubbing = false;
    historyBuf.cursor = parseInt(e.target.value);
    historyBuf._dirty = true;
    if (historyBuf._abortController) historyBuf._abortController.abort();
    historyBuf.ensureLoaded(buildReplayMetricMask());
    requestRedraw();
});

seekLiveBtn.addEventListener('click', () => {
    exitHistoryMode();
    resetLiveScrollOffset();
});

function updateSeekBar() {
    // Hide during replay
    if (replayBuf.count > 0 || (typeof replayPlayer !== 'undefined' && replayPlayer.active)) {
        seekBar.classList.remove('active');
        return;
    }
    seekBar.classList.add('active');
    if (!historyInfo) return;

    const total = historyInfo.total_frames;
    seekSlider.max = Math.max(0, total - 1);
    if (!historyMode) {
        seekSlider.value = seekSlider.max;
    }
    updateSeekTimeDisplay();
    updateSeekLapMarkers();
}

function updateSeekTimeDisplay() {
    if (!historyInfo || historyInfo.total_frames === 0) {
        seekTime.textContent = '0:00';
        return;
    }
    if (!historyMode) {
        seekTime.textContent = '0:00';
        return;
    }
    // Show negative offset from live edge
    const total = historyInfo.total_frames;
    const tickRate = historyInfo.tick_rate || 60;
    const offsetFrames = total - 1 - historyBuf.cursor;
    const offsetSecs = offsetFrames / tickRate;
    const m = Math.floor(offsetSecs / 60);
    const s = Math.floor(offsetSecs % 60);
    seekTime.textContent = `-${m}:${String(s).padStart(2, '0')}`;
}

let _lastLapMarkersHash = '';
function updateSeekLapMarkers() {
    if (!historyInfo || !historyInfo.laps) return;
    const total = historyInfo.total_frames;
    if (total <= 0) return;
    // Quick hash to avoid rebuilding every frame
    const hash = historyInfo.laps.map(l => `${l.lap_number}:${l.start_frame}`).join(',');
    if (hash === _lastLapMarkersHash) return;
    _lastLapMarkersHash = hash;

    seekLaps.innerHTML = '';
    for (let i = 0; i < historyInfo.laps.length; i++) {
        const lap = historyInfo.laps[i];
        const pct = (lap.start_frame / total) * 100;
        const tick = document.createElement('div');
        tick.className = 'seek-lap-tick';
        tick.style.left = pct + '%';
        const label = document.createElement('span');
        label.className = 'seek-lap-label';
        label.textContent = 'L' + lap.lap_number;
        tick.appendChild(label);
        seekLaps.appendChild(tick);
    }
}

// ==================== History Info Polling ====================
const memoryEl = document.getElementById('header-memory');

async function fetchHistoryInfo() {
    try {
        const resp = await fetch(apiBase() + '/api/replay/info');
        if (!resp.ok) return;
        const info = await resp.json();
        if (info.mode === 'history') {
            historyInfo = info;
            updateMemoryDisplay();
            // If in history mode, update total frames for buffer
            if (historyMode) {
                historyBuf.totalFrames = info.total_frames;
                historyBuf.tickRate = info.tick_rate || 60;
            }
        } else if (info.mode === 'replay') {
            // Replay is active, clear history info
            historyInfo = null;
        }
    } catch (e) { /* ignore */ }
}

function updateMemoryDisplay() {
    if (!historyInfo) {
        memoryEl.textContent = '';
        return;
    }
    const mb = historyInfo.process_memory_mb;
    if (mb != null) {
        memoryEl.textContent = mb >= 1 ? `${mb.toFixed(0)} MB` : `${(mb * 1024).toFixed(0)} KB`;
    } else {
        memoryEl.textContent = '';
    }
}

// Poll history info
fetchHistoryInfo();
setInterval(fetchHistoryInfo, 10000);

// ==================== Settings Modal ====================
const PRESETS_KEY = 'ost-user-presets';
const PRESET_OVERRIDES_KEY = 'ost-preset-overrides';

function getUserPresets() {
    try { return JSON.parse(localStorage.getItem(PRESETS_KEY)) || []; } catch { return []; }
}
function saveUserPresets(presets) { localStorage.setItem(PRESETS_KEY, JSON.stringify(presets)); }
function getPresetOverrides() {
    try { return JSON.parse(localStorage.getItem(PRESET_OVERRIDES_KEY)) || {}; } catch { return {}; }
}
function savePresetOverrides(ov) { localStorage.setItem(PRESET_OVERRIDES_KEY, JSON.stringify(ov)); }
function getAllPresets() {
    const overrides = getPresetOverrides();
    const builtIn = GRAPH_PRESETS.map(p => overrides[p.name] || p);
    return [...builtIn, ...getUserPresets()];
}

function openSettingsModal() {
    if (document.getElementById('settings-modal')) return;
    const overlay = document.createElement('div');
    overlay.id = 'settings-modal';
    overlay.className = 'cm-overlay';
    const modal = document.createElement('div');
    modal.className = 'cm-modal';
    modal.style.width = '520px';

    // === History section ===
    const savedSecs = parseInt(localStorage.getItem(HISTORY_DURATION_KEY)) || DEFAULT_HISTORY_SECS;
    const histOptions = HISTORY_DURATION_OPTIONS.map(opt => {
        const memMb = Math.round(opt.secs * 60 * 3 / 1024);
        const selected = opt.secs === savedSecs ? 'selected' : '';
        return `<option value="${opt.secs}" ${selected}>${opt.label} (~${memMb} MB)</option>`;
    }).join('');

    const savedAutoSave = localStorage.getItem('ost-persistence-autosave') === 'true';
    const savedFreq = parseInt(localStorage.getItem('ost-persistence-freq')) || 60;
    const freqOptions = [10, 30, 60].map(hz => {
        const selected = hz === savedFreq ? 'selected' : '';
        return `<option value="${hz}" ${selected}>${hz} Hz</option>`;
    }).join('');

    modal.innerHTML = `
        <div class="cm-modal-title">Settings</div>
        <div class="settings-section-title">History</div>
        <div class="settings-row">
            <span class="settings-label">Buffer Duration</span>
            <select class="settings-select" id="settings-history-duration">${histOptions}</select>
        </div>
        <div class="settings-row">
            <span class="settings-label">Auto-Save</span>
            <input type="checkbox" id="settings-autosave" ${savedAutoSave ? 'checked' : ''}>
        </div>
        <div class="settings-row">
            <span class="settings-label">Save Rate</span>
            <select class="settings-select" id="settings-save-freq">${freqOptions}</select>
        </div>
        <div class="settings-section-title" style="margin-top:12px">Retention</div>
        <div class="settings-row">
            <span class="settings-label">Max Sessions</span>
            <select class="settings-select" id="settings-retention-max-sessions">
                <option value="">Unlimited</option>
                <option value="5">5</option>
                <option value="10">10</option>
                <option value="25">25</option>
                <option value="50">50</option>
                <option value="100">100</option>
            </select>
        </div>
        <div class="settings-row">
            <span class="settings-label">Max Age</span>
            <select class="settings-select" id="settings-retention-max-age">
                <option value="">Unlimited</option>
                <option value="7">7 days</option>
                <option value="14">14 days</option>
                <option value="30">30 days</option>
                <option value="60">60 days</option>
                <option value="90">90 days</option>
            </select>
        </div>
        <div class="settings-row">
            <span class="settings-label">Disk Usage</span>
            <span id="settings-disk-usage" style="color:#888;font-size:12px;">Loading...</span>
        </div>
        <div class="settings-divider"></div>
        <div class="settings-section-title">Dashboard Profiles</div>
        <div id="settings-profiles-list"></div>
        <div style="margin-top:8px;display:flex;gap:6px">
            <input type="text" class="cm-form-input" id="profile-name-input" placeholder="Profile name" style="flex:1">
            <button class="cm-btn cm-btn-save" id="settings-save-profile">Save Current</button>
        </div>
        <div class="settings-divider"></div>
        <div class="settings-section-title">Units</div>
        ${Object.entries(UNIT_SYSTEMS).map(([key, sys]) => {
            const prefs = getUnitPrefs();
            const opts = sys.options.map(o => `<option value="${o}" ${prefs[key] === o ? 'selected' : ''}>${o}</option>`).join('');
            return `<div class="settings-row">
                <span class="settings-label">${key.charAt(0).toUpperCase() + key.slice(1)}</span>
                <select class="settings-select unit-pref-select" data-unit-key="${key}">${opts}</select>
            </div>`;
        }).join('')}
        <div class="settings-divider"></div>
        <div class="settings-section-title">Output Sinks</div>
        <div id="settings-sinks-list" class="sink-list" style="margin-bottom:8px"><div class="no-data">No sinks configured</div></div>
        <div style="font-size:0.65rem;color:var(--text-muted);margin-bottom:4px">ADD UDP SINK</div>
        <form id="settings-sink-form" class="sink-form">
            <div class="sink-form-group"><div class="sink-form-label">Host</div><input type="text" id="settings-sk-host" placeholder="127.0.0.1" required></div>
            <div class="sink-form-group"><div class="sink-form-label">Port</div><input type="number" id="settings-sk-port" placeholder="9200" required></div>
            <div class="sink-form-group"><div class="sink-form-label">Update Rate</div><select id="settings-sk-rate"><option value="60">60 Hz</option><option value="30">30 Hz</option><option value="10">10 Hz</option><option value="1">1 Hz</option></select></div>
            <div class="sink-form-group"><div class="sink-form-label">Metric Filter</div><input type="text" id="settings-sk-mask" placeholder="e.g. rpm,speed,gear"></div>
            <button type="submit" class="btn-add">Add</button>
        </form>
        <div class="settings-divider"></div>
        <div class="settings-section-title">API</div>
        <div class="api-docs" style="font-size:0.65rem">
            <div class="api-section">
                <div class="api-heading">SSE Streams</div>
                <div class="api-endpoint"><code>GET /api/stream</code> — Unified stream (frame, status, sinks events)</div>
                <div class="api-endpoint"><code>GET /api/telemetry/stream</code> — Telemetry frames only</div>
                <div class="api-endpoint" style="margin-left:12px;font-size:0.55rem">Query params: <code>rate</code> (0.01–60), <code>metric_mask</code>, <code>delta</code> (true/false)</div>
                <div class="api-endpoint"><code>GET /api/status/stream</code> — Status updates only</div>
            </div>
            <div class="api-section">
                <div class="api-heading">REST Endpoints</div>
                <div class="api-endpoint"><code>GET /api/adapters</code> — List adapters and their status</div>
                <div class="api-endpoint"><code>POST /api/adapters/:name/toggle</code> — Enable/disable an adapter</div>
                <div class="api-endpoint"><code>GET /api/metrics</code> — Latest telemetry frame</div>
                <div class="api-endpoint"><code>GET /api/sinks</code> — List output sinks</div>
                <div class="api-endpoint"><code>POST /api/sinks</code> — Create UDP sink</div>
                <div class="api-endpoint"><code>DELETE /api/sinks/:id</code> — Remove a sink</div>
            </div>
            <div class="api-section">
                <div class="api-heading">Replay</div>
                <div class="api-endpoint"><code>POST /api/replay/upload</code> — Upload .ibt file (multipart)</div>
                <div class="api-endpoint"><code>GET /api/replay/info</code> — Current replay state</div>
                <div class="api-endpoint"><code>GET /api/replay/frames?start=0&count=100</code> — Fetch frames</div>
                <div class="api-endpoint"><code>POST /api/replay/control</code> — Play/pause/seek/speed</div>
            </div>
        </div>
        <div class="settings-divider"></div>
        <div class="settings-section-title">Graph Presets</div>
        <div id="settings-presets-list"></div>
        <div style="margin-top:8px">
            <button class="cm-btn cm-btn-test" id="settings-add-preset">+ Add Preset</button>
        </div>
        <div class="settings-divider"></div>
        <div class="cm-btn-row">
            <button class="cm-btn cm-btn-cancel" id="settings-close">Close</button>
        </div>
    `;

    overlay.appendChild(modal);
    overlay.addEventListener('click', (e) => { if (e.target === overlay) overlay.remove(); });
    document.body.appendChild(overlay);

    modal.querySelector('#settings-close').addEventListener('click', () => overlay.remove());

    // History duration
    const histSelect = modal.querySelector('#settings-history-duration');
    histSelect.addEventListener('change', () => {
        const secs = parseInt(histSelect.value);
        localStorage.setItem(HISTORY_DURATION_KEY, secs);
        fetch(apiBase() + '/api/history/config', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ max_duration_secs: secs })
        }).catch(() => {});
        fetchHistoryInfo();
    });

    // Persistence
    const autoSaveCb = modal.querySelector('#settings-autosave');
    const freqSelect = modal.querySelector('#settings-save-freq');
    function syncPersistenceConfig() {
        const auto_save = autoSaveCb.checked;
        const frequency_hz = parseInt(freqSelect.value);
        localStorage.setItem('ost-persistence-autosave', auto_save);
        localStorage.setItem('ost-persistence-freq', frequency_hz);
        fetch(apiBase() + '/api/persistence/config', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ auto_save, frequency_hz })
        }).catch(() => {});
        updateRecordingIndicator();
    }
    autoSaveCb.addEventListener('change', syncPersistenceConfig);
    freqSelect.addEventListener('change', syncPersistenceConfig);

    // Retention settings
    const retMaxSessions = modal.querySelector('#settings-retention-max-sessions');
    const retMaxAge = modal.querySelector('#settings-retention-max-age');
    const diskUsageSpan = modal.querySelector('#settings-disk-usage');

    // Load current retention config and stats
    fetch(apiBase() + '/api/persistence/config').then(r => r.json()).then(cfg => {
        if (cfg.retention) {
            retMaxSessions.value = cfg.retention.max_sessions != null ? String(cfg.retention.max_sessions) : '';
            retMaxAge.value = cfg.retention.max_age_days != null ? String(cfg.retention.max_age_days) : '';
        }
    }).catch(() => {});
    fetch(apiBase() + '/api/persistence/stats').then(r => r.json()).then(stats => {
        diskUsageSpan.textContent = `${stats.file_count} files, ${stats.total_size_mb} MB`;
    }).catch(() => { diskUsageSpan.textContent = 'N/A'; });

    function syncRetentionConfig() {
        const maxSessions = retMaxSessions.value ? parseInt(retMaxSessions.value) : null;
        const maxAge = retMaxAge.value ? parseInt(retMaxAge.value) : null;
        fetch(apiBase() + '/api/persistence/config', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ max_sessions: maxSessions, max_age_days: maxAge })
        }).then(() => {
            // Refresh disk usage after cleanup
            fetch(apiBase() + '/api/persistence/stats').then(r => r.json()).then(stats => {
                diskUsageSpan.textContent = `${stats.file_count} files, ${stats.total_size_mb} MB`;
            }).catch(() => {});
        }).catch(() => {});
    }
    retMaxSessions.addEventListener('change', syncRetentionConfig);
    retMaxAge.addEventListener('change', syncRetentionConfig);

    // Output Sinks
    const sinksListEl = modal.querySelector('#settings-sinks-list');
    function renderSinksList() {
        if (store.sinks.length === 0) {
            sinksListEl.innerHTML = '<div class="no-data">No sinks configured</div>';
        } else {
            sinksListEl.innerHTML = store.sinks.map(s => {
                const rate = s.update_rate_hz || 60;
                return `<div class="sink-item"><div><strong>UDP</strong> ${s.host}:${s.port} <span style="color:var(--text-muted);font-size:0.6rem">@ ${rate} Hz</span>${s.metric_mask ? `<br><span style="color:var(--text-muted);font-size:0.6rem">Metrics: ${s.metric_mask}</span>` : ''}</div><button class="btn-delete" data-id="${s.id}">Delete</button></div>`;
            }).join('');
            sinksListEl.querySelectorAll('.btn-delete').forEach(btn => {
                btn.addEventListener('click', async () => {
                    try {
                        await fetch(`${apiBase()}/api/sinks/${btn.dataset.id}`, { method: 'DELETE' });
                        // Re-render after short delay for SSE update
                        setTimeout(renderSinksList, 300);
                    } catch(e) { console.error(e); }
                });
            });
        }
    }
    renderSinksList();

    modal.querySelector('#settings-sink-form').addEventListener('submit', async (e) => {
        e.preventDefault();
        const config = {
            id: '',
            host: modal.querySelector('#settings-sk-host').value,
            port: parseInt(modal.querySelector('#settings-sk-port').value),
            update_rate_hz: parseFloat(modal.querySelector('#settings-sk-rate').value),
            metric_mask: modal.querySelector('#settings-sk-mask').value.trim() || null,
        };
        try {
            await fetch(apiBase() + '/api/sinks', { method: 'POST', headers: {'Content-Type':'application/json'}, body: JSON.stringify(config) });
            modal.querySelector('#settings-sk-host').value = '';
            modal.querySelector('#settings-sk-port').value = '';
            modal.querySelector('#settings-sk-mask').value = '';
            // Re-render after short delay for SSE update
            setTimeout(renderSinksList, 300);
        } catch(e) { console.error(e); }
    });

    // Unit preferences
    modal.querySelectorAll('.unit-pref-select').forEach(sel => {
        sel.addEventListener('change', () => {
            const prefs = getUnitPrefs();
            prefs[sel.dataset.unitKey] = sel.value;
            saveUnitPrefs(prefs);
        });
    });

    // Presets
    const presetsListEl = modal.querySelector('#settings-presets-list');

    function renderPresetsList() {
        presetsListEl.innerHTML = '';
        const userPresets = getUserPresets();
        const overrides = getPresetOverrides();

        // Built-in presets (editable via overrides)
        for (const builtIn of GRAPH_PRESETS) {
            const isOverridden = !!overrides[builtIn.name];
            const preset = isOverridden ? overrides[builtIn.name] : builtIn;
            const item = document.createElement('div');
            item.className = 'cm-list-item';
            const badge = isOverridden ? 'modified' : 'built-in';
            item.innerHTML = `
                <div style="flex:1;min-width:0">
                    <div class="preset-name">${preset.name} <span class="preset-builtin">${badge}</span></div>
                    <div class="preset-metrics">${preset.metrics.join(', ')}</div>
                </div>
                <button class="cm-btn-edit preset-edit-btn">Edit</button>
                ${isOverridden ? '<button class="cm-btn-del preset-reset-btn">Reset</button>' : ''}`;
            item.querySelector('.preset-edit-btn').addEventListener('click', () => {
                openBuiltinPresetEditor(builtIn.name, preset);
            });
            if (isOverridden) {
                item.querySelector('.preset-reset-btn').addEventListener('click', () => {
                    const ov = getPresetOverrides();
                    delete ov[builtIn.name];
                    savePresetOverrides(ov);
                    renderPresetsList();
                });
            }
            presetsListEl.appendChild(item);
        }

        // User presets (editable)
        for (let i = 0; i < userPresets.length; i++) {
            const preset = userPresets[i];
            const item = document.createElement('div');
            item.className = 'cm-list-item';
            item.innerHTML = `
                <div style="flex:1;min-width:0">
                    <div class="preset-name">${preset.name}</div>
                    <div class="preset-metrics">${preset.metrics.join(', ')}</div>
                </div>
                <button class="cm-btn-edit preset-edit-btn">Edit</button>
                <button class="cm-btn-del preset-del-btn">Del</button>`;
            item.querySelector('.preset-edit-btn').addEventListener('click', () => openPresetEditor(i));
            item.querySelector('.preset-del-btn').addEventListener('click', () => {
                userPresets.splice(i, 1);
                saveUserPresets(userPresets);
                renderPresetsList();
            });
            presetsListEl.appendChild(item);
        }
    }

    function openBuiltinPresetEditor(originalName, preset) {
        presetsListEl.innerHTML = `
            <div class="cm-form-row">
                <span class="cm-form-label">Name</span>
                <input type="text" class="cm-form-input" id="preset-name" value="${originalName}" disabled>
            </div>
            <div style="margin-bottom:4px">
                <span class="cm-form-label">Metrics</span>
            </div>
            <textarea class="cm-code-input" id="preset-metrics" rows="4">${preset.metrics.join('\n')}</textarea>
            <div class="cm-btn-row">
                <button class="cm-btn cm-btn-cancel" id="preset-cancel">Cancel</button>
                <button class="cm-btn cm-btn-save" id="preset-save">Save</button>
            </div>`;

        presetsListEl.querySelector('#preset-cancel').addEventListener('click', renderPresetsList);
        presetsListEl.querySelector('#preset-save').addEventListener('click', () => {
            const metricsText = presetsListEl.querySelector('#preset-metrics').value.trim();
            if (!metricsText) return;
            const metrics = metricsText.split('\n').map(l => l.trim()).filter(Boolean);
            const ov = getPresetOverrides();
            ov[originalName] = { name: originalName, metrics };
            savePresetOverrides(ov);
            renderPresetsList();
        });
    }

    function openPresetEditor(editIndex) {
        const userPresets = getUserPresets();
        const existing = editIndex !== undefined ? userPresets[editIndex] : null;
        presetsListEl.innerHTML = `
            <div class="cm-form-row">
                <span class="cm-form-label">Name</span>
                <input type="text" class="cm-form-input" id="preset-name" value="${existing ? existing.name : ''}" placeholder="e.g. Suspension FL">
            </div>
            <div style="margin-bottom:4px">
                <span class="cm-form-label">Metrics</span>
            </div>
            <textarea class="cm-code-input" id="preset-metrics" rows="4" placeholder="One per line. Supports wildcards (*) and /regex/.&#10;e.g. wheels.front_left.suspension_travel&#10;     wheels.*.brake_temp">${existing ? existing.metrics.join('\n') : ''}</textarea>
            <div class="cm-btn-row">
                <button class="cm-btn cm-btn-cancel" id="preset-cancel">Cancel</button>
                <button class="cm-btn cm-btn-save" id="preset-save">${existing ? 'Update' : 'Add'}</button>
            </div>`;

        presetsListEl.querySelector('#preset-cancel').addEventListener('click', renderPresetsList);
        presetsListEl.querySelector('#preset-save').addEventListener('click', () => {
            const name = presetsListEl.querySelector('#preset-name').value.trim();
            const metricsText = presetsListEl.querySelector('#preset-metrics').value.trim();
            if (!name || !metricsText) return;
            const metrics = metricsText.split('\n').map(l => l.trim()).filter(Boolean);
            const fresh = getUserPresets();
            if (editIndex !== undefined) {
                fresh[editIndex] = { name, metrics };
            } else {
                fresh.push({ name, metrics });
            }
            saveUserPresets(fresh);
            renderPresetsList();
        });
    }

    modal.querySelector('#settings-add-preset').addEventListener('click', () => openPresetEditor());
    renderPresetsList();

    // Dashboard Profiles
    const PROFILES_KEY = 'ost-dashboard-profiles';
    function getProfiles() {
        try { return JSON.parse(localStorage.getItem(PROFILES_KEY)) || []; } catch { return []; }
    }

    const profilesListEl = modal.querySelector('#settings-profiles-list');
    function renderProfilesList() {
        const profiles = getProfiles();
        profilesListEl.innerHTML = '';
        if (profiles.length === 0) {
            profilesListEl.innerHTML = '<div class="no-data" style="font-size:0.7rem;color:var(--text-muted);padding:4px 0">No saved profiles</div>';
            return;
        }
        for (let i = 0; i < profiles.length; i++) {
            const p = profiles[i];
            const item = document.createElement('div');
            item.className = 'cm-list-item';
            item.innerHTML = `
                <div style="flex:1;min-width:0">
                    <div class="preset-name">${p.name}</div>
                    <div class="preset-metrics">${(p.graphs || []).length} graph(s)</div>
                </div>`;

            const btnWrap = document.createElement('div');
            btnWrap.style.cssText = 'display:flex;gap:4px;flex-shrink:0';

            const loadBtn = document.createElement('button');
            loadBtn.className = 'cm-btn cm-btn-save';
            loadBtn.textContent = 'Load';
            loadBtn.addEventListener('click', () => {
                // Save current layout/graphs keys, then reload
                if (p.layout) localStorage.setItem(LAYOUT_KEY, JSON.stringify(p.layout));
                if (p.graphs) localStorage.setItem(GRAPHS_KEY, JSON.stringify(p.graphs));
                localStorage.setItem(LAYOUT_VERSION_KEY, LAYOUT_VERSION);
                location.reload();
            });

            const delBtn = document.createElement('button');
            delBtn.className = 'cm-btn-del';
            delBtn.textContent = 'Del';
            delBtn.addEventListener('click', () => {
                const fresh = getProfiles();
                fresh.splice(i, 1);
                localStorage.setItem(PROFILES_KEY, JSON.stringify(fresh));
                renderProfilesList();
            });

            btnWrap.appendChild(loadBtn);
            btnWrap.appendChild(delBtn);
            item.appendChild(btnWrap);
            profilesListEl.appendChild(item);
        }
    }

    modal.querySelector('#settings-save-profile').addEventListener('click', () => {
        const nameInput = modal.querySelector('#profile-name-input');
        const name = nameInput.value.trim();
        if (!name) { nameInput.focus(); return; }

        // Capture current layout
        const layouts = {};
        for (const node of grid.gs.getGridItems()) {
            const w = node.querySelector('.widget')?._widget;
            if (!w) continue;
            const n = node.gridstackNode;
            layouts[w.id] = { x: n.x, y: n.y, w: n.w, h: n.h };
        }

        // Capture current graph configs
        const graphs = [];
        for (const [id, w] of grid.widgets) {
            if (w instanceof GraphWidget) graphs.push(w.getConfig());
        }

        const profiles = getProfiles();
        profiles.push({ name, layout: layouts, graphs });
        localStorage.setItem(PROFILES_KEY, JSON.stringify(profiles));
        nameInput.value = '';
        renderProfilesList();
    });

    renderProfilesList();
}

document.getElementById('settings-btn').addEventListener('click', openSettingsModal);

// Recording indicator — syncs with auto-save state
const recordingDot = document.getElementById('recording-dot');
function updateRecordingIndicator() {
    const active = localStorage.getItem('ost-persistence-autosave') === 'true';
    recordingDot.classList.toggle('active', active);
}
updateRecordingIndicator();

// Sync saved preferences to server on page load
(function syncSavedConfigs() {
    const savedSecs = parseInt(localStorage.getItem(HISTORY_DURATION_KEY));
    if (savedSecs) {
        fetch(apiBase() + '/api/history/config', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ max_duration_secs: savedSecs })
        }).catch(() => {});
    }
    // Sync persistence config
    const autoSave = localStorage.getItem('ost-persistence-autosave') === 'true';
    const freq = parseInt(localStorage.getItem('ost-persistence-freq')) || 60;
    fetch(apiBase() + '/api/persistence/config', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ auto_save: autoSave, frequency_hz: freq })
    }).catch(() => {});
})();

// Start
connectSSE();
requestAnimationFrame(renderLoop);

// Replay: keyboard shortcuts + drag-drop + init
const replayPlayer = new ReplayPlayer(replayBuf);

document.addEventListener('keydown', (e) => {
    replayPlayer.handleKeydown(e);
});
let dragCounter = 0;
const dropOverlay = document.getElementById('drop-overlay');

document.body.addEventListener('dragenter', (e) => {
    e.preventDefault();
    if (replayPlayer.active) return;
    dragCounter++;
    dropOverlay.classList.add('active');
});
document.body.addEventListener('dragleave', (e) => {
    e.preventDefault();
    dragCounter--;
    if (dragCounter <= 0) { dragCounter = 0; dropOverlay.classList.remove('active'); }
});
document.body.addEventListener('dragover', (e) => e.preventDefault());
document.body.addEventListener('drop', (e) => {
    e.preventDefault();
    dragCounter = 0;
    dropOverlay.classList.remove('active');
    if (!replayPlayer.active && e.dataTransfer.files.length > 0) {
        const file = e.dataTransfer.files[0];
        if (file.name.toLowerCase().endsWith('.ibt')) replayPlayer.upload(file);
    }
});

// Check for existing replay on page load
async function checkReplayOnLoad() {
    try {
        const resp = await fetch(apiBase() + '/api/replay/info');
        if (resp.ok) {
            const info = await resp.json();
            if (info.mode === 'replay') {
                replayPlayer.info = info;
                replayPlayer.active = true;
                replayPlayer.currentSpeed = info.playback_speed;
                await replayPlayer.enterReplayMode();
            }
        }
    } catch (e) {
        console.error('Failed to restore replay state:', e);
    }
}
checkReplayOnLoad();
