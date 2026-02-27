/* ==================== TelemetryStore ==================== */
class TelemetryStore {
    constructor() {
        this.currentFrame = null;
        this.adapters = [];
        this.sinks = [];
        this._dirty = false;
        // Ring buffer: fixed-size array with write pointer
        this._ring = new Array(BUFFER_MAX);
        this._head = 0;  // next write position
        this._count = 0; // number of valid entries
    }

    pushFrame(frame) {
        this.currentFrame = frame;
        this._dirty = true;
        const entry = { t: performance.now(), _frame: frame };
        for (let i = 0; i < GRAPH_METRIC_KEYS.length; i++) {
            const key = GRAPH_METRIC_KEYS[i];
            entry[key] = GRAPH_METRICS[key].extract(frame);
        }
        this._ring[this._head] = entry;
        this._head = (this._head + 1) % BUFFER_MAX;
        if (this._count < BUFFER_MAX) this._count++;
    }

    // Returns { start, count } indices into ring for the given time window.
    // Caller uses ringAt(idx) to access entries without allocating an array.
    getWindowRange(durationMs, now) {
        if (this._count === 0) return { start: 0, count: 0 };
        const cutoff = now - durationMs;
        const oldest = this._count < BUFFER_MAX ? 0 : this._head;
        // Binary search for first entry >= cutoff
        let lo = 0, hi = this._count;
        while (lo < hi) {
            const mid = (lo + hi) >>> 1;
            if (this._ring[(oldest + mid) % BUFFER_MAX].t < cutoff) lo = mid + 1;
            else hi = mid;
        }
        return { start: (oldest + lo) % BUFFER_MAX, count: this._count - lo };
    }

    ringAt(idx) {
        return this._ring[idx % BUFFER_MAX];
    }

    latestTime() {
        if (this._count === 0) return null;
        return this._ring[(this._head - 1 + BUFFER_MAX) % BUFFER_MAX].t;
    }
}

/* ==================== ReplayBuffer ==================== */
// Client-side cache for replay frames, enabling instant scrubbing and centered graph windows.
class ReplayBuffer {
    constructor() {
        this.entries = [];       // Processed entries (same format as ring: t, speed, rpm, ..., _frame)
        this.startFrame = 0;     // Frame index of first entry in cache
        this.count = 0;          // Number of loaded entries
        this.totalFrames = 0;    // Total frames in replay
        this.tickRate = 60;      // From replay info
        this.cursor = 0;         // Current frame index (absolute)
        this.playing = false;
        this.playbackSpeed = 1;
        this._fetching = false;
        this._fetchDebounce = null;
        this._dirty = false;
        this._lastPlayTick = null;
    }

    // Sim-time in ms for a frame index
    simTimeMs(frameIndex) { return (frameIndex / this.tickRate) * 1000; }

    // Check if a frame index is in the cache
    has(frameIndex) { return frameIndex >= this.startFrame && frameIndex < this.startFrame + this.count; }

    // Get processed entry by absolute frame index
    getEntry(frameIndex) {
        if (!this.has(frameIndex)) return null;
        return this.entries[frameIndex - this.startFrame];
    }

    // Get current frame entry
    currentEntry() { return this.getEntry(this.cursor); }

    // Get current TelemetryFrame (for store.currentFrame compatibility)
    currentFrame() {
        const e = this.currentEntry();
        return e ? e._frame : null;
    }

    // Process a raw TelemetryFrame into a ring-compatible entry
    _processFrame(frame, frameIndex) {
        const entry = { t: this.simTimeMs(frameIndex), _frame: frame };
        for (let i = 0; i < GRAPH_METRIC_KEYS.length; i++) {
            const key = GRAPH_METRIC_KEYS[i];
            entry[key] = GRAPH_METRICS[key].extract(frame);
        }
        return entry;
    }

    // Load fetched frames into the cache
    loadFrames(serverFrames) {
        if (serverFrames.length === 0) return;
        this.startFrame = serverFrames[0].i;
        this.entries = serverFrames.map(sf => this._processFrame(sf.f, sf.i));
        this.count = this.entries.length;
        this._dirty = true;
    }

    // Fetch a window of frames centered on the cursor
    async fetchWindow(windowFrames, fields) {
        if (this._fetching) return;
        const half = Math.floor(windowFrames / 2);
        const start = Math.max(0, this.cursor - half);
        const count = Math.min(windowFrames, this.totalFrames - start);
        this._fetching = true;
        try {
            let url = `/api/replay/frames?start=${start}&count=${count}`;
            if (fields) url += `&fields=${encodeURIComponent(fields)}`;
            const resp = await fetch(url);
            if (!resp.ok) throw new Error(await resp.text());
            const frames = await resp.json();
            this.loadFrames(frames);
        } catch (e) {
            console.error('Failed to fetch replay frames:', e);
        } finally {
            this._fetching = false;
        }
    }

    // Debounced fetch for rapid scrubbing
    fetchWindowDebounced(windowFrames, delayMs = 200, fields) {
        clearTimeout(this._fetchDebounce);
        this._fetchDebounce = setTimeout(() => this.fetchWindow(windowFrames, fields), delayMs);
    }

    // Check if cursor is near cache boundary and needs re-fetch
    needsFetch(marginFrames = 300) {
        if (this.count === 0) return true;
        if (this.cursor < this.startFrame + marginFrames) return true;
        if (this.cursor > this.startFrame + this.count - marginFrames) return true;
        return false;
    }

    // Get entries for a centered time window around cursor.
    // Returns { entries, startIdx, count, centerTime } for rendering.
    getWindowEntries(windowMs) {
        if (this.count === 0) return { entries: [], startIdx: 0, count: 0, centerTime: 0 };
        const halfFrames = Math.floor((windowMs / 1000) * this.tickRate / 2);
        const from = Math.max(this.startFrame, this.cursor - halfFrames);
        const to = Math.min(this.startFrame + this.count - 1, this.cursor + halfFrames);
        const localFrom = from - this.startFrame;
        const localTo = to - this.startFrame;
        return {
            entries: this.entries,
            startIdx: localFrom,
            count: localTo - localFrom + 1,
            centerTime: this.simTimeMs(this.cursor),
        };
    }

    // Advance cursor during playback (called from requestAnimationFrame)
    advancePlayback(now) {
        if (!this.playing) { this._lastPlayTick = null; return; }
        if (this._lastPlayTick == null) { this._lastPlayTick = now; return; }
        const elapsed = (now - this._lastPlayTick) / 1000; // seconds
        this._lastPlayTick = now;
        const framesToAdvance = elapsed * this.tickRate * this.playbackSpeed;
        this.cursor = Math.min(
            Math.round(this.cursor + framesToAdvance),
            this.totalFrames - 1
        );
        if (this.cursor >= this.totalFrames - 1) {
            this.playing = false;
        }
        this._dirty = true;
    }

    reset() {
        this.entries = [];
        this.startFrame = 0;
        this.count = 0;
        this.cursor = 0;
        this.playing = false;
        this._lastPlayTick = null;
        this._dirty = false;
    }
}

/* ==================== SSEConnection ==================== */
class SSEConnection {
    constructor(url, onFrame, onStatusChange) {
        this.url = url;
        this.onFrame = onFrame;
        this.onStatusChange = onStatusChange;
        this.es = null;
    }

    connect() {
        if (this.es) this.es.close();
        this.es = new EventSource(this.url);
        this.es.onopen = () => { this.onStatusChange(true); };
        this.es.onerror = () => {
            this.onStatusChange(false);
            this.es.close();
            setTimeout(() => this.connect(), 5000);
        };
        this.es.onmessage = (e) => {
            try { this.onFrame(JSON.parse(e.data)); }
            catch (err) { console.error('Parse error:', err); }
        };
    }
}

/* ==================== Widget Visibility Observer ==================== */
const widgetVisibilityObserver = new IntersectionObserver((entries) => {
    for (const entry of entries) {
        const widget = entry.target._widget;
        if (widget) widget._visible = entry.isIntersecting;
    }
}, { rootMargin: '50px' });

