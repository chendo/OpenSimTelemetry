/* ==================== Widget Base ==================== */
class Widget {
    constructor(id, title, defaultLayout) {
        this.id = id;
        this.title = title;
        const dl = defaultLayout || {};
        // GridStack uses 0-based x,y; our defaults use 1-based col,row → convert
        this.gsOpts = {
            x: (dl.col || 1) - 1,
            y: (dl.row || 1) - 1,
            w: dl.width || 4,
            h: dl.height || 4,
            minW: 2, minH: 2,
            id: id,
        };
        this._visible = true;

        this.element = document.createElement('div');
        this.element.className = 'widget';
        this.element.dataset.widgetId = id;
        this.element._widget = this;

        this.titleBar = document.createElement('div');
        this.titleBar.className = 'widget-title';

        const titleText = document.createElement('span');
        titleText.className = 'widget-title-text';
        titleText.textContent = title;
        this.titleBar.appendChild(titleText);

        this.contentArea = document.createElement('div');
        this.contentArea.className = 'widget-content';

        this.element.appendChild(this.titleBar);
        this.element.appendChild(this.contentArea);
    }

    init() {
        this.buildContent(this.contentArea);
        widgetVisibilityObserver.observe(this.element);
        return this;
    }

    buildContent(container) {}
    update(store, now) {}
    onResize() {}
}

/* ==================== DashboardGrid (GridStack wrapper) ==================== */
class DashboardGrid {
    constructor(containerEl) {
        this.container = containerEl;
        this.widgets = new Map();
        this.gs = GridStack.init({
            column: 12,
            cellHeight: 40,
            margin: 5,
            handle: '.widget-title',
            animate: true,
            float: false,
            disableOneColumnMode: true,
            alwaysShowResizeHandle: 'mobile',
        }, '#dashboard-grid');

        // Notify widgets on resize
        this.gs.on('resizestop', (event, el) => {
            const w = el.querySelector('.widget')?._widget;
            if (w) w.onResize();
        });
        // Auto-save on any layout change
        this.gs.on('change', () => this.saveLayout());
    }

    addWidget(widget) {
        this.widgets.set(widget.id, widget);
        // GridStack creates the wrapper; we insert our widget into it
        const gsItem = this.gs.addWidget({
            x: widget.gsOpts.x,
            y: widget.gsOpts.y,
            w: widget.gsOpts.w,
            h: widget.gsOpts.h,
            minW: widget.gsOpts.minW,
            minH: widget.gsOpts.minH,
            id: widget.id,
            content: '',
        });
        gsItem.querySelector('.grid-stack-item-content').appendChild(widget.element);
    }

    removeWidget(id) {
        const w = this.widgets.get(id);
        if (w) {
            widgetVisibilityObserver.unobserve(w.element);
            // GridStack needs the grid-stack-item wrapper, not our inner .widget
            const gsItem = w.element.closest('.grid-stack-item');
            if (gsItem) this.gs.removeWidget(gsItem);
            this.widgets.delete(id);
            this.saveLayout();
            this.saveGraphConfigs();
        }
    }

    saveLayout() {
        const layouts = {};
        for (const node of this.gs.getGridItems()) {
            const w = node.querySelector('.widget')?._widget;
            if (!w) continue;
            const n = node.gridstackNode;
            layouts[w.id] = { x: n.x, y: n.y, w: n.w, h: n.h };
        }
        localStorage.setItem(LAYOUT_KEY, JSON.stringify(layouts));
        localStorage.setItem(LAYOUT_VERSION_KEY, LAYOUT_VERSION);
    }

    restoreLayout() {
        if (localStorage.getItem(LAYOUT_VERSION_KEY) !== LAYOUT_VERSION) return false;
        try {
            const layouts = JSON.parse(localStorage.getItem(LAYOUT_KEY));
            if (!layouts) return false;
            this.gs.batchUpdate();
            for (const node of this.gs.getGridItems()) {
                const w = node.querySelector('.widget')?._widget;
                if (!w || !layouts[w.id]) continue;
                const saved = layouts[w.id];
                this.gs.update(node, { x: saved.x, y: saved.y, w: saved.w, h: saved.h });
            }
            this.gs.batchUpdate(false);
            return true;
        } catch { return false; }
    }

    findOpenPosition(width, height) {
        // Let GridStack figure out placement — return 0-based coords
        // We'll just pass the size to addWidget and let it auto-place
        return { col: 0, row: 0 };
    }

    saveGraphConfigs() {
        const configs = [];
        for (const [id, w] of this.widgets) {
            if (w instanceof GraphWidget) configs.push(w.getConfig());
        }
        localStorage.setItem(GRAPHS_KEY, JSON.stringify(configs));
    }

    restoreGraphConfigs() {
        try {
            const saved = JSON.parse(localStorage.getItem(GRAPHS_KEY));
            return Array.isArray(saved) ? saved : null;
        } catch { return null; }
    }

    resetLayout() {
        localStorage.removeItem(LAYOUT_KEY);
        localStorage.removeItem(LAYOUT_VERSION_KEY);
        localStorage.removeItem(GRAPHS_KEY);
        location.reload();
    }
}

