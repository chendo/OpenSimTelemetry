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
// Client-side cache for replay frames using 5-second chunk fetching.
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
        this._chunkSize = 300;       // frames per chunk (tickRate * 5, set in enterReplayMode)
        this._maxCacheFrames = 7200; // max cached frames (tickRate * 120, set in enterReplayMode)
        this._fetchingChunks = new Set(); // in-flight chunk indices
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

    // Chunk index for a given frame
    _chunkIndex(frame) { return Math.floor(frame / this._chunkSize); }

    // Check if a chunk is fully within the cache
    _hasChunk(chunkIdx) {
        const chunkStart = chunkIdx * this._chunkSize;
        const chunkEnd = Math.min(chunkStart + this._chunkSize, this.totalFrames);
        return chunkStart >= this.startFrame && chunkEnd <= this.startFrame + this.count;
    }

    // Fetch a single chunk, merge into cache
    async _fetchChunk(chunkIdx, fields) {
        if (this._hasChunk(chunkIdx) || this._fetchingChunks.has(chunkIdx)) return;
        const start = chunkIdx * this._chunkSize;
        if (start >= this.totalFrames) return;
        const count = Math.min(this._chunkSize, this.totalFrames - start);
        this._fetchingChunks.add(chunkIdx);
        try {
            let url = `/api/replay/frames?start=${start}&count=${count}`;
            if (fields) url += `&fields=${encodeURIComponent(fields)}`;
            const resp = await fetch(url);
            if (!resp.ok) throw new Error(await resp.text());
            const frames = await resp.json();
            this._mergeFrames(frames);
        } catch (e) {
            console.error(`Failed to fetch chunk ${chunkIdx}:`, e);
        } finally {
            this._fetchingChunks.delete(chunkIdx);
        }
    }

    // Merge fetched frames into the contiguous cache
    _mergeFrames(serverFrames) {
        if (serverFrames.length === 0) return;
        const newStart = serverFrames[0].i;
        const newEnd = newStart + serverFrames.length;
        const processed = serverFrames.map(sf => this._processFrame(sf.f, sf.i));

        if (this.count === 0) {
            // Empty cache — just set it
            this.startFrame = newStart;
            this.entries = processed;
            this.count = processed.length;
        } else {
            const cacheEnd = this.startFrame + this.count;
            // Check if new data is adjacent or overlapping with existing cache
            if (newStart <= cacheEnd && newEnd >= this.startFrame) {
                // Merge: compute union range
                const mergedStart = Math.min(this.startFrame, newStart);
                const mergedEnd = Math.max(cacheEnd, newEnd);
                const mergedLen = mergedEnd - mergedStart;
                const merged = new Array(mergedLen);
                // Copy existing entries
                for (let i = 0; i < this.count; i++) {
                    merged[this.startFrame - mergedStart + i] = this.entries[i];
                }
                // Overlay new entries (overwrites overlapping region)
                for (let i = 0; i < processed.length; i++) {
                    merged[newStart - mergedStart + i] = processed[i];
                }
                this.startFrame = mergedStart;
                this.entries = merged;
                this.count = mergedLen;
            } else {
                // Non-adjacent — check if new data is closer to cursor
                const cursorDistOld = Math.abs(this.cursor - (this.startFrame + this.count / 2));
                const cursorDistNew = Math.abs(this.cursor - (newStart + processed.length / 2));
                if (cursorDistNew < cursorDistOld) {
                    // Replace cache with new data (cursor moved far away)
                    this.startFrame = newStart;
                    this.entries = processed;
                    this.count = processed.length;
                }
                // else: stale fetch from old position, ignore
            }
        }
        this._trimCache();
        this._dirty = true;
    }

    // Trim cache to _maxCacheFrames, keeping data centered on cursor
    _trimCache() {
        if (this.count <= this._maxCacheFrames) return;
        const excess = this.count - this._maxCacheFrames;
        const cursorLocal = this.cursor - this.startFrame;
        // Determine how much to trim from each end
        const distToStart = cursorLocal;
        const distToEnd = this.count - 1 - cursorLocal;
        if (distToStart > distToEnd) {
            // Trim from start
            const trim = Math.min(excess, distToStart - Math.floor(this._maxCacheFrames / 2));
            if (trim > 0) {
                this.entries = this.entries.slice(trim);
                this.startFrame += trim;
                this.count -= trim;
            }
        }
        if (this.count > this._maxCacheFrames) {
            // Trim from end
            const trim = this.count - this._maxCacheFrames;
            this.entries.length = this._maxCacheFrames;
            this.count = this._maxCacheFrames;
        }
    }

    // Main entry point: ensure cursor's region is loaded, prefetch adjacent chunks
    async ensureLoaded(fields) {
        const cursorChunk = this._chunkIndex(this.cursor);
        // Await cursor chunk so UI can render immediately
        await this._fetchChunk(cursorChunk, fields);
        // Fire-and-forget: adjacent chunks
        if (cursorChunk > 0) this._fetchChunk(cursorChunk - 1, fields);
        const maxChunk = this._chunkIndex(this.totalFrames - 1);
        if (cursorChunk < maxChunk) this._fetchChunk(cursorChunk + 1, fields);
        // During playback, prefetch further ahead
        if (this.playing) {
            for (let i = 2; i <= 4; i++) {
                const ahead = cursorChunk + i;
                if (ahead <= maxChunk) this._fetchChunk(ahead, fields);
            }
        }
    }

    // Debounced ensureLoaded for rapid scrubbing
    ensureLoadedDebounced(delayMs = 200, fields) {
        clearTimeout(this._fetchDebounce);
        this._fetchDebounce = setTimeout(() => this.ensureLoaded(fields), delayMs);
    }

    // Check if cursor is near cache boundary and needs fetch
    needsFetch() {
        return !this._hasChunk(this._chunkIndex(this.cursor));
    }

    // Get entries for a centered time window around cursor.
    // Returns { entries, startIdx, count, centerTime } for rendering.
    getWindowEntries(windowMs) {
        if (this.count === 0) return { entries: [], startIdx: 0, count: 0, centerTime: 0 };
        const halfFrames = Math.floor((windowMs / 1000) * this.tickRate / 2);
        const from = Math.max(this.startFrame, this.cursor - halfFrames);
        const to = Math.min(this.startFrame + this.count - 1, this.cursor + halfFrames);
        // Cursor is outside cached range — no renderable data
        if (from > to) return { entries: this.entries, startIdx: 0, count: 0, centerTime: this.simTimeMs(this.cursor) };
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

