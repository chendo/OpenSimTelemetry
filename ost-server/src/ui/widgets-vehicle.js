/* ==================== VehicleWidget ==================== */
class VehicleWidget extends Widget {
    constructor() { super('vehicle', 'Vehicle', { col: 1, row: 1, width: 4, height: 6 }); }

    buildContent(c) {
        c.innerHTML = `
            <div class="vehicle-grid">
                <div class="metric-cell">
                    <div class="metric-label">SPEED</div>
                    <div class="metric-value"><span id="v-speed">---</span><span class="metric-unit">km/h</span></div>
                </div>
                <div class="metric-cell">
                    <div class="metric-label">RPM</div>
                    <div class="metric-value" id="v-rpm">---</div>
                </div>
                <div class="metric-cell">
                    <div class="metric-label">GEAR</div>
                    <div class="metric-value" id="v-gear">N</div>
                </div>
                <div class="vehicle-bar-group">
                    <div class="vehicle-bar-item">
                        <span class="vehicle-bar-label">THR</span>
                        <div class="vehicle-bar-track"><div class="vehicle-bar-fill bar-throttle" id="v-thr-bar"></div></div>
                        <span class="vehicle-bar-pct" id="v-thr-pct">0%</span>
                    </div>
                    <div class="vehicle-bar-item">
                        <span class="vehicle-bar-label">BRK</span>
                        <div class="vehicle-bar-track"><div class="vehicle-bar-fill bar-brake" id="v-brk-bar"></div></div>
                        <span class="vehicle-bar-pct" id="v-brk-pct">0%</span>
                    </div>
                    <div class="vehicle-bar-item">
                        <span class="vehicle-bar-label">STR</span>
                        <div class="vehicle-bar-track"><div class="vehicle-bar-fill bar-steer" id="v-str-bar"></div></div>
                        <span class="vehicle-bar-pct" id="v-str-pct">0%</span>
                    </div>
                </div>
            </div>`;
        this._cache(c, { speed: '#v-speed', rpm: '#v-rpm', gear: '#v-gear',
            thrBar: '#v-thr-bar', thrPct: '#v-thr-pct', brkBar: '#v-brk-bar', brkPct: '#v-brk-pct',
            strBar: '#v-str-bar', strPct: '#v-str-pct' });
    }

    _cache(c, map) { this.els = {}; for (const [k, sel] of Object.entries(map)) this.els[k] = c.querySelector(sel); }

    update(store) {
        const f = store.currentFrame; if (!f) return;
        const v = f.vehicle;
        this.els.speed.textContent = v?.speed != null ? Math.round(v.speed * 3.6) : '---';
        this.els.rpm.textContent = v?.rpm != null ? Math.round(v.rpm) : '---';
        this.els.gear.textContent = v?.gear != null ? (v.gear === -1 ? 'R' : v.gear === 0 ? 'N' : v.gear) : 'N';

        const thr = (v?.throttle ?? 0) * 100;
        this.els.thrBar.style.width = thr + '%';
        this.els.thrPct.textContent = Math.round(thr) + '%';

        const brk = (v?.brake ?? 0) * 100;
        this.els.brkBar.style.width = brk + '%';
        this.els.brkPct.textContent = Math.round(brk) + '%';

        const steer = v?.steering_angle ?? 0;
        const absPct = Math.abs(steer) * 50;
        this.els.strBar.style.width = absPct + '%';
        this.els.strBar.style.left = steer >= 0 ? '50%' : (50 - absPct) + '%';
        this.els.strPct.textContent = Math.round(steer * 100) + '%';
    }
}

/* ==================== GForceWidget ==================== */
class GForceWidget extends Widget {
    constructor() {
        super('gforce', 'G-Force', { col: 5, row: 1, width: 4, height: 6 });
        this.trailMax = 40;
        this._trail = new Array(this.trailMax);
        this._trailHead = 0;
        this._trailCount = 0;
        this.gScale = 2.0;
    }

    buildContent(c) {
        c.innerHTML = `
            <div class="gforce-layout">
                <div class="gforce-canvas-wrap"><canvas id="gf-canvas"></canvas></div>
                <div class="gforce-readouts">
                    <div class="gforce-cell"><div class="metric-label">LAT G</div><div class="gforce-val"><span id="gf-lat-num">0.00</span><span class="gforce-val-unit">G</span></div></div>
                    <div class="gforce-cell"><div class="metric-label">LONG G</div><div class="gforce-val"><span id="gf-long-num">0.00</span><span class="gforce-val-unit">G</span></div></div>
                    <div class="gforce-cell"><div class="metric-label">VERT G</div><div class="gforce-val"><span id="gf-vert-num">0.00</span><span class="gforce-val-unit">G</span></div></div>
                    <div class="gforce-cell"><div class="metric-label">YAW RATE</div><div class="gforce-val"><span id="gf-yaw-num">0.0</span><span class="gforce-val-unit">&deg;/s</span></div></div>
                </div>
            </div>`;
        this.canvas = c.querySelector('#gf-canvas');
        this.ctx = this.canvas.getContext('2d');
        this._cache(c, { lat: '#gf-lat-num', long: '#gf-long-num', vert: '#gf-vert-num', yaw: '#gf-yaw-num' });
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

        this.valEls.lat.textContent = gx.toFixed(2);
        this.valEls.long.textContent = gz.toFixed(2);
        this.valEls.vert.textContent = gy.toFixed(2);
        const yawRate = (f.motion?.angular_velocity?.y ?? 0) * RAD2DEG;
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
        super('wheels', 'Wheels', { col: 1, row: 13, width: 4, height: 7 });
        this.ranges = { min: Infinity, max: -Infinity };
    }

    buildContent(c) {
        const corners = ['fl', 'fr', 'rl', 'rr'];
        const labels = { fl: 'FL', fr: 'FR', rl: 'RL', rr: 'RR' };
        const positions = { fl: 'grid-column:1;grid-row:1', fr: 'grid-column:3;grid-row:1', rl: 'grid-column:1;grid-row:2', rr: 'grid-column:3;grid-row:2' };

        c.innerHTML = `
            <div class="wheel-layout">
                ${corners.map(w => `
                <div class="wheel-corner" style="${positions[w]}">
                    <span class="wheel-label">${labels[w]}</span>
                    <div class="wheel-susp-row">
                        <div class="wheel-bar-track"><div class="wheel-bar-fill" id="w-${w}-bar"></div></div>
                        <span class="wheel-val" id="w-${w}-susp">--</span>
                    </div>
                    <div class="wheel-temp-row">
                        <div class="wheel-temp-strip">
                            <div class="wheel-temp-seg" id="w-${w}-ti"></div>
                            <div class="wheel-temp-seg" id="w-${w}-tm"></div>
                            <div class="wheel-temp-seg" id="w-${w}-to"></div>
                        </div>
                        <span class="wheel-val" id="w-${w}-temp">--</span>
                    </div>
                    <div class="wheel-shock-row">
                        <span class="wheel-shock-label">SHK</span>
                        <span class="wheel-val" id="w-${w}-shock">--</span>
                    </div>
                </div>`).join('')}
                <div class="wheel-car-shape"></div>
            </div>`;

        this.wEls = {};
        for (const w of corners) {
            this.wEls[w] = {
                bar: c.querySelector(`#w-${w}-bar`),
                susp: c.querySelector(`#w-${w}-susp`),
                ti: c.querySelector(`#w-${w}-ti`),
                tm: c.querySelector(`#w-${w}-tm`),
                to: c.querySelector(`#w-${w}-to`),
                temp: c.querySelector(`#w-${w}-temp`),
                shock: c.querySelector(`#w-${w}-shock`),
            };
        }
    }

    update(store) {
        const f = store.currentFrame; if (!f?.wheels) return;
        const map = { fl: f.wheels.front_left, fr: f.wheels.front_right, rl: f.wheels.rear_left, rr: f.wheels.rear_right };

        for (const wd of Object.values(map)) {
            if (wd?.suspension_travel != null) {
                const mm = wd.suspension_travel * 1000;
                if (mm < this.ranges.min) this.ranges.min = mm;
                if (mm > this.ranges.max) this.ranges.max = mm;
            }
        }

        const range = this.ranges.max - this.ranges.min;
        const pMin = this.ranges.min - range * 0.1;
        const pMax = this.ranges.max + range * 0.1;

        for (const [key, wd] of Object.entries(map)) {
            const els = this.wEls[key];

            // Suspension travel
            if (wd?.suspension_travel != null) {
                const mm = wd.suspension_travel * 1000;
                let pct = pMax > pMin ? ((mm - pMin) / (pMax - pMin)) * 100 : 50;
                pct = Math.max(0, Math.min(100, pct));
                els.bar.style.height = pct + '%';
                els.susp.textContent = mm.toFixed(1) + ' mm';
            }

            // Tire surface temps
            const ti = wd?.surface_temp_inner, tm = wd?.surface_temp_middle, to = wd?.surface_temp_outer;
            if (ti != null && tm != null && to != null) {
                els.ti.style.background = tireTemperatureColor(ti);
                els.tm.style.background = tireTemperatureColor(tm);
                els.to.style.background = tireTemperatureColor(to);
                els.temp.textContent = Math.round(tm) + '\u00B0C';
            }

            // Shock velocity
            if (wd?.shock_velocity != null) {
                const mmps = wd.shock_velocity * 1000;
                els.shock.textContent = mmps.toFixed(0) + ' mm/s';
            }
        }
    }
}
