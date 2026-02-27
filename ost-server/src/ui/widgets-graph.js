/* ==================== GraphWidget ==================== */
class GraphWidget extends Widget {
    constructor(id, defaultLayout, defaultEnabled) {
        super(id || 'graph', 'Graph', defaultLayout || { col: 1, row: 7, width: 12, height: 6 });
        this.enabledMetrics = new Set(defaultEnabled || ['speed', 'rpm', 'throttle', 'brake']);
        this.customMetrics = new Map(); // path -> { path, label, color, unit, norm, parts }
        this.timeWindowMs = 10000;
        this.maxSeen = {};
        this.closable = !!id && id !== 'graph';
        this.titleEditable = true;
        this.onTitleChange = () => { if (typeof dashboardSaveGraphs === 'function') dashboardSaveGraphs(); };
    }

    buildContent(c) {
        c.innerHTML = `
            <div class="graph-layout">
                <div class="graph-controls">
                    <div class="graph-legend"></div>
                    <select class="graph-time-select">
                        <option value="10000">10s</option>
                        <option value="30000">30s</option>
                        <option value="60000">60s</option>
                    </select>
                </div>
                <div class="graph-canvas-wrap"><canvas></canvas></div>
            </div>`;

        this.canvas = c.querySelector('canvas');
        this.ctx = this.canvas.getContext('2d');
        this.legendEl = c.querySelector('.graph-legend');
        this.legendItems = {};
        this._lastRender = null; // cached time mapping for crosshair

        // Crosshair: compute timestamp from mouse X using cached render params
        this.canvas.addEventListener('mousemove', (e) => {
            const r = this._lastRender;
            if (!r) return;
            const x = e.offsetX;
            if (x >= r.padLeft && x <= r.padLeft + r.pw) {
                crosshair.t = r.tMin + ((x - r.padLeft) / r.pw) * r.tRange;
            }
            requestRedraw();
        });
        this.canvas.addEventListener('mouseleave', () => {
            crosshair.t = null;
            requestRedraw();
        });

        this.rebuildLegend();

        const sel = c.querySelector('.graph-time-select');
        sel.value = String(this.timeWindowMs);
        sel.addEventListener('change', (e) => {
            this.timeWindowMs = parseInt(e.target.value);
            if (typeof dashboardSaveGraphs === 'function') dashboardSaveGraphs();
        });

        // Add close button to title bar if closable
        if (this.closable) {
            const closeBtn = document.createElement('button');
            closeBtn.className = 'widget-close-btn';
            closeBtn.textContent = '\u00D7';
            closeBtn.title = 'Remove graph';
            closeBtn.addEventListener('click', (e) => {
                e.stopPropagation();
                if (typeof grid !== 'undefined') grid.removeWidget(this.id);
            });
            this.titleBar.appendChild(closeBtn);
        }
    }

    rebuildLegend() {
        this.legendEl.innerHTML = '';
        this.legendItems = {};

        // Only show enabled preset metrics (disabled ones can be re-added via the picker)
        for (const [key, metric] of Object.entries(GRAPH_METRICS)) {
            if (!this.enabledMetrics.has(key)) continue;
            const item = document.createElement('span');
            item.className = 'graph-legend-item active';
            item.innerHTML = `<span class="graph-legend-dot" style="background:${metric.color}"></span>${metric.label}<span class="custom-legend-remove" title="Remove">\u00D7</span>`;
            item.addEventListener('click', (e) => {
                if (e.target.classList.contains('custom-legend-remove')) {
                    this.enabledMetrics.delete(key);
                    this.rebuildLegend();
                    if (typeof dashboardSaveGraphs === 'function') dashboardSaveGraphs();
                    requestRedraw();
                    return;
                }
            });
            this.legendEl.appendChild(item);
            this.legendItems[key] = item;
        }

        // Custom field metrics (only show enabled)
        for (const [path, meta] of this.customMetrics) {
            if (!this.enabledMetrics.has(path)) continue;
            const item = document.createElement('span');
            item.className = 'graph-legend-item active';
            item.innerHTML = `<span class="graph-legend-dot" style="background:${meta.color}"></span>${meta.label}<span class="custom-legend-remove" title="Remove">\u00D7</span>`;
            item.addEventListener('click', (e) => {
                if (e.target.classList.contains('custom-legend-remove')) {
                    this.customMetrics.delete(path);
                    this.enabledMetrics.delete(path);
                    this.rebuildLegend();
                    if (typeof dashboardSaveGraphs === 'function') dashboardSaveGraphs();
                    requestRedraw();
                    return;
                }
            });
            this.legendEl.appendChild(item);
            this.legendItems[path] = item;
        }

        // Metric suggestions (siblings/cousins of current custom metrics)
        const suggestions = this._computeSuggestions();
        for (const sug of suggestions) {
            const item = document.createElement('span');
            item.className = 'graph-legend-item graph-suggestion';
            item.innerHTML = `<span class="graph-suggestion-plus">+</span>${sug.label}`;
            item.title = sug.path;
            item.addEventListener('click', (e) => {
                e.stopPropagation();
                this.addCustomField(sug.path);
            });
            this.legendEl.appendChild(item);
        }

        // "+ Metric" button
        const addBtn = document.createElement('span');
        addBtn.className = 'graph-legend-item graph-add-field-btn';
        addBtn.textContent = '+ Metric';
        addBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            this.openFieldPicker(addBtn);
        });
        this.legendEl.appendChild(addBtn);

        // Presets dropdown (after + Metric)
        const presetsWrap = document.createElement('span');
        presetsWrap.className = 'graph-legend-item graph-preset-btn';
        presetsWrap.textContent = 'Presets \u25BE';
        presetsWrap.addEventListener('click', (e) => {
            e.stopPropagation();
            this._togglePresetsMenu(presetsWrap);
        });
        this.legendEl.appendChild(presetsWrap);
    }

    _togglePresetsMenu(anchor) {
        if (this._presetMenu) { this._presetMenu.remove(); this._presetMenu = null; return; }
        const menu = document.createElement('div');
        menu.className = 'graph-preset-menu';
        for (const preset of GRAPH_PRESETS) {
            const item = document.createElement('div');
            item.className = 'graph-preset-item';
            item.textContent = preset.name;
            item.addEventListener('click', (e) => {
                e.stopPropagation();
                this._applyPreset(preset);
                menu.remove(); this._presetMenu = null;
            });
            menu.appendChild(item);
        }
        const controls = this.contentArea.querySelector('.graph-controls');
        controls.appendChild(menu);
        this._presetMenu = menu;
        const closeMenu = (e) => {
            if (!menu.contains(e.target) && e.target !== anchor) {
                menu.remove(); this._presetMenu = null;
                document.removeEventListener('click', closeMenu);
            }
        };
        setTimeout(() => document.addEventListener('click', closeMenu), 0);
    }

    _applyPreset(preset) {
        // Clear existing metrics
        this.enabledMetrics.clear();
        this.customMetrics.clear();
        this.maxSeen = {};
        // Set graph name
        this.setTitle(preset.name);
        // Resolve each pattern
        const frame = store.currentFrame;
        const allPaths = frame ? this._collectAllPaths(frame) : [];
        for (const pattern of preset.metrics) {
            if (GRAPH_METRICS[pattern]) {
                this.enabledMetrics.add(pattern);
            } else if (pattern.includes('*')) {
                for (const p of allPaths) {
                    if (matchFieldFilter(p, pattern)) this.addCustomField(p);
                }
            } else {
                this.addCustomField(pattern);
            }
        }
        this.rebuildLegend();
        if (typeof dashboardSaveGraphs === 'function') dashboardSaveGraphs();
        requestRedraw();
    }

    _collectAllPaths(frame) {
        const paths = [];
        const walk = (obj, prefix) => {
            for (const [key, value] of Object.entries(obj)) {
                if (key === '_frame') continue;
                const path = prefix ? `${prefix}.${key}` : key;
                if (value && typeof value === 'object' && !Array.isArray(value)) walk(value, path);
                else if (typeof value === 'number') paths.push(path);
            }
        };
        walk(frame, '');
        return paths;
    }

    _computeSuggestions() {
        const frame = store.currentFrame;
        if (!frame) return [];
        const suggestions = [];
        const seen = new Set(this.enabledMetrics);

        for (const path of this.enabledMetrics) {
            if (GRAPH_METRICS[path]) continue; // skip presets
            const parts = path.split('.');
            if (parts.length < 2) continue;

            // Siblings: same parent, different leaf
            const parentParts = parts.slice(0, -1);
            const parentObj = this._resolve(frame, parentParts);
            if (parentObj && typeof parentObj === 'object') {
                for (const [k, v] of Object.entries(parentObj)) {
                    if (typeof v === 'number') {
                        const sibPath = [...parentParts, k].join('.');
                        if (!seen.has(sibPath)) { suggestions.push({ path: sibPath, label: deriveLabel(sibPath) }); seen.add(sibPath); }
                    }
                }
            }

            // Cousins: same grandparent + same leaf, different intermediate segment
            if (parts.length >= 3) {
                const gpParts = parts.slice(0, -2);
                const leaf = parts[parts.length - 1];
                const gpObj = this._resolve(frame, gpParts);
                if (gpObj && typeof gpObj === 'object') {
                    for (const [k, v] of Object.entries(gpObj)) {
                        if (v && typeof v === 'object' && leaf in v && typeof v[leaf] === 'number') {
                            const cousinPath = [...gpParts, k, leaf].join('.');
                            if (!seen.has(cousinPath)) { suggestions.push({ path: cousinPath, label: deriveLabel(cousinPath) }); seen.add(cousinPath); }
                        }
                    }
                }
            }
        }
        return suggestions.slice(0, 6);
    }

    _resolve(obj, parts) {
        let cur = obj;
        for (const p of parts) {
            if (cur == null || typeof cur !== 'object') return null;
            cur = cur[p];
        }
        return cur;
    }

    openFieldPicker(anchorEl) {
        if (this._picker) { this.closeFieldPicker(); return; }
        const frame = store.currentFrame;
        if (!frame) return;

        // Collect all numeric fields grouped by top-level section
        const sections = {};
        const walk = (obj, prefix) => {
            for (const [key, value] of Object.entries(obj)) {
                if (key === '_frame') continue;
                const path = prefix ? `${prefix}.${key}` : key;
                if (value && typeof value === 'object' && !Array.isArray(value)) {
                    walk(value, path);
                } else if (typeof value === 'number') {
                    const section = path.split('.')[0];
                    if (!sections[section]) sections[section] = [];
                    sections[section].push(path);
                }
            }
        };
        walk(frame, '');

        // Build popover
        const popover = document.createElement('div');
        popover.className = 'field-picker-popover';

        const search = document.createElement('input');
        search.className = 'field-picker-search';
        search.placeholder = 'Search... (* wildcard, /regex/)';
        search.type = 'text';
        popover.appendChild(search);

        const list = document.createElement('div');
        list.className = 'field-picker-list';
        popover.appendChild(list);

        const alreadyAdded = new Set([...this.enabledMetrics]);

        const isPatternFilter = (f) => f.includes('*') || f.startsWith('/');

        const renderList = (filter) => {
            list.innerHTML = '';
            const hasPattern = filter && isPatternFilter(filter);

            // Preset metrics section (show un-enabled presets)
            const presetEntries = Object.entries(GRAPH_METRICS).filter(([key]) => !alreadyAdded.has(key));
            const filteredPresets = filter ? presetEntries.filter(([key, m]) => matchFieldFilter(key, filter) || matchFieldFilter(m.label, filter)) : presetEntries;
            if (filteredPresets.length > 0) {
                const hdr = document.createElement('div');
                hdr.className = 'field-picker-section';
                hdr.textContent = 'Presets';
                list.appendChild(hdr);
                for (const [key, metric] of filteredPresets) {
                    const item = document.createElement('div');
                    item.className = 'field-picker-item';
                    const extract = metric.extract(frame);
                    const val = extract != null ? (Math.abs(extract) >= 100 ? extract.toFixed(0) : Math.abs(extract) >= 1 ? extract.toFixed(1) : extract.toFixed(2)) : null;
                    const valHtml = val != null ? `<span class="field-picker-value">${val} <span class="field-picker-unit">${metric.unit}</span></span>` : '';
                    item.innerHTML = `<span class="field-picker-path">${metric.label}</span>${valHtml}`;
                    item.addEventListener('click', () => { this.addCustomField(key); this.closeFieldPicker(); });
                    list.appendChild(item);
                }
            }

            // Raw field sections — collect all addable matches for "Add all" button
            const addablePaths = [];
            for (const [section, paths] of Object.entries(sections).sort((a, b) => a[0].localeCompare(b[0]))) {
                const filtered = filter ? paths.filter(p => matchFieldFilter(p, filter)) : paths;
                if (filtered.length === 0) continue;

                const hdr = document.createElement('div');
                hdr.className = 'field-picker-section';
                hdr.textContent = section.charAt(0).toUpperCase() + section.slice(1);
                list.appendChild(hdr);

                for (const path of filtered) {
                    const item = document.createElement('div');
                    const added = alreadyAdded.has(path);
                    item.className = 'field-picker-item' + (added ? ' dimmed' : '');
                    const parts = path.split('.');
                    const rawVal = resolveFieldPathParts(frame, parts);
                    const fmt = rawVal != null ? formatFieldValue(path, rawVal) : null;
                    const valHtml = fmt ? `<span class="field-picker-value">${fmt.text}${fmt.unit ? ' <span class="field-picker-unit">' + fmt.unit + '</span>' : ''}</span>` : '';
                    item.innerHTML = `<span class="field-picker-path">${path}</span>${valHtml}`;
                    if (!added) {
                        addablePaths.push(path);
                        item.addEventListener('click', () => {
                            this.addCustomField(path);
                            this.closeFieldPicker();
                        });
                    }
                    list.appendChild(item);
                }
            }

            // "Add all" button when using wildcard/regex with multiple matches
            if (hasPattern && addablePaths.length > 1) {
                const btn = document.createElement('div');
                btn.className = 'field-picker-add-all';
                btn.textContent = `+ Add all ${addablePaths.length} matches`;
                btn.addEventListener('click', () => {
                    for (const p of addablePaths) this.addCustomField(p);
                    this.closeFieldPicker();
                });
                list.insertBefore(btn, list.firstChild);
            }
        };
        renderList('');

        search.addEventListener('input', () => renderList(search.value));

        // Position near anchor
        const controls = this.contentArea.querySelector('.graph-controls');
        controls.appendChild(popover);
        this._picker = popover;

        // Focus search
        requestAnimationFrame(() => search.focus());

        // Close on outside click
        this._pickerClickOutside = (e) => {
            if (!popover.contains(e.target) && e.target !== anchorEl) {
                this.closeFieldPicker();
            }
        };
        setTimeout(() => document.addEventListener('click', this._pickerClickOutside), 0);
    }

    closeFieldPicker() {
        if (this._picker) {
            this._picker.remove();
            this._picker = null;
        }
        if (this._pickerClickOutside) {
            document.removeEventListener('click', this._pickerClickOutside);
            this._pickerClickOutside = null;
        }
    }

    addCustomField(path) {
        if (this.enabledMetrics.has(path)) return;
        // If it's a preset metric, just enable it
        if (GRAPH_METRICS[path]) {
            this.enabledMetrics.add(path);
        } else {
            if (this.customMetrics.has(path)) return;
            const unitInfo = getFieldUnitInfo(path);
            const parts = path.split('.');
            this.customMetrics.set(path, {
                path,
                label: deriveLabel(path),
                color: nextCustomColor(),
                unit: unitInfo.unit,
                norm: unitInfo.norm,
                parts,
            });
            this.enabledMetrics.add(path);
        }
        this.rebuildLegend();
        if (typeof dashboardSaveGraphs === 'function') dashboardSaveGraphs();
    }

    update(store, now, replayBuf) {
        const canvas = this.canvas;
        const dpr = window.devicePixelRatio || 1;
        const wrap = canvas.parentElement;
        const w = wrap.clientWidth, h = wrap.clientHeight;
        if (w <= 0 || h <= 0) return;
        const needsResize = canvas.width !== (w * dpr) || canvas.height !== (h * dpr);
        if (needsResize) { canvas.width = w * dpr; canvas.height = h * dpr; }
        const ctx = this.ctx;
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

        // Abstract data source: replay buffer (centered) or live ring buffer (trailing)
        let dataCount, getEntry, centerTime;
        if (replayBuf && replayBuf.count > 0) {
            const win = replayBuf.getWindowEntries(this.timeWindowMs);
            dataCount = win.count;
            getEntry = (i) => win.entries[win.startIdx + i];
            centerTime = win.centerTime;
        } else {
            const latestT = store.latestTime();
            const effectiveNow = (latestT && now - latestT > 2000) ? latestT + 500 : now;
            const range = store.getWindowRange(this.timeWindowMs, effectiveNow);
            dataCount = range.count;
            getEntry = (i) => store.ringAt(range.start + i);
            centerTime = null;
        }
        if (dataCount < 2) { ctx.clearRect(0, 0, w, h); return; }

        // Build unified traces array from enabled metrics, with display conversion
        const traces = [];
        for (const key of this.enabledMetrics) {
            const preset = GRAPH_METRICS[key];
            if (preset) {
                const dM = preset.norm === 'pct' ? 100 : 1;
                const dU = preset.norm === 'pct' ? '%' : (preset.unit || '');
                traces.push({ key, color: preset.color, norm: preset.norm, unit: preset.unit || '', dU, dM, getValue: (entry) => entry[key] });
            } else {
                const custom = this.customMetrics.get(key);
                if (custom) {
                    const parts = custom.parts;
                    const leaf = parts[parts.length - 1];
                    let dU = custom.unit, dM = 1;
                    if (custom.norm === 'pct') { dU = '%'; dM = 100; }
                    else if (custom.unit === 'm' && METERS_TO_MM_FIELDS.test(leaf)) { dU = 'mm'; dM = 1000; }
                    else if (custom.unit === 'm/s' && MPS_TO_MMPS_FIELDS.test(leaf)) { dU = 'mm/s'; dM = 1000; }
                    traces.push({ key, color: custom.color, norm: custom.norm, unit: custom.unit, dU, dM, getValue: (entry) => entry._frame ? resolveFieldPathParts(entry._frame, parts) : null });
                }
            }
        }
        if (traces.length === 0) { ctx.clearRect(0, 0, w, h); return; }

        const tMin = getEntry(0).t;
        const tMax = getEntry(dataCount - 1).t;
        const tRange = Math.max(tMax - tMin, 1);

        // Compute max values grouped by raw unit for shared scaling
        const unitMax = {};
        for (const trace of traces) {
            if (trace.norm === 'autoscale' || trace.norm === 'centered') {
                let mx = 0;
                for (let i = 0; i < dataCount; i++) {
                    const v = trace.getValue(getEntry(i));
                    if (v != null) { const av = Math.abs(v); if (av > mx) mx = av; }
                }
                const ukey = trace.unit || trace.key;
                unitMax[ukey] = Math.max(mx, unitMax[ukey] || 0, 0.001);
                if (trace.norm === 'autoscale') {
                    this.maxSeen[ukey] = Math.max(unitMax[ukey], this.maxSeen[ukey] || 0);
                    unitMax[ukey] = this.maxSeen[ukey];
                }
            }
        }

        // Group traces into Y-axis groups by display unit + norm category
        const axisMap = new Map();
        for (const trace of traces) {
            const axisKey = trace.norm === 'pct' ? 'pct' : trace.norm + ':' + trace.dU;
            if (!axisMap.has(axisKey)) {
                axisMap.set(axisKey, { dU: trace.dU, dM: trace.dM, norm: trace.norm, color: trace.color, rawMax: 0 });
            }
            const g = axisMap.get(axisKey);
            const ukey = trace.unit || trace.key;
            g.rawMax = Math.max(g.rawMax, unitMax[ukey] || 1);
        }
        const axes = [...axisMap.values()];

        const padLeft = axes.length >= 1 ? 52 : 36;
        const padRight = axes.length >= 2 ? 52 : 8;
        const pad = { top: 8, right: padRight, bottom: 16, left: padLeft };
        const pw = w - pad.left - pad.right, ph = h - pad.top - pad.bottom;
        ctx.clearRect(0, 0, w, h);

        // Cache render params for crosshair mouse mapping
        this._lastRender = { tMin, tRange, padLeft: pad.left, pw };

        // Helper: format axis label value
        const fmtAxisVal = (v, unit) => {
            const av = Math.abs(v);
            return (av >= 1000 ? v.toFixed(0) : av >= 10 ? v.toFixed(0) : av >= 1 ? v.toFixed(1) : v.toFixed(2)) + ' ' + unit;
        };

        // Grid lines
        ctx.strokeStyle = 'rgba(255,255,255,0.05)'; ctx.lineWidth = 1;
        for (let i = 0; i <= 4; i++) {
            const y = pad.top + (ph / 4) * i;
            ctx.beginPath(); ctx.moveTo(pad.left, y); ctx.lineTo(pad.left + pw, y); ctx.stroke();
        }

        // Left Y-axis (first axis group)
        if (axes.length >= 1) {
            const g = axes[0];
            ctx.fillStyle = axes.length === 1 ? 'rgba(255,255,255,0.25)' : g.color;
            ctx.font = '9px sans-serif'; ctx.textAlign = 'right';
            for (let i = 0; i <= 4; i++) {
                const y = pad.top + (ph / 4) * i;
                const frac = 1 - i / 4;
                if (g.norm === 'pct') {
                    ctx.fillText((frac * 100).toFixed(0) + '%', pad.left - 4, y + 3);
                } else if (g.norm === 'centered') {
                    ctx.fillText(fmtAxisVal((frac * 2 - 1) * g.rawMax * g.dM, g.dU), pad.left - 4, y + 3);
                } else {
                    ctx.fillText(fmtAxisVal(frac * g.rawMax * g.dM, g.dU), pad.left - 4, y + 3);
                }
            }
        }

        // Right Y-axis (second axis group)
        if (axes.length >= 2) {
            const g = axes[1];
            ctx.fillStyle = g.color;
            ctx.font = '9px sans-serif'; ctx.textAlign = 'left';
            for (let i = 0; i <= 4; i++) {
                const y = pad.top + (ph / 4) * i;
                const frac = 1 - i / 4;
                if (g.norm === 'pct') {
                    ctx.fillText((frac * 100).toFixed(0) + '%', pad.left + pw + 4, y + 3);
                } else if (g.norm === 'centered') {
                    ctx.fillText(fmtAxisVal((frac * 2 - 1) * g.rawMax * g.dM, g.dU), pad.left + pw + 4, y + 3);
                } else {
                    ctx.fillText(fmtAxisVal(frac * g.rawMax * g.dM, g.dU), pad.left + pw + 4, y + 3);
                }
            }
        }

        // Helper: normalize a raw value to 0-1 for Y position
        const normalizeVal = (trace, raw) => {
            const ukey = trace.unit || trace.key;
            const maxVal = unitMax[ukey] || 1;
            let val;
            if (trace.norm === 'pct') val = raw;
            else if (trace.norm === 'centered') val = 0.5 + raw / (maxVal * 2);
            else val = raw / maxVal;
            return Math.max(0, Math.min(1, val));
        };

        // Draw each trace
        for (const trace of traces) {
            ctx.beginPath(); ctx.strokeStyle = trace.color; ctx.lineWidth = 1.5;
            let drawing = false;
            for (let i = 0; i < dataCount; i++) {
                const entry = getEntry(i);
                const raw = trace.getValue(entry);
                if (raw == null) { drawing = false; continue; }
                const x = pad.left + ((entry.t - tMin) / tRange) * pw;
                const y = pad.top + ph * (1 - normalizeVal(trace, raw));
                if (!drawing) { ctx.moveTo(x, y); drawing = true; } else { ctx.lineTo(x, y); }
            }
            ctx.stroke();
        }

        // Crosshair: vertical line + value dots + tooltip
        if (crosshair.t != null) {
            const cx = pad.left + ((crosshair.t - tMin) / tRange) * pw;
            if (cx >= pad.left && cx <= pad.left + pw) {
                ctx.strokeStyle = 'rgba(255,255,255,0.3)';
                ctx.lineWidth = 1;
                ctx.setLineDash([4, 3]);
                ctx.beginPath(); ctx.moveTo(cx, pad.top); ctx.lineTo(cx, pad.top + ph); ctx.stroke();
                ctx.setLineDash([]);

                // Find nearest data entry by binary search
                let lo = 0, hi = dataCount - 1;
                while (lo < hi) {
                    const mid = (lo + hi) >>> 1;
                    if (getEntry(mid).t < crosshair.t) lo = mid + 1;
                    else hi = mid;
                }
                if (lo > 0 && Math.abs(getEntry(lo - 1).t - crosshair.t) < Math.abs(getEntry(lo).t - crosshair.t)) lo--;
                const entry = getEntry(lo);

                // Collect values and draw dots on each trace (tooltip uses display units)
                const tipItems = [];
                for (const trace of traces) {
                    const raw = trace.getValue(entry);
                    if (raw == null) continue;
                    const ny = pad.top + ph * (1 - normalizeVal(trace, raw));
                    ctx.fillStyle = trace.color;
                    ctx.beginPath(); ctx.arc(cx, ny, 3.5, 0, Math.PI * 2); ctx.fill();
                    const label = GRAPH_METRICS[trace.key]?.label || this.customMetrics.get(trace.key)?.label || trace.key;
                    const displayVal = raw * trace.dM;
                    tipItems.push({ color: trace.color, label, displayVal, dU: trace.dU });
                }

                if (tipItems.length > 0) {
                    ctx.font = '10px sans-serif';
                    const lineH = 16;
                    const tipPadX = 8, tipPadY = 6;
                    let maxLabelW = 0, maxValW = 0;
                    for (const item of tipItems) {
                        maxLabelW = Math.max(maxLabelW, ctx.measureText(item.label).width);
                        const av = Math.abs(item.displayVal);
                        item._valStr = (av >= 100 ? item.displayVal.toFixed(0) : av >= 1 ? item.displayVal.toFixed(1) : item.displayVal.toFixed(2)) + (item.dU ? ' ' + item.dU : '');
                        maxValW = Math.max(maxValW, ctx.measureText(item._valStr).width);
                    }
                    const boxW = 14 + maxLabelW + 12 + maxValW + tipPadX * 2;
                    const boxH = tipItems.length * lineH + tipPadY * 2;
                    let boxX = cx + 12;
                    if (boxX + boxW > w - 4) boxX = cx - boxW - 12;
                    let boxY = pad.top + 4;
                    if (boxY + boxH > h - 4) boxY = h - boxH - 4;

                    ctx.fillStyle = 'rgba(22, 27, 42, 0.92)';
                    ctx.strokeStyle = 'rgba(255,255,255,0.1)';
                    ctx.lineWidth = 1;
                    ctx.beginPath(); ctx.roundRect(boxX, boxY, boxW, boxH, 4); ctx.fill(); ctx.stroke();

                    let ty = boxY + tipPadY + 12;
                    for (const item of tipItems) {
                        ctx.fillStyle = item.color;
                        ctx.beginPath(); ctx.arc(boxX + tipPadX + 4, ty - 4, 3, 0, Math.PI * 2); ctx.fill();
                        ctx.fillStyle = 'rgba(255,255,255,0.6)';
                        ctx.textAlign = 'left';
                        ctx.fillText(item.label, boxX + tipPadX + 14, ty);
                        ctx.fillStyle = '#fff';
                        ctx.textAlign = 'right';
                        ctx.fillText(item._valStr, boxX + boxW - tipPadX, ty);
                        ty += lineH;
                    }
                    ctx.textAlign = 'left';
                }
            }
        }

        // Replay cursor: vertical line at the current playback position
        if (centerTime != null) {
            const cx = pad.left + ((centerTime - tMin) / tRange) * pw;
            if (cx >= pad.left && cx <= pad.left + pw) {
                ctx.strokeStyle = 'rgba(255,255,255,0.5)';
                ctx.lineWidth = 1.5;
                ctx.beginPath(); ctx.moveTo(cx, pad.top); ctx.lineTo(cx, pad.top + ph); ctx.stroke();
            }
        }
    }

    getConfig() {
        const cfg = { id: this.id, enabledMetrics: [...this.enabledMetrics], timeWindowMs: this.timeWindowMs };
        if (this.title !== 'Graph') cfg.graphName = this.title;
        if (this.customMetrics.size > 0) {
            cfg.customMetrics = [];
            for (const [path, meta] of this.customMetrics) {
                cfg.customMetrics.push({ path, label: meta.label, color: meta.color, unit: meta.unit, norm: meta.norm });
            }
        }
        return cfg;
    }

    applyConfig(cfg) {
        if (cfg.enabledMetrics) this.enabledMetrics = new Set(cfg.enabledMetrics);
        if (cfg.timeWindowMs) this.timeWindowMs = cfg.timeWindowMs;
        if (cfg.graphName) this.setTitle(cfg.graphName);
        // Restore custom metrics
        if (cfg.customMetrics) {
            this.customMetrics.clear();
            for (const cm of cfg.customMetrics) {
                this.customMetrics.set(cm.path, {
                    path: cm.path,
                    label: cm.label,
                    color: cm.color,
                    unit: cm.unit,
                    norm: cm.norm,
                    parts: cm.path.split('.'),
                });
            }
        }
        this.rebuildLegend();
        // Update time window select
        const sel = this.contentArea?.querySelector('.graph-time-select');
        if (sel) sel.value = String(this.timeWindowMs);
    }
}
