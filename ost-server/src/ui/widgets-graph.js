/* ==================== Graph Helpers ==================== */
// Compute a "nice" step size for axis intervals.
// Returns a value from the sequence 1, 2, 5, 10, 20, 50, 100, 200, 500, ...
function niceStep(range, targetSteps) {
    if (range <= 0) return 1;
    const rough = range / targetSteps;
    const mag = Math.pow(10, Math.floor(Math.log10(rough)));
    const norm = rough / mag;
    let nice;
    if (norm <= 1.5) nice = 1;
    else if (norm <= 3.5) nice = 2;
    else if (norm <= 7.5) nice = 5;
    else nice = 10;
    return nice * mag;
}

/* ==================== GraphWidget ==================== */
class GraphWidget extends Widget {
    static DEFAULT_METRICS = [
        { path: 'vehicle.speed' },
        { path: 'vehicle.rpm' },
        { path: 'vehicle.throttle' },
        { path: 'vehicle.brake' },
        { path: 'vehicle.clutch' },
        { path: 'motion.yaw_rate' },
        { path: 'electronics.abs_active', norm: 'boolean' },
        { path: 'vehicle.steering_angle' },
    ];

    constructor(id, defaultLayout, defaultEnabled) {
        super(id || 'graph', 'Graph', defaultLayout || { col: 1, row: 7, width: 12, height: 9 });
        this.hiddenMetrics = new Set(); // metrics that are enabled but visually hidden
        this.customMetrics = new Map(); // path -> { path, label, color, unit, norm, parts }
        if (defaultEnabled) {
            this.enabledMetrics = new Set(defaultEnabled);
        } else {
            this.enabledMetrics = new Set();
            for (const d of GraphWidget.DEFAULT_METRICS) {
                this.enabledMetrics.add(d.path);
                const unitInfo = d.norm ? { unit: '', norm: d.norm } : getMetricUnitInfo(d.path);
                this.customMetrics.set(d.path, {
                    path: d.path,
                    label: deriveLabel(d.path),
                    color: getMetricColor(d.path),
                    unit: unitInfo.unit,
                    norm: unitInfo.norm,
                    parts: d.path.split('.'),
                });
            }
        }
        this.timeWindowMs = 60000;
        this.maxSeen = {};
        this._centeredMax = {}; // { [ukey]: { max, scanAt } } — cached window max
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
                        <option value="120000">2m</option>
                        <option value="300000">5m</option>
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

        // Click to seek to the clicked time position
        this.canvas.addEventListener('click', (e) => {
            const r = this._lastRender;
            if (!r) return;
            const x = e.offsetX;
            if (x >= r.padLeft && x <= r.padLeft + r.pw) {
                const t = r.tMin + ((x - r.padLeft) / r.pw) * r.tRange;
                if (typeof graphSeekToTime === 'function') graphSeekToTime(t, r.isReplay);
            }
        });

        // Horizontal scroll moves cursor in history/replay mode
        this.canvas.addEventListener('wheel', (e) => {
            // Only act on horizontal scroll (or shift+vertical)
            const dx = e.deltaX || (e.shiftKey ? e.deltaY : 0);
            if (dx === 0) return;
            e.preventDefault();
            // Scale scroll speed proportionally to the time window
            const framesPerTick = Math.max(1, Math.round(Math.abs(dx) * this.timeWindowMs / 5000));
            const delta = dx > 0 ? framesPerTick : -framesPerTick;
            if (typeof graphScrollCursor === 'function') graphScrollCursor(delta);
        }, { passive: false });

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

        // Helper to create a legend item for a metric key
        const makeLegendItem = (key, color, label, isCustom) => {
            const hidden = this.hiddenMetrics.has(key);
            const item = document.createElement('span');
            item.className = 'graph-legend-item active' + (hidden ? ' legend-hidden' : '');
            item.dataset.metricKey = key;
            if (isCustom) item.title = key;

            const dot = document.createElement('span');
            dot.className = 'graph-legend-dot';
            dot.style.background = color;
            dot.title = 'Remove';
            // Click dot (shows as × on hover) to remove the metric
            dot.addEventListener('click', (e) => {
                e.stopPropagation();
                if (isCustom) this.customMetrics.delete(key);
                this.enabledMetrics.delete(key);
                this.hiddenMetrics.delete(key);
                this.rebuildLegend();
                if (typeof dashboardSaveGraphs === 'function') dashboardSaveGraphs();
                requestRedraw();
            });

            const labelSpan = document.createElement('span');
            labelSpan.className = 'legend-label';
            labelSpan.textContent = label;

            item.appendChild(dot);
            item.appendChild(labelSpan);

            // Click label to toggle visibility
            labelSpan.addEventListener('click', (e) => {
                e.stopPropagation();
                if (this.hiddenMetrics.has(key)) {
                    this.hiddenMetrics.delete(key);
                    item.classList.remove('legend-hidden');
                } else {
                    this.hiddenMetrics.add(key);
                    item.classList.add('legend-hidden');
                }
                if (typeof dashboardSaveGraphs === 'function') dashboardSaveGraphs();
                requestRedraw();
            });

            // Hover label to dim other series
            labelSpan.addEventListener('mouseenter', () => {
                this.legendEl.classList.add('legend-hovering');
                item.classList.add('legend-hovered');
            });
            labelSpan.addEventListener('mouseleave', () => {
                this.legendEl.classList.remove('legend-hovering');
                item.classList.remove('legend-hovered');
            });

            this.legendEl.appendChild(item);
            this.legendItems[key] = item;
        };

        // Preset metrics
        for (const [key, metric] of Object.entries(GRAPH_METRICS)) {
            if (!this.enabledMetrics.has(key)) continue;
            makeLegendItem(key, metric.color, metric.label, false);
        }

        // Custom metrics
        for (const [path, meta] of this.customMetrics) {
            if (!this.enabledMetrics.has(path)) continue;
            makeLegendItem(path, meta.color, meta.label, true);
        }

        // "+ Metric" button
        const addBtn = document.createElement('span');
        addBtn.className = 'graph-legend-item graph-add-metric-btn';
        addBtn.textContent = '+ Metric';
        addBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            this.openMetricPicker(addBtn);
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
        for (const preset of getAllPresets()) {
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
        const rect = anchor.getBoundingClientRect();
        menu.style.position = 'fixed';
        menu.style.top = (rect.bottom + 4) + 'px';
        menu.style.left = rect.left + 'px';
        document.body.appendChild(menu);
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
        this.hiddenMetrics.clear();
        this.customMetrics.clear();
        this.maxSeen = {};
        this._centeredMax = {};
        // Set graph name
        this.setTitle(preset.name);
        // Resolve each pattern
        const frame = store.currentFrame;
        const allPaths = frame ? this._collectAllPaths(frame) : [];
        for (const pattern of preset.metrics) {
            if (pattern.startsWith('computed:') && GRAPH_METRICS[pattern]) {
                this.enabledMetrics.add(pattern);
            } else if (pattern.includes('*')) {
                for (const p of allPaths) {
                    if (matchMetricFilter(p, pattern)) this.addCustomMetric(p);
                }
            } else {
                this.addCustomMetric(pattern);
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
                else if (typeof value === 'number' || typeof value === 'boolean' || typeof value === 'string') paths.push(path);
            }
        };
        walk(frame, '');
        return paths;
    }

    _resolve(obj, parts) {
        let cur = obj;
        for (const p of parts) {
            if (cur == null || typeof cur !== 'object') return null;
            cur = cur[p];
        }
        return cur;
    }

    openMetricPicker(anchorEl) {
        if (this._picker) { this.closeMetricPicker(); return; }
        const frame = store.currentFrame;

        // Collect all numeric, boolean, and string fields grouped by top-level section
        const sections = {};
        if (frame) {
            const walk = (obj, prefix) => {
                for (const [key, value] of Object.entries(obj)) {
                    if (key === '_frame') continue;
                    const path = prefix ? `${prefix}.${key}` : key;
                    if (value && typeof value === 'object' && !Array.isArray(value)) {
                        walk(value, path);
                    } else if (typeof value === 'number' || typeof value === 'boolean' || typeof value === 'string') {
                        const section = path.split('.')[0];
                        if (!sections[section]) sections[section] = [];
                        sections[section].push(path);
                    }
                }
            };
            walk(frame, '');
        }

        // Build popover
        const popover = document.createElement('div');
        popover.className = 'metric-picker-popover';

        const search = document.createElement('input');
        search.className = 'metric-picker-search';
        search.placeholder = 'Search... (* wildcard, /regex/)';
        search.type = 'text';
        popover.appendChild(search);

        const list = document.createElement('div');
        list.className = 'metric-picker-list';
        popover.appendChild(list);

        // Append popover to DOM before rendering list
        const rect = anchorEl.getBoundingClientRect();
        popover.style.position = 'fixed';
        popover.style.top = (rect.bottom + 4) + 'px';
        popover.style.left = rect.left + 'px';
        document.body.appendChild(popover);
        this._picker = popover;

        // Close on outside click
        this._pickerClickOutside = (e) => {
            if (!popover.contains(e.target) && !e.target.closest('.graph-add-metric-btn')) {
                this.closeMetricPicker();
            }
        };
        setTimeout(() => document.addEventListener('click', this._pickerClickOutside), 0);

        const isPatternFilter = (f) => f.includes('*') || f.startsWith('/');

        // Helper: get metric display value HTML
        const getValHtml = (key) => {
            if (!frame) return '';
            try {
                const metric = GRAPH_METRICS[key];
                if (metric) {
                    const extract = metric.extract(frame);
                    if (extract != null && typeof extract === 'number') {
                        const val = Math.abs(extract) >= 100 ? extract.toFixed(0) : Math.abs(extract) >= 1 ? extract.toFixed(1) : extract.toFixed(2);
                        return `<span class="metric-picker-value">${val} <span class="metric-picker-unit">${metric.unit}</span></span>`;
                    }
                } else {
                    const parts = key.split('.');
                    const rawVal = resolveMetricPathRaw(frame, parts);
                    if (typeof rawVal === 'string') {
                        return `<span class="metric-picker-value">${rawVal} <span class="metric-picker-unit">text</span></span>`;
                    } else if (typeof rawVal === 'boolean') {
                        return `<span class="metric-picker-value">${rawVal ? 'true' : 'false'} <span class="metric-picker-unit">bool</span></span>`;
                    } else if (typeof rawVal === 'number') {
                        const fmt = formatMetricValue(key, rawVal);
                        return `<span class="metric-picker-value">${fmt.text}${fmt.unit ? ' <span class="metric-picker-unit">' + fmt.unit + '</span>' : ''}</span>`;
                    }
                }
            } catch (e) { /* ignore */ }
            return '';
        };

        // Create a metric item with checkbox
        const createItem = (key, label, checked) => {
            const item = document.createElement('div');
            item.className = 'metric-picker-item';

            const checkbox = document.createElement('input');
            checkbox.type = 'checkbox';
            checkbox.className = 'metric-picker-checkbox';
            checkbox.checked = checked;

            const labelSpan = document.createElement('span');
            labelSpan.className = 'metric-picker-path';
            labelSpan.textContent = label;

            item.appendChild(checkbox);
            item.appendChild(labelSpan);

            const valHtml = getValHtml(key);
            if (valHtml) {
                const valWrap = document.createElement('span');
                valWrap.innerHTML = valHtml;
                while (valWrap.firstChild) item.appendChild(valWrap.firstChild);
            }

            const handleToggle = () => {
                if (checkbox.checked) {
                    this.addCustomMetric(key);
                } else {
                    this.enabledMetrics.delete(key);
                    if (!GRAPH_METRICS[key]) this.customMetrics.delete(key);
                    this.rebuildLegend();
                    if (typeof dashboardSaveGraphs === 'function') dashboardSaveGraphs();
                    requestRedraw();
                }
            };

            item.addEventListener('click', (e) => {
                if (e.target === checkbox) return;
                e.stopPropagation();
                checkbox.checked = !checkbox.checked;
                handleToggle();
            });
            checkbox.addEventListener('change', handleToggle);

            return item;
        };

        const renderList = (filter) => {
            list.innerHTML = '';
            const hasPattern = filter && isPatternFilter(filter);
            const filterFn = filter
                ? (key, label) => matchMetricFilter(key, filter) || matchMetricFilter(label, filter)
                : () => true;

            // Enabled metrics at top
            const enabledItems = [];
            for (const key of this.enabledMetrics) {
                const metric = GRAPH_METRICS[key];
                const custom = this.customMetrics.get(key);
                const label = metric ? metric.label : (custom ? custom.label : key);
                if (filterFn(key, label)) enabledItems.push({ key, label: key.startsWith('computed:') ? label : key });
            }
            if (enabledItems.length > 0) {
                const hdr = document.createElement('div');
                hdr.className = 'metric-picker-section';
                hdr.textContent = 'Enabled';
                list.appendChild(hdr);
                for (const m of enabledItems) list.appendChild(createItem(m.key, m.label, true));
            }

            // Available computed metrics (not enabled)
            const computedItems = [];
            for (const [key, metric] of Object.entries(GRAPH_METRICS)) {
                if (!key.startsWith('computed:') || this.enabledMetrics.has(key)) continue;
                if (filterFn(key, metric.label)) computedItems.push({ key, label: metric.label });
            }
            if (computedItems.length > 0) {
                const hdr = document.createElement('div');
                hdr.className = 'metric-picker-section';
                hdr.textContent = 'Computed';
                list.appendChild(hdr);
                for (const m of computedItems) list.appendChild(createItem(m.key, m.label, false));
            }

            // Raw metrics from frame (not already listed)
            if (frame) {
                const listedKeys = new Set([...enabledItems.map(m => m.key), ...computedItems.map(m => m.key)]);
                const uncheckedRaw = [];
                for (const [section, paths] of Object.entries(sections).sort((a, b) => a[0].localeCompare(b[0]))) {
                    const filtered = paths.filter(p => !listedKeys.has(p) && filterFn(p, p));
                    if (filtered.length === 0) continue;
                    const hdr = document.createElement('div');
                    hdr.className = 'metric-picker-section';
                    hdr.textContent = section.charAt(0).toUpperCase() + section.slice(1);
                    list.appendChild(hdr);
                    for (const path of filtered) {
                        list.appendChild(createItem(path, path, false));
                        uncheckedRaw.push(path);
                    }
                }

                // "Add all" button for wildcard/regex
                if (hasPattern && uncheckedRaw.length > 1) {
                    const btn = document.createElement('div');
                    btn.className = 'metric-picker-add-all';
                    btn.textContent = `+ Add all ${uncheckedRaw.length} matches`;
                    btn.addEventListener('click', () => {
                        for (const p of uncheckedRaw) this.addCustomMetric(p);
                        renderList(search.value);
                    });
                    list.insertBefore(btn, list.firstChild);
                }
            }
        };
        renderList('');

        search.addEventListener('input', () => renderList(search.value));
        requestAnimationFrame(() => search.focus());
    }

    closeMetricPicker() {
        if (this._picker) {
            this._picker.remove();
            this._picker = null;
        }
        if (this._pickerClickOutside) {
            document.removeEventListener('click', this._pickerClickOutside);
            this._pickerClickOutside = null;
        }
    }

    addCustomMetric(path) {
        if (this.enabledMetrics.has(path)) return;
        // If it's a preset metric, just enable it
        if (GRAPH_METRICS[path]) {
            this.enabledMetrics.add(path);
        } else {
            if (this.customMetrics.has(path)) return;
            const parts = path.split('.');
            // Check if this metric is boolean in the current frame
            const frame = store.currentFrame;
            const rawVal = frame ? resolveMetricPathRaw(frame, parts) : undefined;
            const isBool = typeof rawVal === 'boolean';
            const isText = typeof rawVal === 'string';
            const unitInfo = isBool ? { unit: '', norm: 'boolean' } : isText ? { unit: '', norm: 'text' } : getMetricUnitInfo(path);
            this.customMetrics.set(path, {
                path,
                label: deriveLabel(path),
                color: getMetricColor(path),
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
        let dataCount, getEntry, centerTime, windowFrom = 0, isReplay = false;
        if (replayBuf && replayBuf.count > 0) {
            const win = replayBuf.getWindowEntries(this.timeWindowMs);
            dataCount = win.count;
            windowFrom = win.windowFrom;
            getEntry = (i) => win.getEntry(win.windowFrom + i);
            centerTime = win.centerTime;
            isReplay = true;
        } else {
            const latestT = store.latestTime();
            let effectiveNow = (latestT && now - latestT > 2000) ? latestT + 500 : now;
            if (store.liveScrollOffsetMs > 0) effectiveNow -= store.liveScrollOffsetMs;
            const range = store.getWindowRange(this.timeWindowMs, effectiveNow);
            dataCount = range.count;
            getEntry = (i) => store.ringAt(range.start + i);
            centerTime = null;
        }
        if (dataCount < 2) { ctx.clearRect(0, 0, w, h); return; }

        // Build unified traces array from enabled (non-hidden) metrics, with display conversion
        const traces = [];
        const boolTraces = [];
        const textTraces = [];
        for (const key of this.enabledMetrics) {
            if (this.hiddenMetrics.has(key)) continue;
            const preset = GRAPH_METRICS[key];
            if (preset) {
                if (preset.norm === 'boolean') {
                    boolTraces.push({ key, color: preset.color, norm: 'boolean', unit: '', dU: '', dM: 1, label: preset.label,
                        getValue: (entry) => { const v = entry[key]; return typeof v === 'boolean' ? v : null; }
                    });
                } else {
                    let dM = 1, dU = preset.unit || '';
                    if (preset.norm === 'pct') { dM = 100; dU = '%'; }
                    else {
                        const conv = applyUnitPref(preset.unit || '', 1);
                        dU = conv.unit;
                        dM = conv.value;
                    }
                    traces.push({ key, color: preset.color, norm: preset.norm, unit: preset.unit || '', dU, dM, getValue: (entry) => entry[key] });
                }
            } else {
                const custom = this.customMetrics.get(key);
                if (custom) {
                    const parts = custom.parts;
                    if (custom.norm === 'boolean') {
                        boolTraces.push({ key, color: custom.color, norm: 'boolean', unit: '', dU: '', dM: 1, label: custom.label,
                            getValue: (entry) => {
                                if (!entry._frame) return null;
                                const v = resolveMetricPathRaw(entry._frame, parts);
                                return typeof v === 'boolean' ? v : null;
                            }
                        });
                    } else if (custom.norm === 'text') {
                        textTraces.push({ key, color: custom.color, label: custom.label,
                            getValue: (entry) => {
                                if (!entry._frame) return null;
                                const v = resolveMetricPathRaw(entry._frame, parts);
                                return typeof v === 'string' ? v : null;
                            }
                        });
                    } else {
                        const unitInfo = getMetricUnitInfo(key);
                        let dU = custom.unit, dM = unitInfo.multiplier || 1;
                        if (custom.norm === 'pct') { dU = '%'; dM = 100; }
                        else {
                            const conv = applyUnitPref(custom.unit, 1);
                            dU = conv.unit;
                            dM = conv.value * (unitInfo.multiplier || 1);
                        }
                        traces.push({ key, color: custom.color, norm: custom.norm, unit: custom.unit, dU, dM, getValue: (entry) => entry._frame ? resolveMetricPathParts(entry._frame, parts) : null });
                    }
                }
            }
        }
        if (traces.length === 0 && boolTraces.length === 0 && textTraces.length === 0) { ctx.clearRect(0, 0, w, h); return; }

        // Compute time range — for replay, always use the full desired window
        // centered on cursor so the timescale doesn't shrink at boundaries
        let tMin, tMax;
        if (isReplay && replayBuf) {
            const centerTime = replayBuf.simTimeMs(replayBuf.cursor);
            tMin = centerTime - this.timeWindowMs / 2;
            tMax = centerTime + this.timeWindowMs / 2;
        } else {
            tMin = getEntry(0).t;
            tMax = getEntry(dataCount - 1).t;
        }
        const tRange = Math.max(tMax - tMin, 1);

        // Helper: get entry time (works even when entry is null in replay)
        const entryTime = isReplay
            ? (i) => replayBuf.simTimeMs(windowFrom + i)
            : (i) => getEntry(i).t;

        // Compute max values grouped by raw unit for shared scaling
        const unitMax = {};
        for (const trace of traces) {
            if (trace.norm === 'autoscale') {
                // maxSeen is monotonically increasing — no need to scan all dataCount entries.
                // Update it from the latest visible entry only (O(1) instead of O(dataCount)).
                const ukey = trace.unit || trace.key;
                const latestEntry = getEntry(dataCount - 1);
                if (latestEntry) {
                    const v = trace.getValue(latestEntry);
                    if (v != null) {
                        const av = Math.abs(v);
                        if (av > (this.maxSeen[ukey] || 0)) this.maxSeen[ukey] = av;
                    }
                }
                unitMax[ukey] = Math.max(this.maxSeen[ukey] || 0.001, unitMax[ukey] || 0);
            } else if (trace.norm === 'centered') {
                // Cached window max: O(1) update from latest entry, full rescan once per second
                const ukey = trace.unit || trace.key;
                const cm = this._centeredMax[ukey] || (this._centeredMax[ukey] = { max: 0, scanAt: 0 });
                const latestEntry = getEntry(dataCount - 1);
                if (latestEntry) {
                    const v = trace.getValue(latestEntry);
                    if (v != null) { const av = Math.abs(v); if (av > cm.max) cm.max = av; }
                }
                if (now - cm.scanAt > 1000) {
                    cm.scanAt = now;
                    let mx = 0;
                    for (let i = 0; i < dataCount; i++) {
                        const entry = getEntry(i);
                        if (!entry) continue;
                        const v = trace.getValue(entry);
                        if (v != null) { const av = Math.abs(v); if (av > mx) mx = av; }
                    }
                    cm.max = mx;
                }
                unitMax[ukey] = Math.max(cm.max, unitMax[ukey] || 0, 0.001);
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

        const boolBarH = 10, boolBarGap = 2;
        const barTraceCount = boolTraces.length + textTraces.length;
        const boolAreaH = barTraceCount > 0 ? barTraceCount * (boolBarH + boolBarGap) + 4 : 0;
        const axisColW = 45;
        // Percentage axes don't need labels (0-100% is obvious), so exclude from padding
        const numAxes = axes.filter(g => g.norm !== 'pct').length;
        const padLeft = 8;
        const padRight = numAxes >= 1 ? numAxes * axisColW + 7 : 36;
        const pad = { top: 8 + boolAreaH, right: padRight, bottom: 16, left: padLeft };
        const pw = w - pad.left - pad.right, ph = h - pad.top - pad.bottom;
        ctx.clearRect(0, 0, w, h);

        // Cache render params for crosshair mouse mapping
        this._lastRender = { tMin, tRange, padLeft: pad.left, pw, isReplay };

        // Helper: format axis label value
        const fmtAxisVal = (v, unit) => {
            const av = Math.abs(v);
            return (av >= 1000 ? v.toFixed(0) : av >= 10 ? v.toFixed(0) : av >= 1 ? v.toFixed(1) : v.toFixed(2)) + ' ' + unit;
        };

        // Precompute nice axis ranges and store niceMax on each axis
        for (const g of axes) {
            if (g.norm === 'pct') {
                g.niceMax = 1; g.step = 0.25; g.niceDisplayMax = 100;
            } else {
                const displayMax = g.rawMax * g.dM;
                const step = niceStep(displayMax, 5);
                const niceDisplayMax = Math.ceil(displayMax / step) * step;
                g.step = step;
                g.niceDisplayMax = niceDisplayMax;
                g.niceMax = g.dM > 0 ? niceDisplayMax / g.dM : g.rawMax;
            }
        }

        // Grid lines — aligned to primary axis nice intervals
        ctx.strokeStyle = 'rgba(255,255,255,0.05)'; ctx.lineWidth = 1;
        if (axes.length >= 1) {
            const g = axes[0];
            const count = Math.round(g.niceDisplayMax / g.step);
            for (let i = 0; i <= count; i++) {
                const frac = g.norm === 'centered'
                    ? 1 - i / count
                    : 1 - i / count;
                const y = pad.top + ph * (1 - frac);
                ctx.beginPath(); ctx.moveTo(pad.left, y); ctx.lineTo(pad.left + pw, y); ctx.stroke();
            }
        }

        // Y-axes — one column per axis group, stacked on the right
        // Percentage axes are skipped (0-100% is self-evident)
        ctx.font = '9px sans-serif'; ctx.textAlign = 'left';
        let axisCol = 0;
        for (let ai = 0; ai < axes.length; ai++) {
            const g = axes[ai];
            if (g.norm === 'pct') continue; // No labels needed for percentage
            const colX = w - padRight + 4 + axisCol * axisColW;
            axisCol++;
            ctx.fillStyle = numAxes === 1 ? 'rgba(255,255,255,0.25)' : g.color;

            if (g.norm === 'centered') {
                const count = Math.round(g.niceDisplayMax / g.step);
                for (let i = 0; i <= count * 2; i++) {
                    const displayVal = -g.niceDisplayMax + i * g.step;
                    const frac = 0.5 + displayVal / (g.niceMax * g.dM * 2);
                    if (frac < -0.01 || frac > 1.01) continue;
                    const y = pad.top + ph * (1 - frac);
                    ctx.fillText(fmtAxisVal(displayVal, g.dU), colX, y + 3);
                }
            } else {
                const count = Math.round(g.niceDisplayMax / g.step);
                for (let i = 0; i <= count; i++) {
                    const displayVal = i * g.step;
                    const frac = displayVal / g.niceDisplayMax;
                    const y = pad.top + ph * (1 - frac);
                    ctx.fillText(fmtAxisVal(displayVal, g.dU), colX, y + 3);
                }
            }
        }

        // Build a lookup from trace to its axis group's niceMax
        const traceAxisMap = new Map();
        for (const trace of traces) {
            const axisKey = trace.norm === 'pct' ? 'pct' : trace.norm + ':' + trace.dU;
            traceAxisMap.set(trace, axisMap.get(axisKey));
        }

        // Helper: normalize a raw value to 0-1 for Y position
        const normalizeVal = (trace, raw) => {
            const g = traceAxisMap.get(trace);
            const maxVal = g ? g.niceMax : (unitMax[trace.unit || trace.key] || 1);
            let val;
            if (trace.norm === 'pct') val = raw;
            else if (trace.norm === 'centered') val = 0.5 + raw / (maxVal * 2);
            else val = raw / maxVal;
            return Math.max(0, Math.min(1, val));
        };

        // Draw loading indicator (diagonal hatching) for unloaded regions
        if (isReplay) {
            const drawHatch = (x0, x1) => {
                if (x1 - x0 < 1) return;
                ctx.save();
                ctx.beginPath();
                ctx.rect(x0, pad.top, x1 - x0, ph);
                ctx.clip();
                ctx.strokeStyle = 'rgba(255,255,255,0.08)';
                ctx.lineWidth = 1;
                const spacing = 8;
                for (let lx = x0 - ph; lx < x1 + ph; lx += spacing) {
                    ctx.beginPath();
                    ctx.moveTo(lx, pad.top + ph);
                    ctx.lineTo(lx + ph, pad.top);
                    ctx.stroke();
                }
                ctx.restore();
            };
            // Check at chunk boundaries for efficiency
            const chunkSize = replayBuf._chunkSize || 300;
            let gapStartX = null;
            for (let i = 0; i < dataCount; i += chunkSize) {
                const entry = getEntry(i);
                const x = pad.left + ((entryTime(i) - tMin) / tRange) * pw;
                if (!entry) {
                    if (gapStartX == null) gapStartX = x;
                } else {
                    if (gapStartX != null) { drawHatch(gapStartX, x); gapStartX = null; }
                }
            }
            // Check last entry
            if (dataCount > 0) {
                const lastEntry = getEntry(dataCount - 1);
                if (!lastEntry && gapStartX == null) {
                    // Last chunk is unloaded but wasn't caught by step
                    const lastChunkStart = dataCount - (dataCount % chunkSize || chunkSize);
                    gapStartX = pad.left + ((entryTime(lastChunkStart) - tMin) / tRange) * pw;
                }
            }
            if (gapStartX != null) drawHatch(gapStartX, pad.left + pw);

            // Show error text for failed chunks in the visible range
            if (replayBuf._failedChunks && replayBuf._failedChunks.size > 0) {
                const errors = new Set();
                for (const [chunkIdx, errMsg] of replayBuf._failedChunks) {
                    const chunkStart = chunkIdx * chunkSize;
                    const chunkEnd = chunkStart + chunkSize;
                    if (chunkEnd > windowFrom && chunkStart < windowFrom + dataCount) {
                        errors.add(errMsg);
                    }
                }
                if (errors.size > 0) {
                    ctx.font = '11px sans-serif';
                    ctx.fillStyle = 'rgba(255, 71, 87, 0.8)';
                    ctx.textAlign = 'center';
                    const errText = 'Failed to load: ' + [...errors].join('; ');
                    ctx.fillText(errText, pad.left + pw / 2, pad.top + ph / 2);
                    ctx.textAlign = 'left';
                }
            }
        }

        // Draw annotation overlays (behind traces)
        if (store.annotations && store.annotations.length > 0) {
            for (const ann of store.annotations) {
                // Find X bounds by scanning visible entries for matching tick range
                let x0 = null, x1 = null;
                if (ann.start_tick != null || ann.end_tick != null) {
                    for (let i = 0; i < dataCount; i++) {
                        const entry = getEntry(i);
                        if (!entry || !entry._frame?.meta?.tick) continue;
                        const tick = entry._frame.meta.tick;
                        const x = pad.left + ((entryTime(i) - tMin) / tRange) * pw;
                        if (ann.start_tick != null && tick >= ann.start_tick && x0 == null) x0 = x;
                        if (ann.end_tick != null && tick <= ann.end_tick) x1 = x;
                    }
                } else if (ann.start_time_s != null || ann.end_time_s != null) {
                    // Time-based: convert seconds to the graph's time coordinate system
                    if (isReplay) {
                        if (ann.start_time_s != null) x0 = pad.left + ((ann.start_time_s * 1000 - tMin) / tRange) * pw;
                        if (ann.end_time_s != null) x1 = pad.left + ((ann.end_time_s * 1000 - tMin) / tRange) * pw;
                    }
                }
                if (x0 == null && x1 == null) continue;
                // Default to full visible range for open-ended annotations
                if (x0 == null) x0 = pad.left;
                if (x1 == null) x1 = pad.left + pw;
                // Clamp to plot area
                x0 = Math.max(x0, pad.left);
                x1 = Math.min(x1, pad.left + pw);
                if (x1 <= x0) continue;

                // Draw colored rectangle
                ctx.fillStyle = ann.color || 'rgba(255, 107, 107, 0.15)';
                ctx.globalAlpha = 0.15;
                ctx.fillRect(x0, pad.top, x1 - x0, ph);
                ctx.globalAlpha = 1.0;

                // Draw title label at top-left of annotation
                if (ann.title) {
                    ctx.font = '9px sans-serif';
                    ctx.fillStyle = ann.color || '#ff6b6b';
                    ctx.globalAlpha = 0.7;
                    ctx.textAlign = 'left';
                    ctx.fillText(ann.title, x0 + 3, pad.top + 10);
                    ctx.globalAlpha = 1.0;
                }
            }
        }

        // Draw each trace
        for (const trace of traces) {
            ctx.beginPath(); ctx.strokeStyle = trace.color; ctx.lineWidth = 1.5;
            let drawing = false;
            for (let i = 0; i < dataCount; i++) {
                const entry = getEntry(i);
                if (!entry) { drawing = false; continue; }
                const raw = trace.getValue(entry);
                if (raw == null) { drawing = false; continue; }
                const x = pad.left + ((entryTime(i) - tMin) / tRange) * pw;
                const y = pad.top + ph * (1 - normalizeVal(trace, raw));
                if (!drawing) { ctx.moveTo(x, y); drawing = true; } else { ctx.lineTo(x, y); }
            }
            ctx.stroke();
        }

        // Boolean bars: colored rectangles above the chart for true periods
        for (let bi = 0; bi < boolTraces.length; bi++) {
            const bt = boolTraces[bi];
            const barY = 8 + bi * (boolBarH + boolBarGap);
            // Draw label on right side
            ctx.font = '8px sans-serif'; ctx.fillStyle = bt.color; ctx.textAlign = 'left';
            ctx.fillText(bt.label, pad.left + pw + 4, barY + boolBarH - 1);
            // Draw bar background
            ctx.fillStyle = 'rgba(255,255,255,0.03)';
            ctx.fillRect(pad.left, barY, pw, boolBarH);
            // Draw true spans
            ctx.fillStyle = bt.color;
            ctx.globalAlpha = 0.5;
            let spanStart = null;
            for (let i = 0; i < dataCount; i++) {
                const entry = getEntry(i);
                if (!entry) { if (spanStart != null) { const x = pad.left + ((entryTime(i) - tMin) / tRange) * pw; ctx.fillRect(spanStart, barY, Math.max(x - spanStart, 1), boolBarH); spanStart = null; } continue; }
                const val = bt.getValue(entry);
                const x = pad.left + ((entryTime(i) - tMin) / tRange) * pw;
                if (val === true) {
                    if (spanStart == null) spanStart = x;
                } else {
                    if (spanStart != null) {
                        ctx.fillRect(spanStart, barY, Math.max(x - spanStart, 1), boolBarH);
                        spanStart = null;
                    }
                }
            }
            // Close final span
            if (spanStart != null) {
                const lastX = pad.left + ((entryTime(dataCount - 1) - tMin) / tRange) * pw;
                ctx.fillRect(spanStart, barY, Math.max(lastX - spanStart, 1), boolBarH);
            }
            ctx.globalAlpha = 1.0;
        }

        // Text/enum bars: colored rectangles with value labels above the chart
        for (let ti = 0; ti < textTraces.length; ti++) {
            const tt = textTraces[ti];
            const barY = 8 + (boolTraces.length + ti) * (boolBarH + boolBarGap);
            // Draw label on right side
            ctx.font = '8px sans-serif'; ctx.fillStyle = 'rgba(255,255,255,0.4)'; ctx.textAlign = 'left';
            ctx.fillText(tt.label, pad.left + pw + 4, barY + boolBarH - 1);
            // Draw bar background
            ctx.fillStyle = 'rgba(255,255,255,0.03)';
            ctx.fillRect(pad.left, barY, pw, boolBarH);
            // Draw value spans with unique color per value
            let spanStart = null, spanVal = null;
            const drawSpan = (startX, endX, val) => {
                const spanW = Math.max(endX - startX, 1);
                ctx.fillStyle = hashStringColor(val);
                ctx.globalAlpha = 0.5;
                ctx.fillRect(startX, barY, spanW, boolBarH);
                ctx.globalAlpha = 1.0;
                // Draw text label at start of span (clipped to span width)
                ctx.save();
                ctx.beginPath();
                ctx.rect(startX, barY, spanW, boolBarH);
                ctx.clip();
                ctx.fillStyle = '#fff';
                ctx.font = '7px sans-serif';
                ctx.textAlign = 'left';
                ctx.fillText(val, startX + 2, barY + boolBarH - 2);
                ctx.restore();
            };
            for (let i = 0; i < dataCount; i++) {
                const entry = getEntry(i);
                const val = entry ? tt.getValue(entry) : null;
                const x = pad.left + ((entryTime(i) - tMin) / tRange) * pw;
                if (val !== spanVal) {
                    if (spanStart != null && spanVal != null) drawSpan(spanStart, x, spanVal);
                    spanStart = (val != null) ? x : null;
                    spanVal = val;
                }
            }
            if (spanStart != null && spanVal != null) {
                drawSpan(spanStart, pad.left + ((entryTime(dataCount - 1) - tMin) / tRange) * pw, spanVal);
            }
        }

        // Crosshair: vertical line + value dots + tooltip
        if (crosshair.t != null) {
            const cx = pad.left + ((crosshair.t - tMin) / tRange) * pw;
            if (cx >= pad.left && cx <= pad.left + pw) {
                ctx.strokeStyle = 'rgba(255,255,255,0.3)';
                ctx.lineWidth = 1;
                ctx.setLineDash([4, 3]);
                const crosshairTop = barTraceCount > 0 ? 8 : pad.top;
                ctx.beginPath(); ctx.moveTo(cx, crosshairTop); ctx.lineTo(cx, pad.top + ph); ctx.stroke();
                ctx.setLineDash([]);

                // Find nearest data entry by binary search (using entryTime)
                let lo = 0, hi = dataCount - 1;
                while (lo < hi) {
                    const mid = (lo + hi) >>> 1;
                    if (entryTime(mid) < crosshair.t) lo = mid + 1;
                    else hi = mid;
                }
                if (lo > 0 && Math.abs(entryTime(lo - 1) - crosshair.t) < Math.abs(entryTime(lo) - crosshair.t)) lo--;
                const entry = getEntry(lo);

                // Collect values and draw dots on each trace (tooltip uses display units)
                const tipItems = [];
                if (entry) {
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
                    for (const bt of boolTraces) {
                        const val = bt.getValue(entry);
                        if (val == null) continue;
                        tipItems.push({ color: bt.color, label: bt.label, displayVal: null, dU: '', _valStr: val ? 'ON' : 'OFF' });
                    }
                    for (const tt of textTraces) {
                        const val = tt.getValue(entry);
                        if (val == null) continue;
                        tipItems.push({ color: hashStringColor(val), label: tt.label, displayVal: null, dU: '', _valStr: val });
                    }
                }

                if (tipItems.length > 0) {
                    ctx.font = '10px sans-serif';
                    const lineH = 16;
                    const tipPadX = 8, tipPadY = 6;
                    let maxLabelW = 0, maxValW = 0;
                    for (const item of tipItems) {
                        maxLabelW = Math.max(maxLabelW, ctx.measureText(item.label).width);
                        if (!item._valStr) {
                            const av = Math.abs(item.displayVal);
                            item._valStr = (av >= 100 ? item.displayVal.toFixed(0) : av >= 1 ? item.displayVal.toFixed(1) : item.displayVal.toFixed(2)) + (item.dU ? ' ' + item.dU : '');
                        }
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
        if (this.hiddenMetrics.size > 0) cfg.hiddenMetrics = [...this.hiddenMetrics];
        if (this.customMetrics.size > 0) {
            cfg.customMetrics = [];
            for (const [path, meta] of this.customMetrics) {
                cfg.customMetrics.push({ path, label: meta.label, color: meta.color, unit: meta.unit, norm: meta.norm });
            }
        }
        return cfg;
    }

    applyConfig(cfg) {
        if (cfg.enabledMetrics) {
            // Migrate legacy preset keys (e.g. 'speed') to raw paths (e.g. 'vehicle.speed')
            this.enabledMetrics = new Set();
            for (const key of cfg.enabledMetrics) {
                if (GRAPH_METRICS[key] && !key.startsWith('computed:') && GRAPH_METRIC_PATHS[key]) {
                    const rawPath = GRAPH_METRIC_PATHS[key];
                    this.enabledMetrics.add(rawPath);
                    // Create customMetric entry if not already in config
                    if (!cfg.customMetrics?.find(cm => cm.path === rawPath)) {
                        const preset = GRAPH_METRICS[key];
                        const unitInfo = getMetricUnitInfo(rawPath);
                        this.customMetrics.set(rawPath, {
                            path: rawPath,
                            label: deriveLabel(rawPath),
                            color: preset.color,
                            unit: unitInfo.unit,
                            norm: unitInfo.norm,
                            parts: rawPath.split('.'),
                        });
                    }
                } else {
                    this.enabledMetrics.add(key);
                }
            }
        }
        if (cfg.hiddenMetrics) this.hiddenMetrics = new Set(cfg.hiddenMetrics);
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
