/* ==================== Computed Metrics ==================== */
const COMPUTED_METRICS_KEY = 'ost-computed-metrics';

class ComputedMetricsManager {
    constructor() {
        this._metrics = []; // { id, name, code, unit, norm, color }
        this._compiled = {}; // id → Function
        this._load();
    }

    _load() {
        try {
            const raw = localStorage.getItem(COMPUTED_METRICS_KEY);
            if (raw) this._metrics = JSON.parse(raw);
        } catch (e) { this._metrics = []; }
        // Register all saved metrics
        for (const m of this._metrics) this._register(m);
    }

    _save() {
        localStorage.setItem(COMPUTED_METRICS_KEY, JSON.stringify(this._metrics));
    }

    _register(m) {
        const fn = this._compile(m.code);
        if (!fn) return;
        this._compiled[m.id] = fn;
        const key = 'computed:' + m.id;
        GRAPH_METRICS[key] = {
            label: m.name,
            color: m.color,
            unit: m.unit || '',
            norm: m.norm || 'autoscale',
            extract: fn,
        };
        if (!GRAPH_METRIC_KEYS.includes(key)) GRAPH_METRIC_KEYS.push(key);
    }

    _unregister(id) {
        const key = 'computed:' + id;
        delete GRAPH_METRICS[key];
        delete this._compiled[id];
        const idx = GRAPH_METRIC_KEYS.indexOf(key);
        if (idx !== -1) GRAPH_METRIC_KEYS.splice(idx, 1);
    }

    _compile(code) {
        try {
            return new Function('f', code);
        } catch (e) {
            console.error('Computed metric compile error:', e);
            return null;
        }
    }

    get metrics() { return this._metrics; }

    add(def) {
        this._metrics.push(def);
        this._register(def);
        this._save();
    }

    update(id, def) {
        const idx = this._metrics.findIndex(m => m.id === id);
        if (idx === -1) return;
        this._unregister(id);
        this._metrics[idx] = def;
        this._register(def);
        this._save();
    }

    remove(id) {
        this._unregister(id);
        this._metrics = this._metrics.filter(m => m.id !== id);
        this._save();
    }

    nextId() {
        return 'cm_' + Date.now().toString(36) + '_' + Math.random().toString(36).slice(2, 6);
    }

    // Open the computed metrics modal
    openModal(editId) {
        if (document.getElementById('computed-modal')) return;
        const existing = editId ? this._metrics.find(m => m.id === editId) : null;

        const overlay = document.createElement('div');
        overlay.id = 'computed-modal';
        overlay.className = 'cm-overlay';

        const modal = document.createElement('div');
        modal.className = 'cm-modal';

        const title = document.createElement('div');
        title.className = 'cm-modal-title';
        title.textContent = existing ? 'Edit Computed Metric' : 'New Computed Metric';
        modal.appendChild(title);

        // Name
        const nameRow = this._formRow('Name', 'text', existing?.name || '', 'e.g. Speed MPH');
        modal.appendChild(nameRow.row);

        // Unit
        const unitRow = this._formRow('Unit', 'text', existing?.unit || '', 'e.g. mph, %, G');
        modal.appendChild(unitRow.row);

        // Normalization
        const normRow = document.createElement('div');
        normRow.className = 'cm-form-row';
        const normLabel = document.createElement('label');
        normLabel.className = 'cm-form-label';
        normLabel.textContent = 'Scale';
        const normSelect = document.createElement('select');
        normSelect.className = 'cm-form-input';
        for (const opt of ['autoscale', 'pct', 'centered', 'boolean']) {
            const o = document.createElement('option');
            o.value = opt;
            o.textContent = opt;
            if ((existing?.norm || 'autoscale') === opt) o.selected = true;
            normSelect.appendChild(o);
        }
        normRow.appendChild(normLabel);
        normRow.appendChild(normSelect);
        modal.appendChild(normRow);

        // Color
        const colorRow = document.createElement('div');
        colorRow.className = 'cm-form-row';
        const colorLabel = document.createElement('label');
        colorLabel.className = 'cm-form-label';
        colorLabel.textContent = 'Color';
        const colorInput = document.createElement('input');
        colorInput.type = 'color';
        colorInput.className = 'cm-color-input';
        colorInput.value = existing?.color || nextCustomColor();
        colorRow.appendChild(colorLabel);
        colorRow.appendChild(colorInput);
        modal.appendChild(colorRow);

        // Code
        const codeLabel = document.createElement('label');
        codeLabel.className = 'cm-form-label';
        codeLabel.textContent = 'Code (receives frame as f, must return a number)';
        modal.appendChild(codeLabel);

        const codeArea = document.createElement('textarea');
        codeArea.className = 'cm-code-input';
        codeArea.rows = 6;
        codeArea.spellcheck = false;
        codeArea.value = existing?.code || 'return (f.vehicle?.speed ?? 0) * 2.237;';
        codeArea.placeholder = 'return (f.vehicle?.speed ?? 0) * 2.237;';
        modal.appendChild(codeArea);

        // Error display
        const errorEl = document.createElement('div');
        errorEl.className = 'cm-error';
        modal.appendChild(errorEl);

        // Test button + result
        const testRow = document.createElement('div');
        testRow.className = 'cm-test-row';
        const testBtn = document.createElement('button');
        testBtn.className = 'cm-btn cm-btn-test';
        testBtn.textContent = 'Test';
        const testResult = document.createElement('span');
        testResult.className = 'cm-test-result';
        testRow.appendChild(testBtn);
        testRow.appendChild(testResult);
        modal.appendChild(testRow);

        testBtn.addEventListener('click', () => {
            errorEl.textContent = '';
            testResult.textContent = '';
            const fn = this._compile(codeArea.value);
            if (!fn) {
                errorEl.textContent = 'Compilation error — check syntax';
                return;
            }
            try {
                const frame = store.currentFrame;
                if (!frame) { testResult.textContent = 'No frame data'; return; }
                const result = fn(frame);
                testResult.textContent = typeof result === 'number'
                    ? `= ${result.toFixed(4)} ${unitRow.input.value}`
                    : `= ${JSON.stringify(result)} (expected number)`;
            } catch (e) {
                errorEl.textContent = 'Runtime error: ' + e.message;
            }
        });

        // Buttons
        const btnRow = document.createElement('div');
        btnRow.className = 'cm-btn-row';

        const cancelBtn = document.createElement('button');
        cancelBtn.className = 'cm-btn cm-btn-cancel';
        cancelBtn.textContent = 'Cancel';
        cancelBtn.addEventListener('click', () => overlay.remove());

        const saveBtn = document.createElement('button');
        saveBtn.className = 'cm-btn cm-btn-save';
        saveBtn.textContent = existing ? 'Update' : 'Create';
        saveBtn.addEventListener('click', () => {
            const name = nameRow.input.value.trim();
            if (!name) { errorEl.textContent = 'Name is required'; return; }
            const code = codeArea.value.trim();
            if (!code) { errorEl.textContent = 'Code is required'; return; }
            const fn = this._compile(code);
            if (!fn) { errorEl.textContent = 'Compilation error — check syntax'; return; }

            const def = {
                id: existing?.id || this.nextId(),
                name,
                code,
                unit: unitRow.input.value.trim(),
                norm: normSelect.value,
                color: colorInput.value,
            };

            if (existing) {
                this.update(def.id, def);
            } else {
                this.add(def);
            }
            overlay.remove();
            requestRedraw();
        });

        btnRow.appendChild(cancelBtn);
        btnRow.appendChild(saveBtn);
        modal.appendChild(btnRow);

        overlay.appendChild(modal);
        overlay.addEventListener('click', (e) => {
            if (e.target === overlay) overlay.remove();
        });
        document.body.appendChild(overlay);

        requestAnimationFrame(() => nameRow.input.focus());
    }

    // Open a list modal showing all computed metrics with edit/delete
    openListModal() {
        if (document.getElementById('computed-modal')) return;

        const overlay = document.createElement('div');
        overlay.id = 'computed-modal';
        overlay.className = 'cm-overlay';

        const modal = document.createElement('div');
        modal.className = 'cm-modal';

        const title = document.createElement('div');
        title.className = 'cm-modal-title';
        title.textContent = 'Computed Metrics';
        modal.appendChild(title);

        const list = document.createElement('div');
        list.className = 'cm-list';

        const renderList = () => {
            list.innerHTML = '';
            if (this._metrics.length === 0) {
                const empty = document.createElement('div');
                empty.className = 'no-data';
                empty.textContent = 'No computed metrics yet';
                list.appendChild(empty);
            }
            for (const m of this._metrics) {
                const item = document.createElement('div');
                item.className = 'cm-list-item';

                const dot = document.createElement('span');
                dot.className = 'cm-list-dot';
                dot.style.background = m.color;

                const name = document.createElement('span');
                name.className = 'cm-list-name';
                name.textContent = m.name;

                const unit = document.createElement('span');
                unit.className = 'cm-list-unit';
                unit.textContent = m.unit || '';

                const editBtn = document.createElement('button');
                editBtn.className = 'cm-btn cm-btn-edit';
                editBtn.textContent = 'Edit';
                editBtn.addEventListener('click', () => {
                    overlay.remove();
                    this.openModal(m.id);
                });

                const delBtn = document.createElement('button');
                delBtn.className = 'cm-btn cm-btn-del';
                delBtn.textContent = 'Del';
                delBtn.addEventListener('click', () => {
                    this.remove(m.id);
                    renderList();
                    requestRedraw();
                });

                item.appendChild(dot);
                item.appendChild(name);
                item.appendChild(unit);
                item.appendChild(editBtn);
                item.appendChild(delBtn);
                list.appendChild(item);
            }
        };
        renderList();
        modal.appendChild(list);

        const btnRow = document.createElement('div');
        btnRow.className = 'cm-btn-row';

        const closeBtn = document.createElement('button');
        closeBtn.className = 'cm-btn cm-btn-cancel';
        closeBtn.textContent = 'Close';
        closeBtn.addEventListener('click', () => overlay.remove());

        const addBtn = document.createElement('button');
        addBtn.className = 'cm-btn cm-btn-save';
        addBtn.textContent = '+ New Metric';
        addBtn.addEventListener('click', () => {
            overlay.remove();
            this.openModal();
        });

        btnRow.appendChild(closeBtn);
        btnRow.appendChild(addBtn);
        modal.appendChild(btnRow);

        overlay.appendChild(modal);
        overlay.addEventListener('click', (e) => {
            if (e.target === overlay) overlay.remove();
        });
        document.body.appendChild(overlay);
    }

    _formRow(label, type, value, placeholder) {
        const row = document.createElement('div');
        row.className = 'cm-form-row';
        const lbl = document.createElement('label');
        lbl.className = 'cm-form-label';
        lbl.textContent = label;
        const input = document.createElement('input');
        input.type = type;
        input.className = 'cm-form-input';
        input.value = value;
        if (placeholder) input.placeholder = placeholder;
        row.appendChild(lbl);
        row.appendChild(input);
        return { row, input };
    }
}
