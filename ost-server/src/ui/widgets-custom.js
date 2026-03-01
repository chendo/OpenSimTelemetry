/* ==================== Custom iframe Widget ==================== */

class CustomWidget extends Widget {
    constructor(id, url) {
        super(id, 'Custom Widget', { col: 1, row: 1, width: 6, height: 6 });
        this.titleEditable = true;
        this.iframeUrl = url || '';
        this.iframe = null;
        this._lastFrame = null;
    }

    buildContent(container) {
        container.style.padding = '0';
        container.style.overflow = 'hidden';

        if (!this.iframeUrl) {
            container.innerHTML = '<div style="padding:12px;color:#888;font-size:13px;">No URL configured. Double-click the title to rename, or configure a URL.</div>';
            return;
        }

        this.iframe = document.createElement('iframe');
        this.iframe.src = this.iframeUrl;
        this.iframe.style.cssText = 'width:100%;height:100%;border:none;background:#111;';
        this.iframe.sandbox = 'allow-scripts allow-same-origin';
        container.appendChild(this.iframe);
    }

    update(store) {
        if (!this.iframe || !this.iframe.contentWindow) return;
        if (!this._visible) return;
        const frame = store.latestFrame;
        if (!frame || frame === this._lastFrame) return;
        this._lastFrame = frame;
        try {
            this.iframe.contentWindow.postMessage({ type: 'telemetry', frame }, '*');
        } catch (e) {
            // Cross-origin errors are expected if iframe is on different domain
        }
    }

    onResize() {
        // iframe auto-resizes via CSS 100% width/height
    }

    showConfigDialog() {
        const overlay = document.createElement('div');
        overlay.className = 'settings-overlay';
        overlay.style.display = 'flex';

        const modal = document.createElement('div');
        modal.className = 'settings-modal';
        modal.style.maxWidth = '500px';
        modal.innerHTML = `
            <div class="settings-header">
                <h2>Custom Widget Settings</h2>
                <button class="settings-close">&times;</button>
            </div>
            <div class="settings-body" style="padding:16px;">
                <label style="display:block;margin-bottom:8px;color:#ccc;font-size:13px;">
                    iframe URL
                    <input type="text" id="custom-widget-url" value="${this.iframeUrl}"
                        placeholder="https://example.com/widget.html"
                        style="width:100%;padding:6px 8px;background:#1a1a2e;border:1px solid #333;
                               color:#fff;border-radius:4px;margin-top:4px;box-sizing:border-box;">
                </label>
                <p style="color:#666;font-size:11px;margin:8px 0;">
                    The iframe receives telemetry frames via postMessage.
                    Use <code>window.addEventListener('message', e => { if(e.data.type==='telemetry') { ... } })</code>
                </p>
                <div style="margin-top:16px;text-align:right;">
                    <button id="custom-widget-save" class="header-reset-btn" style="padding:6px 16px;">Save</button>
                </div>
            </div>
        `;

        overlay.appendChild(modal);
        document.body.appendChild(overlay);

        const close = () => overlay.remove();
        modal.querySelector('.settings-close').onclick = close;
        overlay.addEventListener('click', (e) => { if (e.target === overlay) close(); });

        modal.querySelector('#custom-widget-save').onclick = () => {
            const url = modal.querySelector('#custom-widget-url').value.trim();
            this.iframeUrl = url;
            this.contentArea.innerHTML = '';
            this.buildContent(this.contentArea);
            _saveCustomWidgetConfigs();
            close();
        };
    }
}

function _saveCustomWidgetConfigs() {
    const configs = [];
    if (typeof widgets !== 'undefined') {
        for (const w of widgets) {
            if (w instanceof CustomWidget) {
                configs.push({
                    id: w.id,
                    title: w.title,
                    url: w.iframeUrl,
                });
            }
        }
    }
    localStorage.setItem('ost-custom-widgets', JSON.stringify(configs));
}

function _restoreCustomWidgets(grid, widgets) {
    try {
        const configs = JSON.parse(localStorage.getItem('ost-custom-widgets') || '[]');
        for (const cfg of configs) {
            const w = new CustomWidget(cfg.id, cfg.url);
            if (cfg.title) w.setTitle(cfg.title);
            w.onTitleChange = () => _saveCustomWidgetConfigs();
            grid.addWidget(w);
            widgets.push(w);
        }
    } catch (e) {
        console.warn('Failed to restore custom widgets:', e);
    }
}
