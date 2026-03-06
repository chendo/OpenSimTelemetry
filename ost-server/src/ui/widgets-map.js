/* ==================== Track Map Widget ==================== */
class TrackMapWidget extends Widget {
    constructor() {
        super('trackmap', 'Track Map', { col: 4, row: 1, width: 3, height: 8 });
        this._outline = null;    // [[lat, lng], ...] from server
        this._bounds = null;     // {minLat, maxLat, minLng, maxLng}
        this._currentPos = null; // {lat, lng}
        this._heading = null;    // compass heading in degrees (0=N, 90=E, CW)
        this._fetchKey = null;   // replay_id used for last fetch
        this._fetching = false;
    }

    buildContent(c) {
        c.innerHTML = '<div class="trackmap-layout"><canvas></canvas></div>';
        this.canvas = c.querySelector('canvas');
        this.ctx = this.canvas.getContext('2d');
    }

    update(store, now) {
        const f = store.currentFrame;
        if (!f) return;

        // Fetch track outline from server when replay changes
        this._maybeFetchOutline();

        const m = f.motion;
        const lat = m?.latitude;
        const lng = m?.longitude;
        if (lat == null || lng == null || (lat === 0 && lng === 0)) {
            this.renderCanvas();
            return;
        }

        // Compass heading from server (0=N, 90=E, CW in degrees)
        this._heading = m?.heading ?? null;

        this._currentPos = { lat, lng };
        this.renderCanvas();
    }

    async _maybeFetchOutline() {
        // Use replay_id from the replay player if available
        const rid = (typeof replayPlayer !== 'undefined' && replayPlayer?.info?.replay_id) || null;
        if (!rid || rid === this._fetchKey || this._fetching) return;

        this._fetching = true;
        try {
            const resp = await fetch(apiBase() + '/api/replay/trackmap');
            if (!resp.ok) return;
            const data = await resp.json();
            if (!Array.isArray(data.outline) || data.outline.length < 2) return;

            this._outline = data.outline;
            this._fetchKey = rid;
            this._heading = null;

            // Compute bounds
            let minLat = Infinity, maxLat = -Infinity;
            let minLng = Infinity, maxLng = -Infinity;
            for (const [lat, lng] of this._outline) {
                if (lat < minLat) minLat = lat;
                if (lat > maxLat) maxLat = lat;
                if (lng < minLng) minLng = lng;
                if (lng > maxLng) maxLng = lng;
            }
            this._bounds = { minLat, maxLat, minLng, maxLng };
        } catch (e) {
            /* ignore fetch errors */
        } finally {
            this._fetching = false;
        }
    }

    onResize() {
        this.canvas.width = 0;
        this.canvas.height = 0;
    }

    renderCanvas() {
        const canvas = this.canvas;
        const dpr = window.devicePixelRatio || 1;
        const wrap = canvas.parentElement;
        const cw = wrap.clientWidth, ch = wrap.clientHeight;
        if (cw <= 0 || ch <= 0) return;

        const tw = Math.round(cw * dpr), th = Math.round(ch * dpr);
        if (canvas.width !== tw || canvas.height !== th) {
            canvas.width = tw;
            canvas.height = th;
            canvas.style.width = cw + 'px';
            canvas.style.height = ch + 'px';
        }

        const ctx = this.ctx;
        const w = canvas.width, h = canvas.height;
        ctx.clearRect(0, 0, w, h);

        const pts = this._outline;
        const b = this._bounds;
        const pos = this._currentPos;

        if (!pts || pts.length < 2 || !b) {
            ctx.fillStyle = 'rgba(255,255,255,0.3)';
            ctx.font = `${14 * dpr}px -apple-system, sans-serif`;
            ctx.textAlign = 'center';
            ctx.fillText('Waiting for track data...', w / 2, h / 2);
            return;
        }

        // Projection: equirectangular with cos(midLat) correction
        const midLat = (b.minLat + b.maxLat) / 2;
        const lonScale = Math.cos(midLat * Math.PI / 180);

        const rangeX = (b.maxLng - b.minLng) * lonScale;
        const rangeY = b.maxLat - b.minLat;

        // Add padding (10% on each side)
        const pad = 0.1;
        const plotW = w * (1 - 2 * pad);
        const plotH = h * (1 - 2 * pad);
        const padX = w * pad;
        const padY = h * pad;

        // Uniform scale to preserve aspect ratio
        let scale;
        if (rangeX <= 0 && rangeY <= 0) scale = 1;
        else if (rangeX <= 0) scale = plotH / rangeY;
        else if (rangeY <= 0) scale = plotW / rangeX;
        else scale = Math.min(plotW / rangeX, plotH / rangeY);

        const drawnW = rangeX * scale;
        const drawnH = rangeY * scale;
        const offX = padX + (plotW - drawnW) / 2;
        const offY = padY + (plotH - drawnH) / 2;

        const project = (lat, lng) => ({
            x: (lng - b.minLng) * lonScale * scale + offX,
            y: (b.maxLat - lat) * scale + offY,
        });

        // Draw track outline
        ctx.beginPath();
        ctx.strokeStyle = 'rgba(255, 255, 255, 0.25)';
        ctx.lineWidth = 2.5 * dpr;
        ctx.lineJoin = 'round';
        ctx.lineCap = 'round';
        for (let i = 0; i < pts.length; i++) {
            const p = project(pts[i][0], pts[i][1]);
            if (i === 0) ctx.moveTo(p.x, p.y);
            else ctx.lineTo(p.x, p.y);
        }
        ctx.stroke();

        // Draw car position
        if (pos) {
            const cp = project(pos.lat, pos.lng);

            if (this._heading != null) {
                // heading: compass degrees CW from north (0=N, 90=E, 180=S, 270=W)
                // Canvas: 0=right, rotation CW. North=up on our map.
                // Canvas angle = (heading - 90) degrees, converted to radians
                const angle = (this._heading - 90) * Math.PI / 180;
                const arrowLen = 12 * dpr;
                const arrowW = 4 * dpr;

                ctx.save();
                ctx.translate(cp.x, cp.y);
                ctx.rotate(angle);

                ctx.beginPath();
                ctx.moveTo(arrowLen, 0);
                ctx.lineTo(-arrowW, -arrowW);
                ctx.lineTo(-arrowW * 0.3, 0);
                ctx.lineTo(-arrowW, arrowW);
                ctx.closePath();
                ctx.fillStyle = '#00d68f';
                ctx.fill();

                ctx.restore();
            } else {
                ctx.beginPath();
                ctx.arc(cp.x, cp.y, 5 * dpr, 0, Math.PI * 2);
                ctx.fillStyle = '#00d68f';
                ctx.fill();
            }
        }
    }
}
