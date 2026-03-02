/* ==================== ReplayPlayer ==================== */
class ReplayPlayer {
    constructor(replayBuf) {
        this.active = false;
        this.info = null;
        this.seeking = false;
        this.currentSpeed = 1;
        this.buf = replayBuf;
        this.laps = [];
        this._currentLapIdx = -1;

        // Loop state
        this.loopStart = null;
        this.loopEnd = null;
        this.loopEnabled = false;
        this._loopRegionEl = null;

        this.badge = document.getElementById('mode-badge');
        this.bar = document.getElementById('replay-bar');
        this.trackEl = document.getElementById('replay-track');
        this.carEl = document.getElementById('replay-car');
        this.playPauseBtn = document.getElementById('replay-play-pause');
        this.seekSlider = document.getElementById('replay-seek');
        this.seekWrap = document.getElementById('replay-seek-wrap');
        this.timeEl = document.getElementById('replay-time');
        this.exitBtn = document.getElementById('replay-exit');
        this.lapGroup = document.getElementById('replay-lap-group');
        this.lapBtn = document.getElementById('replay-lap-btn');
        this.lapMenu = document.getElementById('replay-lap-menu');
        this.loopStartBtn = document.getElementById('replay-loop-start');
        this.loopEndBtn = document.getElementById('replay-loop-end');
        this.loopToggleBtn = document.getElementById('replay-loop-toggle');

        this.playPauseBtn.addEventListener('click', () => this.togglePlayPause());
        this.seekSlider.addEventListener('input', (e) => this.onSeekInput(e.target.value));
        this.seekSlider.addEventListener('change', (e) => this.onSeekChange(e.target.value));
        this.exitBtn.addEventListener('click', () => this.exit());
        this.lapBtn.addEventListener('click', (e) => { e.stopPropagation(); this.toggleLapMenu(); });
        document.addEventListener('click', () => this.closeLapMenu());
        this.lapMenu.addEventListener('click', (e) => e.stopPropagation());
        this.loopStartBtn.addEventListener('click', () => this.setLoopStart());
        this.loopEndBtn.addEventListener('click', () => this.setLoopEnd());
        this.loopToggleBtn.addEventListener('click', () => this.toggleLoop());

        document.querySelectorAll('.speed-btn').forEach(btn => {
            btn.addEventListener('click', () => this.setSpeed(parseFloat(btn.dataset.speed)));
        });
    }

    async upload(file) {
        const MAX_UPLOAD_BYTES = 1024 * 1024 * 1024; // 1 GB
        if (!file || !file.name.toLowerCase().endsWith('.ibt')) {
            alert('Please select a valid .ibt file');
            return;
        }
        if (file.size > MAX_UPLOAD_BYTES) {
            alert(`File is too large (${(file.size / 1024 / 1024).toFixed(0)} MB). Maximum upload size is 1 GB.`);
            return;
        }

        const overlay = document.getElementById('upload-overlay');
        const status = document.getElementById('upload-status');
        const progressFill = document.getElementById('upload-progress-fill');
        overlay.classList.add('active');
        progressFill.style.width = '0%';
        const totalMB = (file.size / 1024 / 1024).toFixed(1);
        status.textContent = `Uploading ${file.name} — 0 / ${totalMB} MB (0%)`;

        try {
            const result = await new Promise((resolve, reject) => {
                const xhr = new XMLHttpRequest();
                xhr.open('POST', apiBase() + '/api/replay/upload');
                xhr.upload.onprogress = (e) => {
                    if (e.lengthComputable) {
                        const pct = Math.round((e.loaded / e.total) * 100);
                        const loadedMB = (e.loaded / 1024 / 1024).toFixed(1);
                        status.textContent = `Uploading ${file.name} — ${loadedMB} / ${totalMB} MB (${pct}%)`;
                        progressFill.style.width = pct + '%';
                    }
                };
                xhr.onload = () => {
                    if (xhr.status >= 200 && xhr.status < 300) {
                        try { resolve(JSON.parse(xhr.responseText)); }
                        catch (e) { reject(new Error('Invalid server response')); }
                    } else {
                        reject(new Error(xhr.responseText || `HTTP ${xhr.status}`));
                    }
                };
                xhr.onerror = () => reject(new Error('Network error'));
                xhr.onloadend = () => {
                    if (xhr.status === 0 && !xhr.responseText) return; // handled by onerror
                };
                status.textContent = `Processing ${file.name}...`;
                const formData = new FormData();
                formData.append('file', file);
                xhr.send(formData);
                // Show "Processing..." once upload completes (before server responds)
                xhr.upload.onload = () => {
                    progressFill.style.width = '100%';
                    status.textContent = `Processing ${file.name}...`;
                };
            });
            this.info = result.info;
            this.active = true;
            this.currentSpeed = 1;
            await this.enterReplayMode();
        } catch (e) {
            console.error('Upload failed:', e);
            alert('Failed to upload .ibt file: ' + e.message);
        } finally {
            overlay.classList.remove('active');
            progressFill.style.width = '0%';
        }
    }

    async enterReplayMode() {
        // Exit history mode if active
        if (typeof exitHistoryMode === 'function') exitHistoryMode();
        this.badge.textContent = 'REPLAY';
        this.badge.className = 'mode-badge mode-replay';
        this.badge.style.display = '';
        this.bar.classList.add('active');

        if (this.info) {
            this.trackEl.textContent = this.info.track_name || 'Unknown Track';
            this.carEl.textContent = this.info.car_name || 'Unknown Car';
            this.seekSlider.max = this.info.total_frames - 1;
            this.seekSlider.value = this.info.current_frame || 0;
            // Initialize replay buffer (restore position from server state)
            this.buf.totalFrames = this.info.total_frames;
            this.buf.tickRate = this.info.tick_rate;
            this.buf._chunkSize = this.info.tick_rate * 5;
            this.buf._maxCacheFrames = this.info.tick_rate * 180;
            this.buf.cursor = this.info.current_frame || 0;
            this.buf.playing = this.info.playing !== false;
            this.buf.playbackSpeed = this.currentSpeed;
            this.buf.replayId = this.info.replay_id || null;
            // Setup lap data
            this.laps = this.info.laps || [];
            this.buildLapUI();
            // Fetch initial chunks around cursor with metric mask
            await this.buf.ensureLoaded(buildReplayMetricMask());
        }
        this.clearLoop();
        this.updateSpeedButtons();
        this.updateControlsFromBuf();
        if (typeof updateStatus === 'function') updateStatus();
    }

    buildLapUI() {
        if (this.laps.length === 0) {
            this.lapGroup.style.display = 'none';
            return;
        }
        this.lapGroup.style.display = '';

        // Find best lap (lowest lap_time_secs, excluding null)
        let bestIdx = -1, bestTime = Infinity;
        for (let i = 0; i < this.laps.length; i++) {
            const lt = this.laps[i].lap_time_secs;
            if (lt != null && lt < bestTime) { bestTime = lt; bestIdx = i; }
        }
        this._bestLapIdx = bestIdx;

        // Build dropdown menu
        this.lapMenu.innerHTML = '';
        for (let i = 0; i < this.laps.length; i++) {
            const lap = this.laps[i];
            const item = document.createElement('div');
            item.className = 'replay-lap-item' + (i === bestIdx ? ' best' : '');
            item.dataset.idx = i;
            const isBest = i === bestIdx;
            const timeStr = lap.lap_time_secs != null ? this.fmtLapTime(lap.lap_time_secs) : '--';
            item.innerHTML = `<span class="replay-lap-num">${isBest ? '\u2605 ' : ''}Lap ${lap.lap_number}</span><span class="replay-lap-time">${timeStr}</span>`;
            item.addEventListener('click', () => this.seekToLap(i));
            this.lapMenu.appendChild(item);
        }

        // Build slider tick marks
        const existingTicks = this.seekWrap.querySelectorAll('.replay-lap-tick');
        existingTicks.forEach(t => t.remove());
        const total = this.info.total_frames;
        for (let i = 1; i < this.laps.length; i++) { // skip first lap tick at 0
            const lap = this.laps[i];
            const pct = (lap.start_frame / total) * 100;
            const tick = document.createElement('div');
            tick.className = 'replay-lap-tick';
            tick.style.left = pct + '%';
            const label = document.createElement('span');
            label.className = 'replay-lap-tick-label';
            label.textContent = lap.lap_number;
            tick.appendChild(label);
            this.seekWrap.appendChild(tick);
        }
    }

    seekToLap(idx) {
        const lap = this.laps[idx];
        if (!lap) return;
        this.closeLapMenu();
        const frame = lap.start_frame;
        this.buf.cursor = frame;
        this.buf.playing = false;
        this.buf._lastPlayTick = null;
        this.buf._dirty = true;
        this.playPauseBtn.innerHTML = '&#9654;';
        // Sync to server
        fetch(apiBase() + '/api/replay/control', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ action: 'pause' })
        }).catch(() => {});
        fetch(apiBase() + '/api/replay/control', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ action: 'seek', value: frame })
        }).catch(() => {});
        this.buf.ensureLoadedDebounced(50, buildReplayMetricMask());
        this.seekSlider.value = frame;
        this.updateControlsFromBuf();
        requestRedraw();
    }

    toggleLapMenu() {
        this.lapMenu.classList.toggle('open');
        if (this.lapMenu.classList.contains('open')) this._updateLapMenuHighlight();
    }

    closeLapMenu() {
        this.lapMenu.classList.remove('open');
    }

    _getCurrentLapIdx() {
        const cursor = this.buf.cursor;
        let idx = 0;
        for (let i = this.laps.length - 1; i >= 0; i--) {
            if (cursor >= this.laps[i].start_frame) { idx = i; break; }
        }
        return idx;
    }

    _updateLapMenuHighlight() {
        const idx = this._getCurrentLapIdx();
        this.lapMenu.querySelectorAll('.replay-lap-item').forEach((item, i) => {
            item.classList.toggle('current', i === idx);
        });
        // Scroll current into view
        const cur = this.lapMenu.querySelector('.current');
        if (cur) cur.scrollIntoView({ block: 'nearest' });
    }

    // Loop marker controls
    setLoopStart() {
        if (this.loopStart === this.buf.cursor) {
            this.loopStart = null;
            if (this.loopEnabled) this.loopEnabled = false;
        } else {
            this.loopStart = this.buf.cursor;
            // Auto-swap if start >= end
            if (this.loopEnd != null && this.loopStart >= this.loopEnd) {
                [this.loopStart, this.loopEnd] = [this.loopEnd, this.loopStart];
            }
        }
        this._syncLoopToBuf();
        this.updateLoopUI();
    }

    setLoopEnd() {
        if (this.loopEnd === this.buf.cursor) {
            this.loopEnd = null;
            if (this.loopEnabled) this.loopEnabled = false;
        } else {
            this.loopEnd = this.buf.cursor;
            // Auto-swap if start >= end
            if (this.loopStart != null && this.loopStart >= this.loopEnd) {
                [this.loopStart, this.loopEnd] = [this.loopEnd, this.loopStart];
            }
        }
        this._syncLoopToBuf();
        this.updateLoopUI();
    }

    toggleLoop() {
        this.loopEnabled = !this.loopEnabled;
        this._syncLoopToBuf();
        this.updateLoopUI();
    }

    _syncLoopToBuf() {
        this.buf.loopStart = this.loopStart;
        this.buf.loopEnd = this.loopEnd;
        this.buf.loopEnabled = this.loopEnabled;
    }

    clearLoop() {
        this.loopStart = null;
        this.loopEnd = null;
        this.loopEnabled = false;
        this._syncLoopToBuf();
        this.updateLoopUI();
    }

    updateLoopUI() {
        const hasStart = this.loopStart != null;
        const hasEnd = this.loopEnd != null;
        this.loopStartBtn.classList.toggle('active', hasStart);
        this.loopEndBtn.classList.toggle('active', hasEnd);
        this.loopToggleBtn.classList.toggle('loop-on', this.loopEnabled);

        // Update or remove slider overlay
        if (hasStart && hasEnd && this.info) {
            const total = this.info.total_frames;
            const leftPct = (this.loopStart / total) * 100;
            const rightPct = (this.loopEnd / total) * 100;
            if (!this._loopRegionEl) {
                this._loopRegionEl = document.createElement('div');
                this._loopRegionEl.className = 'replay-loop-region';
                this.seekWrap.appendChild(this._loopRegionEl);
            }
            this._loopRegionEl.style.left = leftPct + '%';
            this._loopRegionEl.style.width = (rightPct - leftPct) + '%';
        } else if (this._loopRegionEl) {
            this._loopRegionEl.remove();
            this._loopRegionEl = null;
        }
    }

    exitReplayMode() {
        this.badge.style.display = 'none';
        this.bar.classList.remove('active');
        this.active = false;
        this.info = null;
        this.laps = [];
        this.lapGroup.style.display = 'none';
        this.clearLoop();
        this.buf.reset();
        // Clean up tick marks
        this.seekWrap.querySelectorAll('.replay-lap-tick').forEach(t => t.remove());
        if (typeof updateStatus === 'function') updateStatus();
    }

    togglePlayPause() {
        this.buf.playing = !this.buf.playing;
        if (!this.buf.playing) this.buf._lastPlayTick = null;
        this.playPauseBtn.innerHTML = this.buf.playing ? '&#9646;&#9646;' : '&#9654;';
        // Sync play/pause to server
        const action = this.buf.playing ? 'play' : 'pause';
        fetch(apiBase() + '/api/replay/control', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ action })
        }).catch(() => {});
        requestRedraw();
    }

    setSpeed(speed) {
        this.currentSpeed = speed;
        this.buf.playbackSpeed = speed;
        this.updateSpeedButtons();
        // Sync speed to server
        fetch(apiBase() + '/api/replay/control', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ action: 'speed', value: speed })
        }).catch(() => {});
    }

    updateSpeedButtons() {
        document.querySelectorAll('.speed-btn').forEach(btn => {
            btn.classList.toggle('active', parseFloat(btn.dataset.speed) === this.currentSpeed);
        });
    }

    onSeekInput(value) {
        if (!this.seeking) {
            // Pause server playback when scrubbing starts
            fetch(apiBase() + '/api/replay/control', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ action: 'pause' })
            }).catch(() => {});
            this._seekThrottleTime = 0;
        }
        this.seeking = true;
        this.buf.scrubbing = true;
        const frame = parseInt(value);
        this.buf.cursor = frame;
        this.buf.playing = false;
        this.buf._lastPlayTick = null;
        this.buf._dirty = true;
        // Update time display immediately
        if (this.info) {
            const currentTime = (frame / this.info.total_frames) * this.info.duration_secs;
            this.timeEl.textContent = `${this.fmtTime(currentTime)} / ${this.fmtTime(this.info.duration_secs)}`;
        }
        // Throttle: fetch at most once per 250ms during scrubbing
        const now = performance.now();
        if (now - this._seekThrottleTime >= 250) {
            this._seekThrottleTime = now;
            if (this.buf._abortController) this.buf._abortController.abort();
            this.buf.ensureLoaded(buildReplayMetricMask());
        }
        this.playPauseBtn.innerHTML = '&#9654;';
        requestRedraw();
    }

    onSeekChange(value) {
        this.seeking = false;
        this.buf.scrubbing = false;
        const frame = parseInt(value);
        this.buf.cursor = frame;
        this.buf._dirty = true;
        // Sync server position (for exit/cleanup, non-blocking)
        fetch(apiBase() + '/api/replay/control', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ action: 'seek', value: frame })
        }).catch(() => {});
        // Abort any in-flight scrub fetch, then fetch final position with prefetch
        if (this.buf._abortController) this.buf._abortController.abort();
        this.buf.ensureLoaded(buildReplayMetricMask());
        requestRedraw();
    }

    async exit() {
        try { await fetch(apiBase() + '/api/replay', { method: 'DELETE' }); }
        catch (e) { console.error('Exit replay failed:', e); }
        this.exitReplayMode();
    }

    // Called each render frame to update slider and time display from buffer state
    updateControlsFromBuf() {
        if (!this.info) return;
        this.playPauseBtn.innerHTML = this.buf.playing ? '&#9646;&#9646;' : '&#9654;';
        if (!this.seeking) this.seekSlider.value = this.buf.cursor;
        const currentTime = (this.buf.cursor / this.info.total_frames) * this.info.duration_secs;
        this.timeEl.textContent = `${this.fmtTime(currentTime)} / ${this.fmtTime(this.info.duration_secs)}`;
        // Update lap button text
        if (this.laps.length > 0) {
            const idx = this._getCurrentLapIdx();
            if (idx !== this._currentLapIdx) {
                this._currentLapIdx = idx;
                this.lapBtn.textContent = `Lap ${this.laps[idx].lap_number}`;
            }
        }
    }

    fmtTime(secs) {
        if (!secs || isNaN(secs)) return '0:00.0';
        const m = Math.floor(secs / 60);
        const s = Math.floor(secs % 60);
        const t = Math.floor((secs * 10) % 10);
        return `${m}:${String(s).padStart(2, '0')}.${t}`;
    }

    stepFrame(delta) {
        if (this.buf.playing) return; // only step when paused
        this.buf.cursor = Math.max(0, Math.min(this.buf.totalFrames - 1, this.buf.cursor + delta));
        this.buf._dirty = true;
        this.seekSlider.value = this.buf.cursor;
        // Sync server position
        fetch(apiBase() + '/api/replay/control', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ action: 'seek', value: this.buf.cursor })
        }).catch(() => {});
        this.buf.ensureLoadedDebounced(50, buildReplayMetricMask());
        this.updateControlsFromBuf();
        requestRedraw();
    }

    seekToStart() {
        this.buf.cursor = this.loopStart || 0;
        this.buf._dirty = true;
        this.seekSlider.value = this.buf.cursor;
        fetch(apiBase() + '/api/replay/control', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ action: 'seek', value: this.buf.cursor })
        }).catch(() => {});
        this.buf.ensureLoadedDebounced(50, buildReplayMetricMask());
        this.updateControlsFromBuf();
        requestRedraw();
    }

    seekToEnd() {
        this.buf.cursor = (this.loopEnd != null ? this.loopEnd : this.buf.totalFrames - 1);
        this.buf._dirty = true;
        this.seekSlider.value = this.buf.cursor;
        fetch(apiBase() + '/api/replay/control', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ action: 'seek', value: this.buf.cursor })
        }).catch(() => {});
        this.buf.ensureLoadedDebounced(50, buildReplayMetricMask());
        this.updateControlsFromBuf();
        requestRedraw();
    }

    handleKeydown(e) {
        if (!this.active) return false;
        // Don't capture when typing in inputs
        if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA' || e.target.tagName === 'SELECT') return false;

        switch (e.key) {
            case ' ':
                e.preventDefault();
                this.togglePlayPause();
                return true;
            case '[':
                this.setLoopStart();
                return true;
            case ']':
                this.setLoopEnd();
                return true;
            case 'ArrowLeft':
                e.preventDefault();
                this.stepFrame(e.shiftKey ? -10 : -1);
                return true;
            case 'ArrowRight':
                e.preventDefault();
                this.stepFrame(e.shiftKey ? 10 : 1);
                return true;
            case ',':
                this.stepFrame(-1);
                return true;
            case '.':
                this.stepFrame(1);
                return true;
            case 'Home':
                e.preventDefault();
                this.seekToStart();
                return true;
            case 'End':
                e.preventDefault();
                this.seekToEnd();
                return true;
            case 'l':
            case 'L':
                this.toggleLoop();
                return true;
            case 'j':
            case 'J':
                this.setSpeed(Math.max(0.25, this.currentSpeed / 2));
                return true;
            case 'k':
            case 'K':
                this.setSpeed(Math.min(4, this.currentSpeed * 2));
                return true;
        }
        return false;
    }

    fmtLapTime(secs) {
        if (!secs || isNaN(secs)) return '--';
        const m = Math.floor(secs / 60);
        const s = (secs % 60).toFixed(3).padStart(6, '0');
        return `${m}:${s}`;
    }
}
