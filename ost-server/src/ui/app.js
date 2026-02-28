/* ==================== Initialization ==================== */
const store = new TelemetryStore();
const replayBuf = new ReplayBuffer();
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
function renderLoop() {
    const now = performance.now();
    const activeReplay = replayBuf.count > 0 ? replayBuf : null;

    // Advance client-side cursor and sync store for all widgets
    if (activeReplay) {
        replayBuf.advancePlayback(now);
        // Ensure chunks around cursor are loaded (prefetches ahead during playback)
        if (replayBuf.needsFetch() || replayBuf.playing) {
            replayBuf.ensureLoaded(buildReplayMetricMask());
        }
        // Sync store.currentFrame so non-graph widgets see the replay frame
        const replayFrame = replayBuf.currentFrame();
        if (replayFrame) {
            store.currentFrame = replayFrame;
            store._dirty = true;
        }
        // Update replay controls (slider, time)
        if (typeof replayPlayer !== 'undefined' && replayPlayer.active) {
            replayPlayer.updateControlsFromBuf();
        }
    }

    if (store._dirty || _uiDirty || (activeReplay && replayBuf._dirty)) {
        store._dirty = false;
        _uiDirty = false;
        if (activeReplay) replayBuf._dirty = false;
        for (const w of grid.widgets.values()) {
            if (w._visible) w.update(store, now, activeReplay);
        }
    }
    requestAnimationFrame(renderLoop);
}

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
            replayPlayer.info = info;
            replayPlayer.active = true;
            replayPlayer.currentSpeed = info.playback_speed;
            await replayPlayer.enterReplayMode();
        }
    } catch (e) {
        console.error('Failed to restore replay state:', e);
    }
})();
</script>
