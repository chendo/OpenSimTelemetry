/* ==================== VehicleWidget ==================== */
class VehicleWidget extends Widget {
    constructor() { super('vehicle', 'Vehicle', { col: 1, row: 1, width: 4, height: 8 }); }

    buildContent(c) {
        c.innerHTML = `
            <div class="vehicle-layout">
                <div class="vehicle-stats-row">
                    <div class="metric-cell">
                        <div class="metric-label">SPEED</div>
                        <div class="vehicle-stat-val"><span id="v-speed">---</span><span class="metric-unit" id="v-speed-unit">m/s</span></div>
                    </div>
                    <div class="metric-cell">
                        <div class="metric-label">GEAR</div>
                        <div class="vehicle-stat-val" id="v-gear">N</div>
                    </div>
                    <div class="metric-cell">
                        <div class="metric-label">RPM</div>
                        <div class="vehicle-stat-val" id="v-rpm">---</div>
                    </div>
                </div>
                <div class="vehicle-controls">
                    <div class="vehicle-wheel-wrap"><canvas id="v-wheel-canvas"></canvas></div>
                    <div class="vehicle-pedals">
                        <div class="vehicle-pedal-group">
                            <span class="vehicle-pedal-label">T</span>
                            <div class="vehicle-pedal-track"><div class="vehicle-pedal-fill pedal-throttle" id="v-thr-bar"></div></div>
                            <span class="vehicle-pedal-pct" id="v-thr-pct">0</span>
                        </div>
                        <div class="vehicle-pedal-group">
                            <span class="vehicle-pedal-label">B</span>
                            <div class="vehicle-pedal-track"><div class="vehicle-pedal-fill pedal-brake" id="v-brk-bar"></div></div>
                            <span class="vehicle-pedal-pct" id="v-brk-pct">0</span>
                        </div>
                        <div class="vehicle-pedal-group">
                            <span class="vehicle-pedal-label">C</span>
                            <div class="vehicle-pedal-track"><div class="vehicle-pedal-fill pedal-clutch" id="v-clt-bar"></div></div>
                            <span class="vehicle-pedal-pct" id="v-clt-pct">0</span>
                        </div>
                    </div>
                </div>
            </div>`;
        this.canvas = c.querySelector('#v-wheel-canvas');
        this.ctx = this.canvas.getContext('2d');
        this._steerAngle = 0;
        this._cache(c, { speed: '#v-speed', speedUnit: '#v-speed-unit', rpm: '#v-rpm', gear: '#v-gear',
            thrBar: '#v-thr-bar', thrPct: '#v-thr-pct',
            brkBar: '#v-brk-bar', brkPct: '#v-brk-pct',
            cltBar: '#v-clt-bar', cltPct: '#v-clt-pct' });
        requestAnimationFrame(() => this.renderWheel());
    }

    _cache(c, map) { this.els = {}; for (const [k, sel] of Object.entries(map)) this.els[k] = c.querySelector(sel); }

    update(store) {
        const f = store.currentFrame;
        const v = f?.vehicle;
        if (v?.speed != null) {
            const sp = applyUnitPref('m/s', v.speed);
            this.els.speed.textContent = Math.round(sp.value);
            this.els.speedUnit.textContent = sp.unit;
        } else {
            this.els.speed.textContent = '---';
        }
        this.els.rpm.textContent = v?.rpm != null ? Math.round(v.rpm) : '---';
        this.els.gear.textContent = v?.gear != null ? (v.gear === -1 ? 'R' : v.gear === 0 ? 'N' : v.gear) : 'N';

        const thr = (v?.throttle ?? 0) * 100;
        this.els.thrBar.style.height = thr + '%';
        this.els.thrPct.textContent = Math.round(thr);

        const brk = (v?.brake ?? 0) * 100;
        this.els.brkBar.style.height = brk + '%';
        this.els.brkPct.textContent = Math.round(brk);

        // Clutch: 0=engaged, 1=disengaged; invert so pressing clutch fills bar
        const clt = (1 - (v?.clutch ?? 0)) * 100;
        this.els.cltBar.style.height = clt + '%';
        this.els.cltPct.textContent = Math.round(clt);

        this._steerAngle = -(v?.steering_angle ?? 0) * Math.PI / 180;  // negate: iRacing positive=left, canvas positive=clockwise
        this.renderWheel();
    }

    renderWheel() {
        const canvas = this.canvas;
        const dpr = window.devicePixelRatio || 1;
        const wrap = canvas.parentElement;
        const size = Math.min(wrap.clientWidth, wrap.clientHeight);
        if (size <= 0) return;
        const targetW = size * dpr, targetH = size * dpr;
        if (canvas.width !== targetW || canvas.height !== targetH) {
            canvas.width = targetW;
            canvas.height = targetH;
            canvas.style.width = size + 'px';
            canvas.style.height = size + 'px';
        }
        const ctx = this.ctx;
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        ctx.clearRect(0, 0, size, size);

        const cx = size / 2, cy = size / 2;
        const r = size * 0.40; // wheel radius

        // Fixed reference mark at top (doesn't rotate)
        ctx.beginPath();
        ctx.moveTo(cx, cy - r - 6);
        ctx.lineTo(cx - 4, cy - r - 12);
        ctx.lineTo(cx + 4, cy - r - 12);
        ctx.closePath();
        ctx.fillStyle = 'rgba(255,255,255,0.4)';
        ctx.fill();

        // Rotate for steering
        ctx.save();
        ctx.translate(cx, cy);
        ctx.rotate(this._steerAngle);

        // --- Draw flat-bottom F1 wheel ---
        const wheelColor = 'rgba(255,255,255,0.5)';
        const spokeColor = 'rgba(255,255,255,0.25)';
        const lw = Math.max(2, size * 0.025);

        // Flat bottom angle: bottom 60° is flat (from 240° to 300°, i.e. ±30° from bottom)
        const flatHalfAngle = Math.PI / 6; // 30°
        const arcStart = Math.PI / 2 + flatHalfAngle;  // 120° (from right, going CW)
        const arcEnd = Math.PI / 2 - flatHalfAngle + Math.PI * 2; // 60° + 360°

        // Outer rim arc (top portion)
        ctx.beginPath();
        ctx.arc(0, 0, r, arcStart, arcEnd);
        ctx.strokeStyle = wheelColor;
        ctx.lineWidth = lw;
        ctx.lineCap = 'round';
        ctx.stroke();

        // Flat bottom line
        const flatLeftX = Math.cos(arcStart) * r;
        const flatLeftY = Math.sin(arcStart) * r;
        const flatRightX = Math.cos(Math.PI / 2 - flatHalfAngle) * r;
        const flatRightY = Math.sin(Math.PI / 2 - flatHalfAngle) * r;
        ctx.beginPath();
        ctx.moveTo(flatLeftX, flatLeftY);
        ctx.lineTo(flatRightX, flatRightY);
        ctx.stroke();

        // Hub (center circle)
        const hubR = r * 0.15;
        ctx.beginPath();
        ctx.arc(0, 0, hubR, 0, Math.PI * 2);
        ctx.strokeStyle = spokeColor;
        ctx.lineWidth = lw * 0.7;
        ctx.stroke();

        // Center dot
        ctx.beginPath();
        ctx.arc(0, 0, 3, 0, Math.PI * 2);
        ctx.fillStyle = '#00d68f';
        ctx.fill();

        // Spokes: left, right, and top
        ctx.beginPath();
        ctx.strokeStyle = spokeColor;
        ctx.lineWidth = lw * 0.7;
        // Left spoke
        ctx.moveTo(-hubR, 0);
        ctx.lineTo(-r, 0);
        // Right spoke
        ctx.moveTo(hubR, 0);
        ctx.lineTo(r, 0);
        // Top spoke
        ctx.moveTo(0, -hubR);
        ctx.lineTo(0, -r);
        ctx.stroke();

        ctx.restore();
    }
}

/* ==================== GForceWidget ==================== */
class GForceWidget extends Widget {
    constructor() {
        super('gforce', 'G-Force', { col: 5, row: 1, width: 4, height: 8 });
        this.trailMax = 40;
        this._trail = new Array(this.trailMax);
        this._trailHead = 0;
        this._trailCount = 0;
        this.gScale = 2.0;
        this._history = [];
        this._maxLat = 0;
        this._maxLong = 0;
        this._maxLatPt = null;
        this._maxLongPt = null;
    }

    buildContent(c) {
        c.innerHTML = `
            <div class="gforce-layout">
                <div class="gforce-canvas-wrap"><canvas id="gf-canvas"></canvas></div>
                <div class="gforce-readouts">
                    <div class="gforce-cell"><div class="metric-label">LAT G</div><div class="gforce-val"><span id="gf-lat-num">0.00</span><span class="gforce-val-unit">G</span><span class="gforce-max" id="gf-lat-max">--</span></div></div>
                    <div class="gforce-cell"><div class="metric-label">LONG G</div><div class="gforce-val"><span id="gf-long-num">0.00</span><span class="gforce-val-unit">G</span><span class="gforce-max" id="gf-long-max">--</span></div></div>
                    <div class="gforce-cell"><div class="metric-label">VERT G</div><div class="gforce-val"><span id="gf-vert-num">0.00</span><span class="gforce-val-unit">G</span></div></div>
                    <div class="gforce-cell"><div class="metric-label">YAW RATE</div><div class="gforce-val"><span id="gf-yaw-num">0.0</span><span class="gforce-val-unit">&deg;/s</span></div></div>
                </div>
            </div>`;
        this.canvas = c.querySelector('#gf-canvas');
        this.ctx = this.canvas.getContext('2d');
        this._cache(c, { lat: '#gf-lat-num', long: '#gf-long-num', vert: '#gf-vert-num', yaw: '#gf-yaw-num',
            latMax: '#gf-lat-max', longMax: '#gf-long-max' });
    }

    _cache(c, map) { this.valEls = {}; for (const [k, sel] of Object.entries(map)) this.valEls[k] = c.querySelector(sel); }

    update(store, now) {
        const f = store.currentFrame; if (!f) return;
        const gf = f.motion?.g_force;
        const gx = gf?.x ?? 0, gy = gf?.y ?? 0, gz = gf?.z ?? 0;
        // Ring buffer trail
        this._trail[this._trailHead] = { x: gx, y: gz };
        this._trailHead = (this._trailHead + 1) % this.trailMax;
        if (this._trailCount < this.trailMax) this._trailCount++;

        // 60s max tracking
        this._history.push({ t: now, lat: gx, long: gz });
        const cutoff = now - 60000;
        while (this._history.length > 0 && this._history[0].t < cutoff) this._history.shift();
        let maxLat = 0, maxLong = 0, maxLatPt = null, maxLongPt = null;
        for (const h of this._history) {
            const aLat = Math.abs(h.lat);
            if (aLat > maxLat) { maxLat = aLat; maxLatPt = { x: h.lat, y: h.long }; }
            const aLong = Math.abs(h.long);
            if (aLong > maxLong) { maxLong = aLong; maxLongPt = { x: h.lat, y: h.long }; }
        }
        this._maxLat = maxLat;
        this._maxLong = maxLong;
        this._maxLatPt = maxLatPt;
        this._maxLongPt = maxLongPt;

        this.valEls.lat.textContent = gx.toFixed(2);
        this.valEls.long.textContent = gz.toFixed(2);
        this.valEls.vert.textContent = gy.toFixed(2);
        this.valEls.latMax.textContent = maxLat > 0 ? 'max ' + maxLat.toFixed(2) : '--';
        this.valEls.longMax.textContent = maxLong > 0 ? 'max ' + maxLong.toFixed(2) : '--';
        const yawRate = f.motion?.yaw_rate ?? 0;
        this.valEls.yaw.textContent = yawRate.toFixed(1);

        this.renderCanvas();
    }

    renderCanvas() {
        const canvas = this.canvas;
        const dpr = window.devicePixelRatio || 1;
        const wrap = canvas.parentElement;
        const size = Math.min(wrap.clientWidth, wrap.clientHeight);
        if (size <= 0) return;
        const targetW = size * dpr, targetH = size * dpr;
        if (canvas.width !== targetW || canvas.height !== targetH) {
            canvas.width = targetW;
            canvas.height = targetH;
            canvas.style.width = size + 'px';
            canvas.style.height = size + 'px';
        }
        const ctx = this.ctx;
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

        const cx = size / 2, cy = size / 2;
        const radius = size * 0.42;

        ctx.clearRect(0, 0, size, size);

        // Outer circle (dashed, 2G)
        ctx.beginPath(); ctx.arc(cx, cy, radius, 0, Math.PI * 2);
        ctx.setLineDash([4, 4]); ctx.strokeStyle = 'rgba(255,255,255,0.12)'; ctx.lineWidth = 1; ctx.stroke(); ctx.setLineDash([]);

        // Inner circle (1G)
        ctx.beginPath(); ctx.arc(cx, cy, radius / this.gScale, 0, Math.PI * 2);
        ctx.strokeStyle = 'rgba(255,255,255,0.06)'; ctx.stroke();

        // Crosshairs
        ctx.beginPath();
        ctx.moveTo(cx - radius, cy); ctx.lineTo(cx + radius, cy);
        ctx.moveTo(cx, cy - radius); ctx.lineTo(cx, cy + radius);
        ctx.strokeStyle = 'rgba(255,255,255,0.08)'; ctx.stroke();

        const scale = radius / this.gScale;
        const toP = (lat, lon) => ({ px: cx + lat * scale, py: cy - lon * scale });

        // Trail (ring buffer iteration, oldest to newest, skip last which is current dot)
        const trailStart = this._trailCount < this.trailMax ? 0 : this._trailHead;
        for (let i = 0; i < this._trailCount - 1; i++) {
            const alpha = (i / this._trailCount) * 0.5;
            const entry = this._trail[(trailStart + i) % this.trailMax];
            const p = toP(entry.x, entry.y);
            ctx.beginPath(); ctx.arc(p.px, p.py, 2, 0, Math.PI * 2);
            ctx.fillStyle = `rgba(0,214,143,${alpha})`; ctx.fill();
        }

        // Current dot
        if (this._trailCount > 0) {
            const last = this._trail[(this._trailHead - 1 + this.trailMax) % this.trailMax];
            const p = toP(last.x, last.y);
            ctx.beginPath(); ctx.arc(p.px, p.py, 6, 0, Math.PI * 2);
            ctx.fillStyle = 'rgba(0,214,143,0.3)'; ctx.fill();
            ctx.beginPath(); ctx.arc(p.px, p.py, 4, 0, Math.PI * 2);
            ctx.fillStyle = '#00d68f'; ctx.fill();
        }

        // Max G markers (orange hollow circles)
        const drawMaxMarker = (pt) => {
            if (!pt) return;
            const p = toP(pt.x, pt.y);
            ctx.beginPath(); ctx.arc(p.px, p.py, 4, 0, Math.PI * 2);
            ctx.strokeStyle = '#ffa502'; ctx.lineWidth = 1.5; ctx.stroke();
        };
        if (this._maxLatPt && this._maxLongPt &&
            this._maxLatPt.x === this._maxLongPt.x && this._maxLatPt.y === this._maxLongPt.y) {
            drawMaxMarker(this._maxLatPt);
        } else {
            drawMaxMarker(this._maxLatPt);
            drawMaxMarker(this._maxLongPt);
        }
    }
}

/* ==================== WheelsWidget ==================== */
function _lerpColor(a, b, t) {
    const pa = [parseInt(a.slice(1,3),16), parseInt(a.slice(3,5),16), parseInt(a.slice(5,7),16)];
    const pb = [parseInt(b.slice(1,3),16), parseInt(b.slice(3,5),16), parseInt(b.slice(5,7),16)];
    return `rgb(${Math.round(pa[0]+(pb[0]-pa[0])*t)},${Math.round(pa[1]+(pb[1]-pa[1])*t)},${Math.round(pa[2]+(pb[2]-pa[2])*t)})`;
}

function tireTemperatureColor(tempC) {
    if (tempC <= 60) return '#3b82f6';
    if (tempC <= 75) return _lerpColor('#3b82f6', '#22c55e', (tempC - 60) / 15);
    if (tempC <= 105) return '#22c55e';
    if (tempC <= 120) return _lerpColor('#22c55e', '#eab308', (tempC - 105) / 15);
    if (tempC <= 135) return _lerpColor('#eab308', '#ef4444', (tempC - 120) / 15);
    return '#ef4444';
}

class WheelsWidget extends Widget {
    constructor() {
        super('wheels', 'Wheels', { col: 9, row: 1, width: 4, height: 8 });
        this.suspRange = { min: Infinity, max: -Infinity };
        // Suspension travel ring buffers per corner (~2s at 60Hz)
        this._suspHistLen = 120;
        this._suspHist = {};
        for (const w of ['fl', 'fr', 'rl', 'rr']) {
            this._suspHist[w] = { buf: new Float32Array(120), head: 0, count: 0 };
        }
    }

    buildContent(c) {
        const corners = ['fl', 'fr', 'rl', 'rr'];
        const labels = { fl: 'FL', fr: 'FR', rl: 'RL', rr: 'RR' };
        const positions = { fl: 'grid-column:1;grid-row:1', fr: 'grid-column:3;grid-row:1', rl: 'grid-column:1;grid-row:2', rr: 'grid-column:3;grid-row:2' };
        const isLeft = { fl: true, fr: false, rl: true, rr: false };

        c.innerHTML = `
            <div class="wheel-layout">
                ${corners.map(w => {
                    const left = isLeft[w];
                    const seg1 = left ? 'to' : 'ti';
                    const seg3 = left ? 'ti' : 'to';
                    const lbl1 = left ? 'O' : 'I';
                    const lbl3 = left ? 'I' : 'O';
                    return `
                <div class="wheel-corner" style="${positions[w]}">
                    <span class="wheel-label">${labels[w]}</span>
                    <div class="wheel-susp-row">
                        <div class="wheel-susp-track"><div class="wheel-susp-fill" id="w-${w}-suspbar"></div></div>
                        <canvas class="wheel-susp-spark" id="w-${w}-spark"></canvas>
                        <span class="wheel-val" id="w-${w}-susp">--</span>
                    </div>
                    <div class="wheel-temp-row">
                        <div class="wheel-temp-cell">
                            <div class="wheel-temp-seg" id="w-${w}-${seg1}"></div>
                            <span class="wheel-temp-val" id="w-${w}-${seg1}v">--</span>
                            <span class="wheel-temp-lbl">${lbl1}</span>
                        </div>
                        <div class="wheel-temp-cell">
                            <div class="wheel-temp-seg" id="w-${w}-tm"></div>
                            <span class="wheel-temp-val" id="w-${w}-tmv">--</span>
                            <span class="wheel-temp-lbl">M</span>
                        </div>
                        <div class="wheel-temp-cell">
                            <div class="wheel-temp-seg" id="w-${w}-${seg3}"></div>
                            <span class="wheel-temp-val" id="w-${w}-${seg3}v">--</span>
                            <span class="wheel-temp-lbl">${lbl3}</span>
                        </div>
                    </div>
                    <div class="wheel-wear-row">
                        <span class="wheel-wear-label">WEAR</span>
                        <div class="wheel-wear-track"><div class="wheel-wear-fill" id="w-${w}-wear"></div></div>
                        <span class="wheel-val" id="w-${w}-wearv">--</span>
                    </div>
                </div>`;
                }).join('')}
                <div class="wheel-car-shape"></div>
            </div>
            <div class="wheel-warning">Only some sims provide live tire data</div>`;

        this.wEls = {};
        for (const w of corners) {
            this.wEls[w] = {
                suspbar: c.querySelector(`#w-${w}-suspbar`),
                spark: c.querySelector(`#w-${w}-spark`),
                susp: c.querySelector(`#w-${w}-susp`),
                ti: c.querySelector(`#w-${w}-ti`),
                tm: c.querySelector(`#w-${w}-tm`),
                to: c.querySelector(`#w-${w}-to`),
                tiv: c.querySelector(`#w-${w}-tiv`),
                tmv: c.querySelector(`#w-${w}-tmv`),
                tov: c.querySelector(`#w-${w}-tov`),
                wear: c.querySelector(`#w-${w}-wear`),
                wearv: c.querySelector(`#w-${w}-wearv`),
            };
        }
    }

    // Map suspension compression (normalized 0-1) to color: green → yellow → red
    _loadColor(t) {
        t = Math.max(0, Math.min(1, t));
        if (t <= 0.5) return _lerpColor('#22c55e', '#eab308', t / 0.5);
        return _lerpColor('#eab308', '#ef4444', (t - 0.5) / 0.5);
    }

    _renderSuspSparkline(canvas, hist) {
        const dpr = window.devicePixelRatio || 1;
        const w = canvas.clientWidth;
        const h = canvas.clientHeight;
        if (w <= 0 || h <= 0) return;
        const tw = w * dpr, th = h * dpr;
        if (canvas.width !== tw || canvas.height !== th) {
            canvas.width = tw;
            canvas.height = th;
        }
        const ctx = canvas.getContext('2d');
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        ctx.clearRect(0, 0, w, h);

        if (hist.count < 2) return;

        const n = hist.count;
        const len = hist.buf.length;
        const start = hist.count < len ? 0 : hist.head;
        const sRange = this.suspRange.max - this.suspRange.min;
        const sMin = this.suspRange.min;
        const pad = 1;

        // Draw filled sparkline — normalized to dynamic range
        ctx.beginPath();
        for (let i = 0; i < n; i++) {
            const idx = (start + i) % len;
            const v = hist.buf[idx];
            const t = sRange > 0.1 ? (v - sMin) / sRange : 0.5;
            const x = (n < this._suspHistLen) ? i * (w / (n - 1)) : i * (w / (this._suspHistLen - 1));
            const y = pad + (1 - t) * (h - 2 * pad); // inverted: more compression at top
            if (i === 0) ctx.moveTo(x, y);
            else ctx.lineTo(x, y);
        }
        ctx.strokeStyle = 'rgba(0,214,143,0.6)';
        ctx.lineWidth = 1;
        ctx.stroke();

        // Fill below the line
        const lastX = (n < this._suspHistLen) ? (n - 1) * (w / (n - 1)) : (n - 1) * (w / (this._suspHistLen - 1));
        ctx.lineTo(lastX, h);
        ctx.lineTo(0, h);
        ctx.closePath();
        ctx.fillStyle = 'rgba(0,214,143,0.1)';
        ctx.fill();
    }

    update(store) {
        const f = store.currentFrame; if (!f?.wheels) return;
        const map = { fl: f.wheels.front_left, fr: f.wheels.front_right, rl: f.wheels.rear_left, rr: f.wheels.rear_right };

        // Update dynamic ranges
        for (const wd of Object.values(map)) {
            if (wd?.suspension_travel != null) {
                const mm = wd.suspension_travel * 1000;
                if (mm < this.suspRange.min) this.suspRange.min = mm;
                if (mm > this.suspRange.max) this.suspRange.max = mm;
            }
        }

        const sRange = this.suspRange.max - this.suspRange.min;
        const sMin = this.suspRange.min;

        for (const [key, wd] of Object.entries(map)) {
            const els = this.wEls[key];

            // Suspension travel — vertical bar + sparkline
            if (wd?.suspension_travel != null) {
                const mm = wd.suspension_travel * 1000;
                const t = sRange > 0.1 ? (mm - sMin) / sRange : 0.5;
                const pct = Math.max(0, Math.min(1, t)) * 100;
                els.suspbar.style.height = pct + '%';
                els.suspbar.style.background = this._loadColor(t);
                els.susp.textContent = mm.toFixed(1);

                // Push to ring buffer and render sparkline
                const hist = this._suspHist[key];
                hist.buf[hist.head] = mm;
                hist.head = (hist.head + 1) % hist.buf.length;
                if (hist.count < hist.buf.length) hist.count++;
                this._renderSuspSparkline(els.spark, hist);
            }

            // Tire surface temps
            const ti = wd?.surface_temp_inner, tm = wd?.surface_temp_middle, to = wd?.surface_temp_outer;
            if (ti != null) { els.ti.style.background = tireTemperatureColor(ti); els.tiv.textContent = Math.round(ti) + '\u00B0'; }
            if (tm != null) { els.tm.style.background = tireTemperatureColor(tm); els.tmv.textContent = Math.round(tm) + '\u00B0'; }
            if (to != null) { els.to.style.background = tireTemperatureColor(to); els.tov.textContent = Math.round(to) + '\u00B0'; }

            // Tire wear (0 = new, 1 = worn)
            if (wd?.tyre_wear != null) {
                const pct = Math.max(0, Math.min(1, wd.tyre_wear)) * 100;
                const remaining = 100 - pct;
                els.wear.style.width = remaining + '%';
                els.wear.style.background = pct < 30 ? '#22c55e' : pct < 60 ? '#eab308' : '#ef4444';
                els.wearv.textContent = remaining.toFixed(0) + '%';
            }
        }
    }
}
