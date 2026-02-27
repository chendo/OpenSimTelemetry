/* ==================== LapTimingWidget ==================== */
class LapTimingWidget extends Widget {
    constructor() { super('laptiming', 'Lap Timing', { col: 5, row: 13, width: 4, height: 5 }); }

    buildContent(c) {
        c.innerHTML = `
            <div class="lap-grid">
                <div class="lap-item"><div class="metric-label">CURRENT</div><div class="lap-time" id="lt-cur">--:--.---</div></div>
                <div class="lap-item lap-best"><div class="metric-label">BEST</div><div class="lap-time" id="lt-best">--:--.---</div></div>
                <div class="lap-item"><div class="metric-label">LAST</div><div class="lap-time" id="lt-last">--:--.---</div></div>
                <div class="lap-item"><div class="metric-label">LAP</div><div class="lap-number" id="lt-num">--</div></div>
            </div>`;
        this._cache(c, { cur: '#lt-cur', best: '#lt-best', last: '#lt-last', num: '#lt-num' });
    }

    _cache(c, map) { this.els = {}; for (const [k,s] of Object.entries(map)) this.els[k] = c.querySelector(s); }

    update(store) {
        const f = store.currentFrame; if (!f) return;
        const t = f.timing;
        this.els.cur.textContent = t?.current_lap_time != null ? this._fmt(t.current_lap_time) : '--:--.---';
        this.els.best.textContent = t?.best_lap_time != null ? this._fmt(t.best_lap_time) : '--:--.---';
        this.els.last.textContent = t?.last_lap_time != null ? this._fmt(t.last_lap_time) : '--:--.---';
        this.els.num.textContent = t?.lap_number ?? '--';
    }

    _fmt(s) { const m = Math.floor(s / 60); return `${m}:${(s % 60).toFixed(3).padStart(6, '0')}`; }
}

/* ==================== SessionWidget ==================== */
class SessionWidget extends Widget {
    constructor() { super('session', 'Session', { col: 9, row: 13, width: 4, height: 5 }); }

    buildContent(c) {
        c.innerHTML = `
            <div class="session-info">
                <div class="session-row"><span class="session-label">TRACK</span><span class="session-value" id="ss-track">--</span></div>
                <div class="session-row"><span class="session-label">CAR</span><span class="session-value" id="ss-car">--</span></div>
                <div class="session-row"><span class="session-label">SESSION</span><span class="session-value" id="ss-type">--</span></div>
                <div class="session-row"><span class="session-label">FLAG</span><span class="session-value" id="ss-flag">--</span></div>
                <div class="session-row"><span class="session-label">TRACK TEMP</span><span class="session-value" id="ss-ttemp">--</span></div>
                <div class="session-row"><span class="session-label">AIR TEMP</span><span class="session-value" id="ss-atemp">--</span></div>
            </div>
            <div class="section-label-inline">ADAPTERS</div>
            <div id="ss-adapters" class="session-adapters"></div>`;
        this._cache(c, { track: '#ss-track', car: '#ss-car', type: '#ss-type', flag: '#ss-flag',
            ttemp: '#ss-ttemp', atemp: '#ss-atemp', adapters: '#ss-adapters' });
    }

    _cache(c, map) { this.els = {}; for (const [k,s] of Object.entries(map)) this.els[k] = c.querySelector(s); }

    update(store) {
        const f = store.currentFrame;
        if (f) {
            const s = f.session, w = f.weather;
            this.els.track.textContent = s?.track_name ?? '--';
            this.els.car.textContent = s?.car_name ?? '--';
            this.els.type.textContent = s?.session_type ?? '--';
            const flags = s?.flags;
            const flagText = flags ? Object.entries(flags).filter(([,v]) => v === true).map(([k]) => k).join(', ') || 'None' : '--';
            this.els.flag.textContent = flagText;
            this.els.ttemp.textContent = w?.track_temp != null ? w.track_temp.toFixed(0) + '\u00B0C' : '--';
            this.els.atemp.textContent = w?.air_temp != null ? w.air_temp.toFixed(0) + '\u00B0C' : '--';
        }

        // Adapter list (low-frequency update)
        const now = performance.now();
        if (!this._lastAdapterRender || now - this._lastAdapterRender > 2000) {
            this._lastAdapterRender = now;
            this.els.adapters.innerHTML = store.adapters.map(a => `
                <div class="session-adapter-row">
                    <span class="status-dot ${a.active ? 'dot-active' : a.detected ? 'dot-detected' : 'dot-inactive'}"></span>
                    <span>${a.name}</span>
                </div>`).join('');
        }
    }
}

/* ==================== AllFieldsWidget ==================== */
class AllFieldsWidget extends Widget {
    constructor() { super('allfields', 'All Fields', { col: 1, row: 18, width: 12, height: 6 }); }

    buildContent(c) {
        c.innerHTML = `<input type="text" class="fields-filter" id="af-filter" placeholder="Filter fields..."><div class="fields-list" id="af-list"></div>`;
        this.filterInput = c.querySelector('#af-filter');
        this.listEl = c.querySelector('#af-list');
        this.filterInput.addEventListener('input', () => this.renderFields());
    }

    update(store, now) {
        this.lastFrame = store.currentFrame;
        if (!this._lastRender || now - this._lastRender > 100) {
            this._lastRender = now;
            this.renderFields();
        }
    }

    renderFields() {
        if (!this.lastFrame) return;
        const filter = this.filterInput.value.toLowerCase();
        const fields = [];
        const extract = (obj, prefix) => {
            for (const [key, value] of Object.entries(obj)) {
                const fk = prefix ? `${prefix}.${key}` : key;
                if (value && typeof value === 'object' && !Array.isArray(value)) extract(value, fk);
                else {
                    const fmt = formatFieldValue(fk, value);
                    fields.push({ key: fk, text: fmt.text, unit: fmt.unit });
                }
            }
        };
        extract(this.lastFrame, '');
        fields.sort((a, b) => a.key.localeCompare(b.key));
        const filtered = filter ? fields.filter(f => f.key.toLowerCase().includes(filter)) : fields;

        // Reuse existing DOM nodes instead of rebuilding via innerHTML
        const existing = this.listEl.children;
        let i = 0;
        for (; i < filtered.length; i++) {
            const f = filtered[i];
            if (i < existing.length) {
                // Reuse existing node
                const row = existing[i];
                const nameEl = row.firstChild, valEl = row.lastChild;
                if (nameEl.textContent !== f.key) nameEl.textContent = f.key;
                const display = f.unit ? f.text + ' ' + f.unit : f.text;
                if (valEl._display !== display) {
                    valEl._display = display;
                    if (f.unit) {
                        valEl.innerHTML = '';
                        valEl.appendChild(document.createTextNode(f.text + ' '));
                        const u = document.createElement('span');
                        u.className = 'field-unit';
                        u.textContent = f.unit;
                        valEl.appendChild(u);
                    } else {
                        valEl.textContent = f.text;
                    }
                }
            } else {
                // Create new node
                const row = document.createElement('div');
                row.className = 'field-item';
                const nameEl = document.createElement('span');
                nameEl.className = 'field-name';
                nameEl.textContent = f.key;
                const valEl = document.createElement('span');
                valEl.className = 'field-value';
                if (f.unit) {
                    valEl.appendChild(document.createTextNode(f.text + ' '));
                    const u = document.createElement('span');
                    u.className = 'field-unit';
                    u.textContent = f.unit;
                    valEl.appendChild(u);
                } else {
                    valEl.textContent = f.text;
                }
                valEl._display = f.unit ? f.text + ' ' + f.unit : f.text;
                row.appendChild(nameEl);
                row.appendChild(valEl);
                this.listEl.appendChild(row);
            }
        }
        // Remove excess nodes
        while (this.listEl.children.length > filtered.length) {
            this.listEl.lastChild.remove();
        }
    }
}

/* ==================== OutputSinksWidget ==================== */
class OutputSinksWidget extends Widget {
    constructor() { super('sinks', 'Output Sinks', { col: 1, row: 24, width: 12, height: 5 }); this._lastSinkCount = -1; }

    buildContent(c) {
        c.innerHTML = `
            <div id="sk-list" class="sink-list"><div class="no-data">No sinks configured</div></div>
            <div class="section-label-inline">ADD SINK</div>
            <form id="sk-form" class="sink-form">
                <div class="sink-form-group"><div class="sink-form-label">Type</div><select id="sk-type"><option value="http">HTTP POST</option><option value="udp">UDP</option><option value="file">File (NDJSON)</option></select></div>
                <div id="sk-type-fields"></div>
                <div class="sink-form-group"><div class="sink-form-label">Field Filter</div><input type="text" id="sk-mask" placeholder="e.g. rpm,speed,gear"></div>
                <button type="submit" class="btn-add">Add</button>
            </form>`;

        this.listEl = c.querySelector('#sk-list');
        this.typeFieldsEl = c.querySelector('#sk-type-fields');
        const typeSelect = c.querySelector('#sk-type');
        typeSelect.addEventListener('change', () => this.updateTypeFields(typeSelect.value));
        this.updateTypeFields('http');

        c.querySelector('#sk-form').addEventListener('submit', async (e) => {
            e.preventDefault();
            const sinkType = typeSelect.value;
            const config = { id: '', sink_type: { type: sinkType }, field_mask: c.querySelector('#sk-mask').value.trim() || null };
            if (sinkType === 'http') config.sink_type.url = c.querySelector('#sk-url')?.value;
            else if (sinkType === 'udp') { config.sink_type.host = c.querySelector('#sk-host')?.value; config.sink_type.port = parseInt(c.querySelector('#sk-port')?.value); }
            else if (sinkType === 'file') config.sink_type.path = c.querySelector('#sk-path')?.value;
            try { await fetch('/api/sinks', { method: 'POST', headers: {'Content-Type':'application/json'}, body: JSON.stringify(config) }); } catch(e) { console.error(e); }
        });
    }

    updateTypeFields(type) {
        const tf = this.typeFieldsEl;
        if (type === 'http') tf.innerHTML = '<div class="sink-form-group"><div class="sink-form-label">URL</div><input type="url" id="sk-url" placeholder="http://localhost:8080/telemetry" required></div>';
        else if (type === 'udp') tf.innerHTML = '<div class="sink-form-group"><div class="sink-form-label">Host</div><input type="text" id="sk-host" placeholder="127.0.0.1" required></div><div class="sink-form-group"><div class="sink-form-label">Port</div><input type="number" id="sk-port" placeholder="9200" required></div>';
        else if (type === 'file') tf.innerHTML = '<div class="sink-form-group"><div class="sink-form-label">Path</div><input type="text" id="sk-path" placeholder="/tmp/telemetry.ndjson" required></div>';
    }

    update(store) {
        const ver = store._sinksVersion || 0;
        if (ver !== this._lastSinksVersion) {
            this._lastSinksVersion = ver;
            if (store.sinks.length === 0) {
                this.listEl.innerHTML = '<div class="no-data">No sinks configured</div>';
            } else {
                this.listEl.innerHTML = store.sinks.map(s => {
                    const desc = s.sink_type.type === 'http' ? s.sink_type.url : s.sink_type.type === 'udp' ? `${s.sink_type.host}:${s.sink_type.port}` : s.sink_type.path;
                    return `<div class="sink-item"><div><strong>${s.sink_type.type.toUpperCase()}</strong> ${desc}${s.field_mask ? `<br><span style="color:var(--text-muted);font-size:0.6rem">Fields: ${s.field_mask}</span>` : ''}</div><button class="btn-delete" data-id="${s.id}">Delete</button></div>`;
                }).join('');
                this.listEl.querySelectorAll('.btn-delete').forEach(btn => {
                    btn.addEventListener('click', async () => {
                        try { await fetch(`/api/sinks/${btn.dataset.id}`, { method: 'DELETE' }); } catch(e) { console.error(e); }
                    });
                });
            }
        }
    }
}
