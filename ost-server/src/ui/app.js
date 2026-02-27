/* ==================== Initialization ==================== */
const store = new TelemetryStore();
const replayBuf = new ReplayBuffer();
const grid = new DashboardGrid(document.getElementById('dashboard-grid'));

// Global function for GraphWidget to call when config changes
function dashboardSaveGraphs() { grid.saveGraphConfigs(); }

// SSE connection
const connEl = document.getElementById('header-conn');
const sse = new SSEConnection('/api/telemetry/stream',
    (frame) => { if (!streamPaused && replayBuf.count === 0) store.pushFrame(frame); },
    (connected) => {
        connEl.innerHTML = `<span class="status-dot ${connected ? 'dot-active' : 'dot-inactive'}"></span><span>${connected ? 'Connected' : 'Disconnected'}</span>`;
    }
);

// Create all widgets in a batch to avoid excessive change events
grid.gs.batchUpdate();

const staticWidgets = [
    new VehicleWidget(),
    new GForceWidget(),
    new OrientationWidget(),
    new SuspensionWidget(),
    new LapTimingWidget(),
    new SessionWidget(),
    new AllFieldsWidget(),
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
    const defaultGraph = new GraphWidget('graph', { col: 1, row: 7, width: 12, height: 6 }, ['speed', 'rpm', 'throttle', 'brake']);
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

// Header: adapter status dots
const headerAdapters = document.getElementById('header-adapters');
function updateHeaderAdapters() {
    headerAdapters.innerHTML = store.adapters.map(a =>
        `<span class="header-adapter-item"><span class="status-dot ${a.active ? 'dot-active' : a.detected ? 'dot-detected' : 'dot-inactive'}"></span>${a.name}</span>`
    ).join('');
}

// Pause/resume streaming button
const pauseBtn = document.getElementById('header-pause-btn');
pauseBtn.addEventListener('click', () => {
    streamPaused = !streamPaused;
    pauseBtn.textContent = streamPaused ? 'Resume' : 'Pause';
    pauseBtn.style.borderColor = streamPaused ? 'var(--accent)' : '';
    pauseBtn.style.color = streamPaused ? 'var(--accent)' : '';
});

// Reset layout button
document.getElementById('header-reset-layout').addEventListener('click', () => grid.resetLayout());

// Render loop (decoupled from SSE; also redraws on UI interactions like hover/toggle)
function renderLoop() {
    const now = performance.now();
    const activeReplay = replayBuf.count > 0 ? replayBuf : null;

    // Advance client-side replay playback
    if (activeReplay) {
        replayBuf.advancePlayback(now);
        // Pre-fetch when cursor nears cache boundary
        if (replayBuf.playing && replayBuf.needsFetch()) {
            replayBuf.fetchWindowDebounced(7200, 100);
        }
        // Update store.currentFrame so dashboard widgets get values
        const curEntry = replayBuf.currentEntry();
        if (curEntry) store.currentFrame = curEntry._frame;
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

// Polling: adapters every 30s, sinks every 30s (also refreshed on mutation)
async function pollAdapters() {
    try { store.adapters = await (await fetch('/api/adapters')).json(); updateHeaderAdapters(); } catch {}
}
async function pollSinks() {
    try { store.sinks = await (await fetch('/api/sinks')).json(); } catch {}
}

// Start
sse.connect();
pollAdapters(); pollSinks();
setInterval(pollAdapters, 30000);
setInterval(pollSinks, 30000);
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
            replayPlayer.enterReplayMode();
        }
    } catch {}
})();
</script>
