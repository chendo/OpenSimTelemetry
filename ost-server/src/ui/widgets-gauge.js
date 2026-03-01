/* ==================== GaugeWidget ==================== */

// Gauge presets for common telemetry metrics
const GAUGE_PRESETS = {
    'vehicle.rpm': { min: 0, max: 10000, label: 'RPM', bands: [
        { from: 0, to: 0.7, color: '#22c55e' },
        { from: 0.7, to: 0.85, color: '#eab308' },
        { from: 0.85, to: 1.0, color: '#ef4444' },
    ]},
    'vehicle.speed': { min: 0, max: 300, label: 'km/h', multiplier: 3.6, bands: [
        { from: 0, to: 1.0, color: '#38bdf8' },
    ]},
    'engine.water_temp': { min: 50, max: 130, label: 'Water', unit: '\u00b0C', bands: [
        { from: 0, to: 0.6, color: '#3b82f6' },
        { from: 0.6, to: 0.85, color: '#22c55e' },
        { from: 0.85, to: 1.0, color: '#ef4444' },
    ]},
    'engine.oil_temp': { min: 50, max: 160, label: 'Oil', unit: '\u00b0C', bands: [
        { from: 0, to: 0.5, color: '#3b82f6' },
        { from: 0.5, to: 0.8, color: '#22c55e' },
        { from: 0.8, to: 1.0, color: '#ef4444' },
    ]},
    'vehicle.fuel': { min: 0, max: 1, label: 'Fuel', bands: [
        { from: 0, to: 0.15, color: '#ef4444' },
        { from: 0.15, to: 0.4, color: '#eab308' },
        { from: 0.4, to: 1.0, color: '#22c55e' },
    ]},
};

class GaugeWidget extends Widget {
    constructor(id, defaultLayout, config) {
        super(id, config?.label || 'Gauge', defaultLayout || { width: 3, height: 5 });
        this.titleEditable = true;
        this.metricPath = config?.path || 'vehicle.rpm';
        this.minVal = config?.min ?? 0;
        this.maxVal = config?.max ?? 10000;
        this.bands = config?.bands || [{ from: 0, to: 1.0, color: '#22c55e' }];
        this.multiplier = config?.multiplier ?? 1;
        this.unit = config?.unit || '';
        this._metricParts = this.metricPath.split('.');
        this._currentValue = 0;
        this._displayValue = 0; // lerped for animation
        this._canvas = null;
        this._valueEl = null;
        this._labelEl = null;
    }

    buildContent(container) {
        container.style.cssText = 'display:flex;flex-direction:column;align-items:center;justify-content:center;padding:4px';

        this._canvas = document.createElement('canvas');
        this._canvas.style.cssText = 'width:100%;flex:1;min-height:0';
        container.appendChild(this._canvas);

        this._valueEl = document.createElement('div');
        this._valueEl.style.cssText = 'font-family:var(--font-mono);font-size:1.2rem;font-weight:700;color:var(--text-primary);text-align:center;line-height:1';
        container.appendChild(this._valueEl);

        this._labelEl = document.createElement('div');
        this._labelEl.style.cssText = 'font-size:0.6rem;color:var(--text-muted);text-align:center;margin-top:2px';
        this._labelEl.textContent = this.unit || this.metricPath;
        container.appendChild(this._labelEl);

        // Settings button in title bar
        const settingsBtn = document.createElement('button');
        settingsBtn.className = 'widget-title-btn';
        settingsBtn.textContent = '\u2699';
        settingsBtn.title = 'Configure gauge';
        settingsBtn.addEventListener('click', (e) => { e.stopPropagation(); this._openConfig(); });
        this.titleBar.appendChild(settingsBtn);

        // Remove button
        const removeBtn = document.createElement('button');
        removeBtn.className = 'widget-title-btn widget-remove-btn';
        removeBtn.textContent = '\u00d7';
        removeBtn.title = 'Remove gauge';
        removeBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            grid.removeWidget(this.id);
        });
        this.titleBar.appendChild(removeBtn);
    }

    update(store, now) {
        if (!store.currentFrame) return;
        const raw = resolveMetricPathParts(store.currentFrame, this._metricParts);
        if (raw !== null) this._currentValue = raw * this.multiplier;

        // Lerp for smooth needle animation
        const lerpFactor = 0.15;
        this._displayValue += (this._currentValue - this._displayValue) * lerpFactor;

        this._drawGauge();
        const formatted = formatMetricValue(this.metricPath, this._currentValue);
        this._valueEl.textContent = formatted.text;
    }

    onResize() {
        // Canvas resolution needs to match display size
        this._drawGauge();
    }

    _drawGauge() {
        const canvas = this._canvas;
        if (!canvas) return;
        const rect = canvas.getBoundingClientRect();
        const dpr = window.devicePixelRatio || 1;
        const w = rect.width * dpr;
        const h = rect.height * dpr;
        if (w === 0 || h === 0) return;
        canvas.width = w;
        canvas.height = h;

        const ctx = canvas.getContext('2d');
        ctx.clearRect(0, 0, w, h);

        // Gauge dimensions
        const cx = w / 2;
        const cy = h * 0.7;
        const radius = Math.min(cx - 8 * dpr, cy - 8 * dpr);
        if (radius <= 0) return;

        const startAngle = Math.PI * 0.75; // 135 degrees (bottom-left)
        const endAngle = Math.PI * 0.25;   // 45 degrees (bottom-right)
        const totalArc = Math.PI * 1.5;    // 270 degrees sweep

        // Draw threshold bands
        const bandWidth = radius * 0.12;
        for (const band of this.bands) {
            const a1 = startAngle + totalArc * band.from;
            const a2 = startAngle + totalArc * band.to;
            ctx.beginPath();
            ctx.arc(cx, cy, radius - bandWidth / 2, a1, a2);
            ctx.strokeStyle = band.color;
            ctx.globalAlpha = 0.3;
            ctx.lineWidth = bandWidth;
            ctx.lineCap = 'butt';
            ctx.stroke();
        }

        // Draw track (dim arc)
        ctx.globalAlpha = 0.15;
        ctx.beginPath();
        ctx.arc(cx, cy, radius, startAngle, startAngle + totalArc);
        ctx.strokeStyle = '#ffffff';
        ctx.lineWidth = 2 * dpr;
        ctx.stroke();

        // Draw tick marks
        ctx.globalAlpha = 0.4;
        ctx.strokeStyle = '#ffffff';
        ctx.lineWidth = 1 * dpr;
        const tickCount = 10;
        for (let i = 0; i <= tickCount; i++) {
            const angle = startAngle + (totalArc * i) / tickCount;
            const inner = radius - bandWidth - 4 * dpr;
            const outer = radius + 2 * dpr;
            ctx.beginPath();
            ctx.moveTo(cx + Math.cos(angle) * inner, cy + Math.sin(angle) * inner);
            ctx.lineTo(cx + Math.cos(angle) * outer, cy + Math.sin(angle) * outer);
            ctx.stroke();
        }

        // Draw needle
        const range = this.maxVal - this.minVal;
        const normalised = range > 0 ? Math.max(0, Math.min(1, (this._displayValue - this.minVal) / range)) : 0;
        const needleAngle = startAngle + totalArc * normalised;

        ctx.globalAlpha = 1;
        ctx.beginPath();
        ctx.moveTo(cx, cy);
        const needleLen = radius - bandWidth - 8 * dpr;
        ctx.lineTo(
            cx + Math.cos(needleAngle) * needleLen,
            cy + Math.sin(needleAngle) * needleLen
        );

        // Determine needle color from active band
        let needleColor = '#ffffff';
        for (const band of this.bands) {
            if (normalised >= band.from && normalised <= band.to) {
                needleColor = band.color;
                break;
            }
        }
        ctx.strokeStyle = needleColor;
        ctx.lineWidth = 2.5 * dpr;
        ctx.lineCap = 'round';
        ctx.stroke();

        // Center dot
        ctx.beginPath();
        ctx.arc(cx, cy, 4 * dpr, 0, Math.PI * 2);
        ctx.fillStyle = needleColor;
        ctx.fill();

        // Min/max labels
        ctx.globalAlpha = 0.5;
        ctx.font = `${10 * dpr}px ${getComputedStyle(canvas).fontFamily}`;
        ctx.fillStyle = '#ffffff';
        ctx.textAlign = 'center';
        const labelR = radius + 14 * dpr;
        ctx.fillText(
            this.minVal.toString(),
            cx + Math.cos(startAngle) * labelR,
            cy + Math.sin(startAngle) * labelR
        );
        ctx.fillText(
            this.maxVal.toString(),
            cx + Math.cos(startAngle + totalArc) * labelR,
            cy + Math.sin(startAngle + totalArc) * labelR
        );
    }

    getConfig() {
        return {
            id: this.id,
            path: this.metricPath,
            label: this.title,
            min: this.minVal,
            max: this.maxVal,
            bands: this.bands,
            multiplier: this.multiplier,
            unit: this.unit,
        };
    }

    applyConfig(cfg) {
        if (cfg.path) { this.metricPath = cfg.path; this._metricParts = cfg.path.split('.'); }
        if (cfg.label) this.setTitle(cfg.label);
        if (cfg.min != null) this.minVal = cfg.min;
        if (cfg.max != null) this.maxVal = cfg.max;
        if (cfg.bands) this.bands = cfg.bands;
        if (cfg.multiplier != null) this.multiplier = cfg.multiplier;
        if (cfg.unit != null) { this.unit = cfg.unit; if (this._labelEl) this._labelEl.textContent = cfg.unit || this.metricPath; }
    }

    _openConfig() {
        if (document.getElementById('gauge-config-modal')) return;
        const overlay = document.createElement('div');
        overlay.id = 'gauge-config-modal';
        overlay.className = 'cm-overlay';
        const modal = document.createElement('div');
        modal.className = 'cm-modal';
        modal.style.width = '420px';

        // Preset buttons
        const presetNames = Object.keys(GAUGE_PRESETS);
        const presetBtns = presetNames.map(p => `<button class="cm-btn cm-btn-test gauge-preset-btn" data-preset="${p}" style="font-size:0.65rem;padding:3px 6px">${GAUGE_PRESETS[p].label || p}</button>`).join(' ');

        modal.innerHTML = `
            <div class="cm-modal-title">Configure Gauge</div>
            <div style="margin-bottom:8px;display:flex;flex-wrap:wrap;gap:4px">${presetBtns}</div>
            <div class="cm-form-row">
                <span class="cm-form-label">Metric Path</span>
                <input type="text" class="cm-form-input" id="gauge-path" value="${this.metricPath}">
            </div>
            <div class="cm-form-row">
                <span class="cm-form-label">Label</span>
                <input type="text" class="cm-form-input" id="gauge-label" value="${this.title}">
            </div>
            <div class="cm-form-row">
                <span class="cm-form-label">Min</span>
                <input type="number" class="cm-form-input" id="gauge-min" value="${this.minVal}" style="width:80px">
                <span class="cm-form-label" style="margin-left:12px">Max</span>
                <input type="number" class="cm-form-input" id="gauge-max" value="${this.maxVal}" style="width:80px">
            </div>
            <div class="cm-form-row">
                <span class="cm-form-label">Multiplier</span>
                <input type="number" class="cm-form-input" id="gauge-mult" value="${this.multiplier}" step="0.1" style="width:80px">
                <span class="cm-form-label" style="margin-left:12px">Unit</span>
                <input type="text" class="cm-form-input" id="gauge-unit" value="${this.unit}" style="width:80px">
            </div>
            <div class="cm-btn-row">
                <button class="cm-btn cm-btn-cancel" id="gauge-cancel">Cancel</button>
                <button class="cm-btn cm-btn-save" id="gauge-apply">Apply</button>
            </div>
        `;

        overlay.appendChild(modal);
        overlay.addEventListener('click', (e) => { if (e.target === overlay) overlay.remove(); });
        document.body.appendChild(overlay);

        // Preset click handlers
        modal.querySelectorAll('.gauge-preset-btn').forEach(btn => {
            btn.addEventListener('click', () => {
                const key = btn.dataset.preset;
                const p = GAUGE_PRESETS[key];
                modal.querySelector('#gauge-path').value = key;
                modal.querySelector('#gauge-label').value = p.label || key;
                modal.querySelector('#gauge-min').value = p.min;
                modal.querySelector('#gauge-max').value = p.max;
                modal.querySelector('#gauge-mult').value = p.multiplier || 1;
                modal.querySelector('#gauge-unit').value = p.unit || '';
            });
        });

        modal.querySelector('#gauge-cancel').addEventListener('click', () => overlay.remove());
        modal.querySelector('#gauge-apply').addEventListener('click', () => {
            this.metricPath = modal.querySelector('#gauge-path').value.trim();
            this._metricParts = this.metricPath.split('.');
            this.setTitle(modal.querySelector('#gauge-label').value.trim() || this.metricPath);
            this.minVal = parseFloat(modal.querySelector('#gauge-min').value) || 0;
            this.maxVal = parseFloat(modal.querySelector('#gauge-max').value) || 100;
            this.multiplier = parseFloat(modal.querySelector('#gauge-mult').value) || 1;
            this.unit = modal.querySelector('#gauge-unit').value.trim();
            this._labelEl.textContent = this.unit || this.metricPath;

            // Update bands from preset if the path matches
            const preset = GAUGE_PRESETS[this.metricPath];
            if (preset) this.bands = preset.bands;

            this._saveGaugeConfigs();
            overlay.remove();
        });
    }

    _saveGaugeConfigs() {
        // Save all gauge configs to localStorage
        const configs = [];
        for (const [id, w] of grid.widgets) {
            if (w instanceof GaugeWidget) configs.push(w.getConfig());
        }
        localStorage.setItem('ost-dashboard-gauges', JSON.stringify(configs));
    }
}
