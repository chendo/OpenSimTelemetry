/* ==================== AllMetricsWidget ==================== */
class AllMetricsWidget extends Widget {
    constructor() { super('allfields', 'Metrics', { col: 9, row: 15, width: 4, height: 5 }); }

    buildContent(c) {
        c.innerHTML = `
            <div class="metrics-toolbar">
                <input type="text" class="metrics-filter" id="af-filter" placeholder="Filter... (* wildcard, /regex/)">
                <button class="metrics-toggle-btn active" id="af-hide-nulls" title="Hide null values">Hide Nulls</button>
                <button class="metrics-toggle-btn" id="af-show-range" title="Show min/max range">Range</button>
                <div class="metrics-rate-wrap">
                    <button class="metrics-toggle-btn" id="af-rate-btn" title="Update frequency">1 Hz</button>
                    <div class="metrics-rate-menu" id="af-rate-menu">
                        <div class="metrics-rate-opt" data-hz="0">Off</div>
                        <div class="metrics-rate-opt" data-hz="0.1">0.1 Hz</div>
                        <div class="metrics-rate-opt active" data-hz="1">1 Hz</div>
                        <div class="metrics-rate-opt" data-hz="10">10 Hz</div>
                        <div class="metrics-rate-opt" data-hz="30">30 Hz</div>
                        <div class="metrics-rate-opt" data-hz="60">60 Hz</div>
                    </div>
                </div>
            </div>
            <div class="metrics-list" id="af-list"></div>`;
        this.filterInput = c.querySelector('#af-filter');
        this.listEl = c.querySelector('#af-list');
        this._hideNulls = true;
        this._showRange = false;
        this._minMax = {}; // path -> { min, max }
        this._updateIntervalMs = 1000; // 1 Hz default

        this._createGraphPaths = [];
        this.filterInput.addEventListener('input', () => this.renderMetrics());

        // Event delegation for create-graph button (survives innerHTML replacement)
        this.listEl.addEventListener('click', (e) => {
            if (e.target.closest('#af-create-graph') && this._createGraphPaths.length > 0) {
                const id = 'graph-' + Date.now();
                const gw = new GraphWidget(id, { col: 1, row: 100, width: 12, height: 6 }, []);
                gw.init();
                for (const path of this._createGraphPaths) gw.addCustomMetric(path);
                gw.setTitle(this.filterInput.value);
                grid.addWidget(gw);
                grid.saveLayout();
                grid.saveGraphConfigs();
            }
        });

        const nullsBtn = c.querySelector('#af-hide-nulls');
        nullsBtn.addEventListener('click', () => {
            this._hideNulls = !this._hideNulls;
            nullsBtn.classList.toggle('active', this._hideNulls);
            this.renderMetrics();
        });

        const rangeBtn = c.querySelector('#af-show-range');
        rangeBtn.addEventListener('click', () => {
            this._showRange = !this._showRange;
            rangeBtn.classList.toggle('active', this._showRange);
            this.renderMetrics();
        });

        // Update frequency dropdown
        const rateBtn = c.querySelector('#af-rate-btn');
        const rateMenu = c.querySelector('#af-rate-menu');
        rateBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            rateMenu.classList.toggle('open');
        });
        rateMenu.addEventListener('click', (e) => {
            const opt = e.target.closest('.metrics-rate-opt');
            if (!opt) return;
            const hz = parseFloat(opt.dataset.hz);
            this._updateIntervalMs = hz > 0 ? 1000 / hz : Infinity;
            rateBtn.textContent = hz > 0 ? `${hz} Hz` : 'Off';
            rateMenu.querySelectorAll('.metrics-rate-opt').forEach(o => o.classList.remove('active'));
            opt.classList.add('active');
            rateMenu.classList.remove('open');
        });
        document.addEventListener('click', () => rateMenu.classList.remove('open'));
    }

    update(store, now) {
        this.lastFrame = store.currentFrame;
        if (!this._lastRender || now - this._lastRender > this._updateIntervalMs) {
            this._lastRender = now;
            this._updateMinMax();
            this.renderMetrics();
        }
    }

    _updateMinMax() {
        if (!this.lastFrame) return;
        const walk = (obj, prefix) => {
            for (const [key, value] of Object.entries(obj)) {
                const fk = prefix ? `${prefix}.${key}` : key;
                if (value && typeof value === 'object' && !Array.isArray(value)) {
                    walk(value, fk);
                } else if (typeof value === 'number' && isFinite(value)) {
                    const mm = this._minMax[fk];
                    if (mm) {
                        if (value < mm.min) mm.min = value;
                        if (value > mm.max) mm.max = value;
                    } else {
                        this._minMax[fk] = { min: value, max: value };
                    }
                }
            }
        };
        walk(this.lastFrame, '');
    }

    renderMetrics() {
        if (!this.lastFrame) return;
        const filter = this.filterInput.value;

        // Extract all leaf values grouped by top-level section
        const sections = {};
        let totalMatches = 0;
        const allMatchedPaths = [];
        const extract = (obj, prefix) => {
            for (const [key, value] of Object.entries(obj)) {
                const fk = prefix ? `${prefix}.${key}` : key;
                if (value && typeof value === 'object' && !Array.isArray(value)) {
                    extract(value, fk);
                } else {
                    if (this._hideNulls && (value === null || value === undefined)) continue;
                    if (filter && !matchMetricFilter(fk, filter)) continue;
                    // For extras with adapter prefix (e.g. "iracing/Foo"),
                    // group under the adapter name instead of "extras"
                    let section;
                    if (prefix === 'extras' && key.includes('/')) {
                        section = key.split('/')[0];
                    } else {
                        section = fk.split('.')[0];
                    }
                    if (!sections[section]) sections[section] = [];
                    const fmt = formatMetricValue(fk, value);
                    sections[section].push({ key: fk, value, text: fmt.text, unit: fmt.unit });
                    if (typeof value === 'number') allMatchedPaths.push(fk);
                    totalMatches++;
                }
            }
        };
        extract(this.lastFrame, '');

        // Render sections
        const sortedSections = Object.entries(sections).sort((a, b) => a[0].localeCompare(b[0]));
        let html = '';

        // Show "Create Graph" button when filter is active and <15 numeric matches
        if (filter && allMatchedPaths.length > 0 && allMatchedPaths.length <= 15) {
            html += `<div class="metrics-create-graph" id="af-create-graph">Create Graph from ${allMatchedPaths.length} metric${allMatchedPaths.length > 1 ? 's' : ''}</div>`;
        }

        for (const [section, fields] of sortedSections) {
            fields.sort((a, b) => a.key.localeCompare(b.key));
            html += `<div class="metric-section-header">${section.charAt(0).toUpperCase() + section.slice(1)}</div>`;
            for (const f of fields) {
                let rangeHtml = '';
                if (this._showRange) {
                    const mm = this._minMax[f.key];
                    if (mm) {
                        const fmtMin = formatMetricValue(f.key, mm.min);
                        const fmtMax = formatMetricValue(f.key, mm.max);
                        rangeHtml = `<span class="metric-range">${fmtMin.text}\u2013${fmtMax.text}</span>`;
                    }
                }
                const unitHtml = f.unit ? ` <span class="field-unit">${f.unit}</span>` : '';
                html += `<div class="metric-item"><span class="metric-name">${f.key}</span><span class="field-value">${rangeHtml}${f.text}${unitHtml}</span></div>`;
            }
        }
        this.listEl.innerHTML = html;
        this._createGraphPaths = allMatchedPaths;
    }

    resetMinMax() { this._minMax = {}; }
}

/* ==================== OutputSinksWidget ==================== */
class OutputSinksWidget extends Widget {
    constructor() { super('sinks', 'Output Sinks', { col: 1, row: 20, width: 12, height: 5 }); this._lastSinkCount = -1; }

    buildContent(c) {
        c.innerHTML = `
            <div id="sk-list" class="sink-list"><div class="no-data">No sinks configured</div></div>
            <div class="section-label-inline">ADD UDP SINK</div>
            <form id="sk-form" class="sink-form">
                <div class="sink-form-group"><div class="sink-form-label">Host</div><input type="text" id="sk-host" placeholder="127.0.0.1" required></div>
                <div class="sink-form-group"><div class="sink-form-label">Port</div><input type="number" id="sk-port" placeholder="9200" required></div>
                <div class="sink-form-group"><div class="sink-form-label">Update Rate</div><select id="sk-rate"><option value="60">60 Hz</option><option value="30">30 Hz</option><option value="10">10 Hz</option><option value="1">1 Hz</option></select></div>
                <div class="sink-form-group"><div class="sink-form-label">Metric Filter</div><input type="text" id="sk-mask" placeholder="e.g. rpm,speed,gear"></div>
                <button type="submit" class="btn-add">Add</button>
            </form>`;

        this.listEl = c.querySelector('#sk-list');

        c.querySelector('#sk-form').addEventListener('submit', async (e) => {
            e.preventDefault();
            const config = {
                id: '',
                host: c.querySelector('#sk-host').value,
                port: parseInt(c.querySelector('#sk-port').value),
                update_rate_hz: parseFloat(c.querySelector('#sk-rate').value),
                metric_mask: c.querySelector('#sk-mask').value.trim() || null,
            };
            try { await fetch(apiBase() + '/api/sinks', { method: 'POST', headers: {'Content-Type':'application/json'}, body: JSON.stringify(config) }); } catch(e) { console.error(e); }
        });
    }

    update(store) {
        const ver = store._sinksVersion || 0;
        if (ver !== this._lastSinksVersion) {
            this._lastSinksVersion = ver;
            if (store.sinks.length === 0) {
                this.listEl.innerHTML = '<div class="no-data">No sinks configured</div>';
            } else {
                this.listEl.innerHTML = store.sinks.map(s => {
                    const rate = s.update_rate_hz || 60;
                    return `<div class="sink-item"><div><strong>UDP</strong> ${s.host}:${s.port} <span style="color:var(--text-muted);font-size:0.6rem">@ ${rate} Hz</span>${s.metric_mask ? `<br><span style="color:var(--text-muted);font-size:0.6rem">Metrics: ${s.metric_mask}</span>` : ''}</div><button class="btn-delete" data-id="${s.id}">Delete</button></div>`;
                }).join('');
                this.listEl.querySelectorAll('.btn-delete').forEach(btn => {
                    btn.addEventListener('click', async () => {
                        try { await fetch(`${apiBase()}/api/sinks/${btn.dataset.id}`, { method: 'DELETE' }); } catch(e) { console.error(e); }
                    });
                });
            }
        }
    }
}

/* ==================== ApiWidget ==================== */
class ApiWidget extends Widget {
    constructor() { super('api', 'API', { col: 1, row: 25, width: 12, height: 6 }); }

    buildContent(c) {
        const base = location.origin;
        c.innerHTML = `
            <div class="api-docs">
                <div class="api-section">
                    <div class="api-heading">SSE Streams</div>
                    <div class="api-endpoint"><code>GET /api/stream</code> — Unified stream (frame, status, sinks events)</div>
                    <div class="api-endpoint"><code>GET /api/telemetry/stream</code> — Telemetry frames only</div>
                    <div class="api-endpoint" style="margin-left:12px;font-size:0.6rem">Query params: <code>rate</code> (0.01–60, default 60), <code>metric_mask</code></div>
                    <div class="api-example"><code>curl -N "${base}/api/telemetry/stream?rate=1"</code></div>
                    <div class="api-endpoint"><code>GET /api/status/stream</code> — Status updates only</div>
                </div>
                <div class="api-section">
                    <div class="api-heading">REST Endpoints</div>
                    <div class="api-endpoint"><code>GET /api/adapters</code> — List adapters and their status</div>
                    <div class="api-endpoint"><code>POST /api/adapters/:name/toggle</code> — Enable/disable an adapter</div>
                    <div class="api-endpoint"><code>GET /api/sinks</code> — List output sinks</div>
                    <div class="api-endpoint"><code>POST /api/sinks</code> — Create UDP sink</div>
                    <div class="api-endpoint"><code>DELETE /api/sinks/:id</code> — Remove a sink</div>
                </div>
                <div class="api-section">
                    <div class="api-heading">Replay</div>
                    <div class="api-endpoint"><code>POST /api/replay/upload</code> — Upload .ibt file (multipart)</div>
                    <div class="api-endpoint"><code>GET /api/replay/info</code> — Current replay state</div>
                    <div class="api-endpoint"><code>GET /api/replay/frames?start=0&count=100</code> — Fetch frames</div>
                    <div class="api-endpoint"><code>POST /api/replay/control</code> — Play/pause/seek/speed</div>
                </div>
                <div class="api-section">
                    <div class="api-heading">JavaScript Example</div>
                    <pre class="api-code">const es = new EventSource('${base}/api/telemetry/stream');
es.onmessage = (e) => {
  const frame = JSON.parse(e.data);
  console.log(frame.vehicle?.speed);
};</pre>
                </div>
            </div>`;
    }

    update() {} // Static content, no updates needed
}
