/* ==================== ReplayPlayer ==================== */
class ReplayPlayer {
    constructor(replayBuf) {
        this.active = false;
        this.info = null;
        this.seeking = false;
        this.currentSpeed = 1;
        this.buf = replayBuf;

        this.badge = document.getElementById('mode-badge');
        this.bar = document.getElementById('replay-bar');
        this.trackEl = document.getElementById('replay-track');
        this.carEl = document.getElementById('replay-car');
        this.playPauseBtn = document.getElementById('replay-play-pause');
        this.seekSlider = document.getElementById('replay-seek');
        this.timeEl = document.getElementById('replay-time');
        this.exitBtn = document.getElementById('replay-exit');

        this.playPauseBtn.addEventListener('click', () => this.togglePlayPause());
        this.seekSlider.addEventListener('input', (e) => this.onSeekInput(e.target.value));
        this.seekSlider.addEventListener('change', (e) => this.onSeekChange(e.target.value));
        this.exitBtn.addEventListener('click', () => this.exit());

        document.querySelectorAll('.speed-btn').forEach(btn => {
            btn.addEventListener('click', () => this.setSpeed(parseFloat(btn.dataset.speed)));
        });
    }

    async upload(file) {
        if (!file || !file.name.toLowerCase().endsWith('.ibt')) {
            alert('Please select a valid .ibt file');
            return;
        }

        const overlay = document.getElementById('upload-overlay');
        const status = document.getElementById('upload-status');
        overlay.classList.add('active');
        status.textContent = `Uploading ${file.name} (${(file.size / 1024 / 1024).toFixed(1)} MB)...`;

        try {
            const formData = new FormData();
            formData.append('file', file);
            const resp = await fetch('/api/replay/upload', { method: 'POST', body: formData });
            if (!resp.ok) throw new Error(await resp.text());
            const result = await resp.json();
            this.info = result.info;
            this.active = true;
            this.currentSpeed = 1;
            this.enterReplayMode();
        } catch (e) {
            console.error('Upload failed:', e);
            alert('Failed to upload .ibt file: ' + e.message);
        } finally {
            overlay.classList.remove('active');
        }
    }

    async enterReplayMode() {
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
            this.buf._maxCacheFrames = this.info.tick_rate * 120;
            this.buf.cursor = this.info.current_frame || 0;
            this.buf.playing = this.info.playing !== false;
            this.buf.playbackSpeed = this.currentSpeed;
            // Fetch initial chunks around cursor (no field filter — all widgets need full frames)
            await this.buf.ensureLoaded(null);
        }
        this.updateSpeedButtons();
        this.updateControlsFromBuf();
        if (typeof updateStatus === 'function') updateStatus();
    }

    exitReplayMode() {
        this.badge.style.display = 'none';
        this.bar.classList.remove('active');
        this.active = false;
        this.info = null;
        this.buf.reset();
        if (typeof updateStatus === 'function') updateStatus();
    }

    togglePlayPause() {
        this.buf.playing = !this.buf.playing;
        if (!this.buf.playing) this.buf._lastPlayTick = null;
        this.playPauseBtn.innerHTML = this.buf.playing ? '&#9646;&#9646;' : '&#9654;';
        // Sync play/pause to server
        const action = this.buf.playing ? 'play' : 'pause';
        fetch('/api/replay/control', {
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
        fetch('/api/replay/control', {
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
            fetch('/api/replay/control', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ action: 'pause' })
            }).catch(() => {});
        }
        this.seeking = true;
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
        // Debounce chunk fetch for rapid scrubbing
        this.buf.ensureLoadedDebounced(200, null);
        this.playPauseBtn.innerHTML = '&#9654;';
        requestRedraw();
    }

    onSeekChange(value) {
        this.seeking = false;
        const frame = parseInt(value);
        this.buf.cursor = frame;
        this.buf._dirty = true;
        // Sync server position (for exit/cleanup, non-blocking)
        fetch('/api/replay/control', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ action: 'seek', value: frame })
        }).catch(() => {});
        // Ensure cache covers this position
        this.buf.ensureLoadedDebounced(50, null);
        requestRedraw();
    }

    async exit() {
        try { await fetch('/api/replay', { method: 'DELETE' }); }
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
    }

    fmtTime(secs) {
        if (!secs || isNaN(secs)) return '0:00.0';
        const m = Math.floor(secs / 60);
        const s = Math.floor(secs % 60);
        const t = Math.floor((secs * 10) % 10);
        return `${m}:${String(s).padStart(2, '0')}.${t}`;
    }
}
