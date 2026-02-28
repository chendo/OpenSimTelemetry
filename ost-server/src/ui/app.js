/* ==================== Initialization ==================== */
const store = new TelemetryStore();
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
            text = `Receiving from ${active.name}`;
            dotClass = 'dot-active';
        } else {
            text = 'Waiting for data';
            dotClass = 'dot-detected';
        }
    }
    connEl.innerHTML = `<span class="status-dot ${dotClass}"></span><span>${text}</span>`;
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

// Single unified SSE connection (avoids exhausting HTTP/1.1 connection slots)
let sseSource = null;
function connectSSE() {
    if (sseSource) sseSource.close();
    const es = new EventSource('/api/stream');
    sseSource = es;
    es.onopen = () => {
        sseConnected = true; sseEverConnected = true; updateStatus();
        if (!_throughputInterval) _throughputInterval = setInterval(updateThroughput, 500);
    };
    es.onerror = () => {
        sseConnected = false; updateStatus(); es.close();
        _frameTimestamps = [];
        updateThroughput();
        setTimeout(connectSSE, 5000);
    };
    es.addEventListener('frame', (e) => {
        try {
            if (!streamPaused) store.pushFrame(JSON.parse(e.data));
            _frameTimestamps.push(performance.now());
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
    new LapTimingWidget(),
    new SessionWidget(),
    new AllMetricsWidget(),
    new OutputSinksWidget(),
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
    const defaultGraph = new GraphWidget('graph', { col: 1, row: 9, width: 12, height: 6 }, ['speed', 'rpm', 'throttle', 'brake']);
    defaultGraph.init();
    grid.addWidget(defaultGraph);
}

grid.gs.batchUpdate(false);
grid.restoreLayout();

// Add Graph button
let graphCounter = Date.now();
document.getElementById('header-add-graph').addEventListener('click', () => {
    const id = 'graph-' + (graphCounter++);
    const gw = new GraphWidget(id, { col: 1, row: 100, width: 12, height: 6 }, ['speed', 'rpm', 'throttle', 'brake']);
    gw.init();
    grid.addWidget(gw);
    grid.saveLayout();
    grid.saveGraphConfigs();
});

// Sources dropdown
const sourcesBtn = document.getElementById('sources-btn');
const sourcesMenu = document.getElementById('sources-menu');
let sourcesOpen = false;

sourcesBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    sourcesOpen = !sourcesOpen;
    sourcesMenu.classList.toggle('open', sourcesOpen);
});
document.addEventListener('click', () => { sourcesOpen = false; sourcesMenu.classList.remove('open'); });
sourcesMenu.addEventListener('click', (e) => e.stopPropagation());

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
                await fetch(`/api/adapters/${key}/toggle`, { method: 'POST' });
                // Status SSE will push the updated adapter list
            } catch (e) { console.error('Toggle adapter failed:', e); }
        });
    });
    // Update button to show active count
    const activeCount = store.adapters.filter(a => a.active).length;
    sourcesBtn.textContent = activeCount > 0 ? `Sources (${activeCount})` : 'Sources';
}

// Pause/resume streaming button
const pauseBtn = document.getElementById('header-pause-btn');
pauseBtn.addEventListener('click', () => {
    streamPaused = !streamPaused;
    pauseBtn.textContent = streamPaused ? 'Resume' : 'Pause';
    pauseBtn.style.borderColor = streamPaused ? 'var(--accent)' : '';
    pauseBtn.style.color = streamPaused ? 'var(--accent)' : '';
    // Sync pause state to server history buffer
    fetch('/api/replay/control', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ action: streamPaused ? 'pause' : 'play' })
    }).catch(() => {});
    updateStatus();
});

// Reset layout button
document.getElementById('header-reset-layout').addEventListener('click', () => grid.resetLayout());

// Load .ibt button
const ibtFileInput = document.getElementById('ibt-file-input');
document.getElementById('header-load-ibt').addEventListener('click', () => ibtFileInput.click());
ibtFileInput.addEventListener('change', () => {
    const file = ibtFileInput.files[0];
    if (file && file.name.toLowerCase().endsWith('.ibt')) replayPlayer.upload(file);
    ibtFileInput.value = '';
});

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

    // Update seek bar
    updateSeekBar();

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

seekLiveBtn.addEventListener('click', exitHistoryMode);

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
        const resp = await fetch('/api/replay/info');
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
    const mb = historyInfo.estimated_memory_mb || 0;
    memoryEl.textContent = mb >= 1 ? `${mb.toFixed(0)} MB` : `${(mb * 1024).toFixed(0)} KB`;
}

// Poll history info
fetchHistoryInfo();
setInterval(fetchHistoryInfo, 10000);

// ==================== Settings Dropdown ====================
const settingsBtn = document.getElementById('settings-btn');
const settingsMenu = document.getElementById('settings-menu');
let settingsOpen = false;

function buildSettingsMenu() {
    const savedSecs = parseInt(localStorage.getItem(HISTORY_DURATION_KEY)) || DEFAULT_HISTORY_SECS;
    const options = HISTORY_DURATION_OPTIONS.map(opt => {
        const memMb = Math.round(opt.secs * 60 * 3 / 1024);
        const selected = opt.secs === savedSecs ? 'selected' : '';
        return `<option value="${opt.secs}" ${selected}>${opt.label} (~${memMb} MB)</option>`;
    }).join('');

    settingsMenu.innerHTML = `
        <div class="settings-row">
            <span class="settings-label">History Buffer</span>
            <select class="settings-select" id="settings-history-duration">${options}</select>
        </div>
        <div class="settings-hint" id="settings-memory-hint"></div>
    `;

    const select = document.getElementById('settings-history-duration');
    select.addEventListener('change', () => {
        const secs = parseInt(select.value);
        localStorage.setItem(HISTORY_DURATION_KEY, secs);
        fetch('/api/history/config', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ max_duration_secs: secs })
        }).catch(() => {});
        fetchHistoryInfo();
    });
}
buildSettingsMenu();

settingsBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    settingsOpen = !settingsOpen;
    settingsMenu.classList.toggle('open', settingsOpen);
});
document.addEventListener('click', () => { settingsOpen = false; settingsMenu.classList.remove('open'); });
settingsMenu.addEventListener('click', (e) => e.stopPropagation());

// Sync saved history preference to server on page load
(function syncHistoryConfig() {
    const savedSecs = parseInt(localStorage.getItem(HISTORY_DURATION_KEY));
    if (savedSecs) {
        fetch('/api/history/config', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ max_duration_secs: savedSecs })
        }).catch(() => {});
    }
})();

// Start
connectSSE();
requestAnimationFrame(renderLoop);

// Replay: drag-drop and init
const replayPlayer = new ReplayPlayer(replayBuf);
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
(async () => {
    try {
        const resp = await fetch('/api/replay/info');
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
})();
</script>
