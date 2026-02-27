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
                        <span class="vehicle-bar-pct" id="v-str-pct">0\u00B0</span>
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

        const steerRad = v?.steering_angle ?? 0;
        const steerDeg = steerRad * RAD2DEG;
        // Bar: normalize to ~±900° range (2.5 full turns), capped at 50% each side
        const absPct = Math.min(Math.abs(steerDeg) / 900 * 50, 50);
        this.els.strBar.style.width = absPct + '%';
        this.els.strBar.style.left = steerDeg >= 0 ? '50%' : (50 - absPct) + '%';
        this.els.strPct.textContent = Math.round(steerDeg) + '\u00B0';
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
        super('wheels', 'Wheels', { col: 1, row: 13, width: 4, height: 8 });
        this.suspRange = { min: Infinity, max: -Infinity };
        this.shockMax = 1; // track absolute max for symmetric scaling
    }

    buildContent(c) {
        const corners = ['fl', 'fr', 'rl', 'rr'];
        const labels = { fl: 'FL', fr: 'FR', rl: 'RL', rr: 'RR' };
        const positions = { fl: 'grid-column:1;grid-row:1', fr: 'grid-column:3;grid-row:1', rl: 'grid-column:1;grid-row:2', rr: 'grid-column:3;grid-row:2' };
        // Left wheels: outer-middle-inner (O nearest edge, I nearest car)
        // Right wheels: inner-middle-outer (I nearest car, O nearest edge)
        const isLeft = { fl: true, fr: false, rl: true, rr: false };

        c.innerHTML = `
            <div class="wheel-layout">
                ${corners.map(w => {
                    const left = isLeft[w];
                    // Temp segments ordered so inner is always toward the car silhouette
                    const seg1 = left ? 'to' : 'ti';
                    const seg3 = left ? 'ti' : 'to';
                    const lbl1 = left ? 'O' : 'I';
                    const lbl3 = left ? 'I' : 'O';
                    return `
                <div class="wheel-corner" style="${positions[w]}">
                    <span class="wheel-label">${labels[w]}</span>
                    <div class="wheel-bars-row">
                        <div class="wheel-bar-group">
                            <div class="wheel-bar-track"><div class="wheel-bar-fill" id="w-${w}-sbar"></div></div>
                            <span class="wheel-val" id="w-${w}-susp">--</span>
                        </div>
                        <div class="wheel-bar-group">
                            <div class="wheel-bar-track wheel-shock-track"><div class="wheel-bar-center"></div><div class="wheel-shock-fill" id="w-${w}-shbar"></div></div>
                            <span class="wheel-val" id="w-${w}-shock">--</span>
                        </div>
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
            </div>`;

        this.wEls = {};
        for (const w of corners) {
            this.wEls[w] = {
                sbar: c.querySelector(`#w-${w}-sbar`),
                susp: c.querySelector(`#w-${w}-susp`),
                shbar: c.querySelector(`#w-${w}-shbar`),
                shock: c.querySelector(`#w-${w}-shock`),
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
            if (wd?.shock_velocity != null) {
                const abs = Math.abs(wd.shock_velocity * 1000);
                if (abs > this.shockMax) this.shockMax = abs;
            }
        }

        const sRange = this.suspRange.max - this.suspRange.min;
        const sMin = this.suspRange.min - sRange * 0.1;
        const sMax = this.suspRange.max + sRange * 0.1;

        for (const [key, wd] of Object.entries(map)) {
            const els = this.wEls[key];

            // Suspension travel bar
            if (wd?.suspension_travel != null) {
                const mm = wd.suspension_travel * 1000;
                let pct = sMax > sMin ? ((mm - sMin) / (sMax - sMin)) * 100 : 50;
                pct = Math.max(0, Math.min(100, pct));
                els.sbar.style.height = pct + '%';
                els.susp.textContent = mm.toFixed(1);
            }

            // Shock velocity centered bar
            if (wd?.shock_velocity != null) {
                const mmps = wd.shock_velocity * 1000;
                const halfPct = Math.min(Math.abs(mmps) / this.shockMax, 1) * 50;
                if (mmps >= 0) {
                    els.shbar.style.bottom = '50%';
                    els.shbar.style.top = '';
                    els.shbar.style.height = halfPct + '%';
                    els.shbar.style.background = '#fb923c';
                } else {
                    els.shbar.style.top = '50%';
                    els.shbar.style.bottom = '';
                    els.shbar.style.height = halfPct + '%';
                    els.shbar.style.background = '#38bdf8';
                }
                els.shock.textContent = mmps.toFixed(0);
            }

            // Tire surface temps — show value per segment
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
