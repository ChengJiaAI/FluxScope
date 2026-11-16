/* FluxScope v2.0 — High-Frame-Rate HiDPI Canvas2D Oscilloscope, Overlay, X-Y & FFT Renderer */

(function () {
  const PAD_L = 58;
  const PAD_R = 14;
  const PAD_T = 26;
  const PAD_B = 22;

  function themeColors() {
    const cs = getComputedStyle(document.documentElement);
    const g = (k, fb) => cs.getPropertyValue(k).trim() || fb;
    return {
      canvas: g("--bg-canvas", "#141416"),
      gridMajor: g("--bg-grid-major", "#25262B"),
      gridMinor: g("--bg-grid-minor", "#1B1C20"),
      border: g("--border", "#2D2F36"),
      text: g("--text-primary", "#E6E7EB"),
      textSec: g("--text-secondary", "#989CA6"),
      t1: g("--cursor-t1", "#06B6D4"),
      t2: g("--cursor-t2", "#F59E0B"),
      band: g("--cursor-band", "rgba(6,182,212,0.07)"),
      crosshair: g("--crosshair", "rgba(230,231,235,0.32)"),
    };
  }

  function niceStep(span, targetTicks) {
    if (span <= 0 || !isFinite(span)) return 1;
    const raw = span / Math.max(1, targetTicks);
    const mag = Math.pow(10, Math.floor(Math.log10(raw)));
    const norm = raw / mag;
    let mult = 1;
    if (norm >= 5) mult = 5;
    else if (norm >= 2) mult = 2;
    return mult * mag;
  }

  function fmtAxisNum(v, step) {
    if (Math.abs(v) < 1e-11) return "0";
    if (Math.abs(v) >= 100000 || (Math.abs(v) > 0 && Math.abs(v) < 0.001)) {
      return v.toExponential(1);
    }
    const dec = step < 0.01 ? 4 : step < 0.1 ? 3 : step < 1 ? 2 : step < 10 ? 1 : 0;
    return Number(v.toFixed(dec)).toString();
  }

  function fitCanvasHiDPI(canvas) {
    const dpr = window.devicePixelRatio || 1;
    const w = Math.max(10, canvas.clientWidth || 10);
    const h = Math.max(10, canvas.clientHeight || 10);
    const pw = Math.floor(w * dpr);
    const ph = Math.floor(h * dpr);
    if (canvas.width !== pw || canvas.height !== ph) {
      canvas.width = pw;
      canvas.height = ph;
    }
    const ctx = canvas.getContext("2d", { alpha: false });
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    return { ctx, w, h, dpr };
  }

  function computeSmartAutoBounds(traces) {
    let rawMin = Infinity;
    let rawMax = -Infinity;
    const sampleBuf = [];

    let totalPts = 0;
    for (const tr of traces) {
      if (tr.y) totalPts += tr.y.length;
    }
    const stride = Math.max(1, Math.floor(totalPts / 600));

    for (const tr of traces) {
      const yArr = tr.y;
      if (!yArr || yArr.length === 0) continue;
      const g = Number.isFinite(tr.gain) ? tr.gain : 1.0;
      for (let i = 0; i < yArr.length; i++) {
        const v = yArr[i] * g;
        if (!Number.isFinite(v)) continue;
        if (v < rawMin) rawMin = v;
        if (v > rawMax) rawMax = v;
        if (i % stride === 0) sampleBuf.push(v);
      }
    }

    if (!Number.isFinite(rawMin) || !Number.isFinite(rawMax)) {
      return [-1, 1];
    }

    let ymin = rawMin;
    let ymax = rawMax;

    // Guard against isolated corrupted IEEE-754 UART spikes (e.g. 4e+9) crushing normal waveforms
    if (sampleBuf.length >= 16 && (Math.abs(rawMax) >= 1e6 || Math.abs(rawMin) >= 1e6)) {
      sampleBuf.sort((a, b) => a - b);
      const p02 = sampleBuf[Math.floor(sampleBuf.length * 0.02)];
      const p98 = sampleBuf[Math.min(sampleBuf.length - 1, Math.ceil(sampleBuf.length * 0.98))];
      const med = sampleBuf[sampleBuf.length >> 1];
      const coreSpan = Math.max(Math.abs(p98 - p02), Math.abs(med) * 0.2, 1.0);
      if (rawMax - rawMin > coreSpan * 25) {
        const loFence = Math.min(-1e6, p02 - coreSpan * 15);
        const hiFence = Math.max(1e6, p98 + coreSpan * 15);
        let cMin = Infinity;
        let cMax = -Infinity;
        for (const tr of traces) {
          const yArr = tr.y;
          if (!yArr) continue;
          const g = Number.isFinite(tr.gain) ? tr.gain : 1.0;
          for (let i = 0; i < yArr.length; i++) {
            const v = yArr[i] * g;
            if (Number.isFinite(v) && v >= loFence && v <= hiFence) {
              if (v < cMin) cMin = v;
              if (v > cMax) cMax = v;
            }
          }
        }
        if (Number.isFinite(cMin) && Number.isFinite(cMax)) {
          ymin = cMin;
          ymax = cMax;
        }
      }
    }

    if (Math.abs(ymax - ymin) < 1e-6) {
      const pad = Math.max(1.0, Math.abs(ymax) * 0.15);
      return [ymin - pad, ymax + pad];
    }
    const pad = (ymax - ymin) * 0.09;
    return [ymin - pad, ymax + pad];
  }

  function binaryNearestIdx(arr, x) {
    let lo = 0;
    let hi = arr.length - 1;
    if (hi <= 0) return 0;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (arr[mid] < x) lo = mid + 1;
      else hi = mid;
    }
    if (lo > 0 && Math.abs(arr[lo - 1] - x) < Math.abs(arr[lo] - x)) {
      return lo - 1;
    }
    return lo;
  }

  /**
   * Synchronized Multi-Pane Time-Domain Oscilloscope Controller
   */
  class MultiScopeGroup {
    constructor(callbacks) {
      this.cb = callbacks || {};
      this.panes = new Map(); // plotId -> { canvas, legendEl, title, yRangeCustom: null|[min,max], autoBounds:[min,max] }
      this.tracesByPlot = new Map();
      this.viewTMin = 0;
      this.viewTMax = 10;
      this.fullTMin = 0;
      this.fullTMax = 10;
      this.cursorEnabled = true;
      this.cursorT1 = 1;
      this.cursorT2 = 3;
      this.hoverTime = null;
      this.followEnabled = true;
      this.followAnchorRatio = 1.0;
      this.anchorDragging = false;
      this._rafPending = false;
    }

    setPanes(paneSpecs) {
      this.panes.clear();
      for (const sp of paneSpecs) {
        if (sp.legendEl) {
          sp.legendEl.style.display = "none";
        }
        this.panes.set(sp.id, {
          id: sp.id,
          title: sp.title,
          canvas: sp.canvas,
          legendEl: sp.legendEl,
          yCustom: null,
          yAuto: [-1, 1],
          lastTopPad: PAD_T,
          lastPh: 200,
        });
        this._bindEvents(sp.id, sp.canvas);
      }
      this.scheduleRender();
    }

    getPixelWidth() {
      for (const p of this.panes.values()) {
        const w = p.canvas.clientWidth;
        if (w > 80) return Math.floor(w - PAD_L - PAD_R);
      }
      return 960;
    }

    setData(payload, preserveManualX) {
      this.fullTMin = payload.full_t_min;
      this.fullTMax = payload.full_t_max;
      if (!preserveManualX) {
        this.viewTMin = payload.view_t_min;
        this.viewTMax = payload.view_t_max;
      }
      this.cursorEnabled = payload.cursor_enabled;
      this.cursorT1 = payload.cursor_t1;
      this.cursorT2 = payload.cursor_t2;

      this.tracesByPlot.clear();
      for (const tr of payload.traces || []) {
        if (!this.tracesByPlot.has(tr.plot_id)) {
          this.tracesByPlot.set(tr.plot_id, []);
        }
        this.tracesByPlot.get(tr.plot_id).push(tr);
      }

      // Compute outlier-safe auto Y bounds per plot (respecting display_gain)
      for (const [pid, pane] of this.panes.entries()) {
        const list = this.tracesByPlot.get(pid) || [];
        pane.yAuto = computeSmartAutoBounds(list);
      }

      this.scheduleRender();
    }

    clearAllTraces() {
      this.tracesByPlot.clear();
      for (const p of this.panes.values()) {
        p.yCustom = null;
        p.yAuto = [-1, 1];
      }
      this.scheduleRender();
    }

    resetZoomFull() {
      for (const p of this.panes.values()) {
        p.yCustom = null;
      }
      this.viewTMin = this.fullTMin;
      this.viewTMax = Math.max(this.fullTMin + 0.01, this.fullTMax);
      if (this.cb.onViewportChanged) {
        this.cb.onViewportChanged(null, null, true, "reset");
      }
      this.scheduleRender();
    }

    scheduleRender() {
      if (this._rafPending) return;
      this._rafPending = true;
      requestAnimationFrame(() => {
        this._rafPending = false;
        this.renderAll();
      });
    }

    renderAll() {
      const col = themeColors();
      for (const [pid, pane] of this.panes.entries()) {
        this._renderSinglePane(pane, this.tracesByPlot.get(pid) || [], col);
      }
    }

    _drawLegendOnCanvas(ctx, pane, traces, col, w, h) {
      const maxRight = Math.max(PAD_L + 120, w - PAD_R);
      const maxHeaderH = Math.max(26, Math.floor(h * 0.34));
      let cx = PAD_L;
      let cy = 5;
      const chipH = 16;
      const rowStep = 19;

      ctx.font = '600 11px "Segoe UI Variable Text", "Microsoft YaHei UI", sans-serif';
      const titleStr = pane.title || "";
      const titleW = Math.ceil(ctx.measureText(titleStr).width) + 12;
      ctx.fillStyle = col.gridMinor;
      ctx.strokeStyle = col.border;
      ctx.lineWidth = 1;
      ctx.fillRect(cx + 0.5, cy + 0.5, titleW, chipH);
      ctx.strokeRect(cx + 0.5, cy + 0.5, titleW, chipH);
      ctx.fillStyle = col.textSec;
      ctx.textAlign = "left";
      ctx.textBaseline = "middle";
      ctx.fillText(titleStr, cx + 6, cy + chipH * 0.5 + 0.5);
      cx += titleW + 8;

      for (const tr of traces) {
        const g = Number.isFinite(tr.gain) ? tr.gain : 1.0;
        const gainTag = Math.abs(g - 1.0) > 1e-5 ? ` ×${Number(g.toFixed(3))}` : "";
        let valStr = "";
        if (this.hoverTime !== null && tr.t && tr.t.length > 0) {
          const idx = binaryNearestIdx(tr.t, this.hoverTime);
          const hv = tr.y[idx]; // True unscaled physical value!
          if (hv !== undefined && Number.isFinite(hv)) {
            valStr = ` ${Number(hv).toFixed(2)}`;
          }
        }
        const baseTxt = tr.label + gainTag;
        ctx.font = '11px "Segoe UI Variable Text", "Microsoft YaHei UI", sans-serif';
        const baseW = ctx.measureText(baseTxt).width;
        let valW = 0;
        if (valStr) {
          ctx.font = '600 11px "JetBrains Mono", "Cascadia Mono", Consolas, monospace';
          valW = ctx.measureText(valStr).width;
        }
        const itemW = Math.ceil(14 + baseW + valW + 8);
        if (cx + itemW > maxRight && cx > PAD_L + 4) {
          if (cy + rowStep + chipH > maxHeaderH) break;
          cx = PAD_L;
          cy += rowStep;
        }

        // Color swatch
        ctx.fillStyle = tr.color;
        ctx.fillRect(cx, cy + 7, 10, 2.5);

        // Channel label
        ctx.font = '11px "Segoe UI Variable Text", "Microsoft YaHei UI", sans-serif';
        ctx.fillStyle = col.text;
        ctx.fillText(baseTxt, cx + 14, cy + chipH * 0.5 + 0.5);

        if (valStr) {
          ctx.font = '600 11px "JetBrains Mono", "Cascadia Mono", Consolas, monospace';
          ctx.fillText(valStr, cx + 14 + baseW, cy + chipH * 0.5 + 0.5);
        }

        cx += itemW + 6;
      }

      return Math.max(PAD_T, cy + chipH + 5);
    }

    _renderSinglePane(pane, traces, col) {
      const { ctx, w, h } = fitCanvasHiDPI(pane.canvas);
      const pw = Math.max(10, w - PAD_L - PAD_R);

      ctx.fillStyle = col.canvas;
      ctx.fillRect(0, 0, w, h);

      // Bake legend directly onto canvas so right-click "Save image as" includes all names & colors!
      const topPad = this._drawLegendOnCanvas(ctx, pane, traces, col, w, h);
      const ph = Math.max(24, h - topPad - PAD_B);
      pane.lastTopPad = topPad;
      pane.lastPh = ph;

      const tMin = this.viewTMin;
      const tMax = Math.max(tMin + 1e-9, this.viewTMax);
      const [yMin, yMax] = pane.yCustom || pane.yAuto;
      const ySpan = Math.max(1e-9, yMax - yMin);

      const xOf = (t) => PAD_L + ((t - tMin) / (tMax - tMin)) * pw;
      const yOf = (y) => topPad + (1 - (y - yMin) / ySpan) * ph;
      const clampPy = (py) => Math.max(-4096, Math.min(h + 4096, py));

      // Gridlines & Axis Ticks
      const tStep = niceStep(tMax - tMin, Math.floor(pw / 95));
      const yStep = niceStep(ySpan, Math.floor(ph / 48));

      ctx.lineWidth = 1;
      ctx.font = '10.5px "JetBrains Mono", "Cascadia Mono", Consolas, monospace';

      // Vertical time grid
      const firstTX = Math.ceil(tMin / tStep) * tStep;
      ctx.strokeStyle = col.gridMajor;
      ctx.fillStyle = col.textSec;
      ctx.textAlign = "center";
      ctx.textBaseline = "top";
      for (let tv = firstTX; tv <= tMax + tStep * 0.001; tv += tStep) {
        const px = Math.round(xOf(tv)) + 0.5;
        if (px < PAD_L || px > PAD_L + pw) continue;
        ctx.beginPath();
        ctx.moveTo(px, topPad);
        ctx.lineTo(px, topPad + ph);
        ctx.stroke();
        ctx.fillText(fmtAxisNum(tv, tStep) + "s", px, topPad + ph + 5);
      }

      // Horizontal Y grid
      const firstTY = Math.ceil(yMin / yStep) * yStep;
      ctx.textAlign = "right";
      ctx.textBaseline = "middle";
      for (let yv = firstTY; yv <= yMax + yStep * 0.001; yv += yStep) {
        const py = Math.round(yOf(yv)) + 0.5;
        if (py < topPad || py > topPad + ph) continue;
        ctx.strokeStyle = Math.abs(yv) < yStep * 1e-4 ? col.border : col.gridMajor;
        ctx.beginPath();
        ctx.moveTo(PAD_L, py);
        ctx.lineTo(PAD_L + pw, py);
        ctx.stroke();
        ctx.fillText(fmtAxisNum(yv, yStep), PAD_L - 6, py);
      }

      // Plot frame border
      ctx.strokeStyle = col.border;
      ctx.strokeRect(PAD_L + 0.5, topPad + 0.5, pw, ph);

      // Clip to plot interior
      ctx.save();
      ctx.beginPath();
      ctx.rect(PAD_L, topPad, pw, ph);
      ctx.clip();

      // Shaded [T1, T2] Dual Cursor Region
      if (this.cursorEnabled) {
        const cx1 = xOf(this.cursorT1);
        const cx2 = xOf(this.cursorT2);
        const leftX = Math.min(cx1, cx2);
        const bandW = Math.abs(cx2 - cx1);
        ctx.fillStyle = col.band;
        ctx.fillRect(leftX, topPad, bandW, ph);
      }

      // Waveform Polyline Traces (scaled by display_gain ONLY on graph coordinates!)
      let hasAnyPoints = false;
      for (const tr of traces) {
        const tArr = tr.t;
        const yArr = tr.y;
        if (!tArr || tArr.length === 0) continue;
        const g = Number.isFinite(tr.gain) ? tr.gain : 1.0;
        hasAnyPoints = true;
        ctx.strokeStyle = tr.color;
        ctx.lineWidth = tr.width || 1.5;
        ctx.lineJoin = "bevel";
        ctx.beginPath();
        let penDown = false;
        for (let i = 0; i < tArr.length; i++) {
          const yv = yArr[i] * g;
          if (!Number.isFinite(yv)) {
            penDown = false;
            continue;
          }
          const px = xOf(tArr[i]);
          const py = clampPy(yOf(yv));
          if (!penDown) {
            ctx.moveTo(px, py);
            penDown = true;
          } else {
            ctx.lineTo(px, py);
          }
        }
        ctx.stroke();
      }

      if (!hasAnyPoints) {
        ctx.fillStyle = col.textSec;
        ctx.globalAlpha = 0.55;
        ctx.font = '12px "Segoe UI Variable Text", "Microsoft YaHei UI", sans-serif';
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillText("待机无波形 — 请连接串口或点击顶部「打开CSV」载入测试数据", PAD_L + pw * 0.5, topPad + ph * 0.5);
        ctx.globalAlpha = 1.0;
      }

      // Latest-Frame Anchor Frontier Line
      const showAnchorLine =
        (this.followEnabled && this.followAnchorRatio < 0.998) || this.anchorDragging;
      if (showAnchorLine) {
        const ax = Math.round(PAD_L + this.followAnchorRatio * pw) + 0.5;
        ctx.strokeStyle = "rgba(139, 92, 246, 0.72)";
        ctx.lineWidth = 1.2;
        ctx.setLineDash([4, 3]);
        ctx.beginPath();
        ctx.moveTo(ax, topPad);
        ctx.lineTo(ax, topPad + ph);
        ctx.stroke();
        ctx.setLineDash([]);
      }

      // Cursor lines T1 & T2
      if (this.cursorEnabled) {
        this._drawCursorLine(ctx, xOf(this.cursorT1), topPad, ph, col.t1, "T1");
        this._drawCursorLine(ctx, xOf(this.cursorT2), topPad, ph, col.t2, "T2");
      }

      // Synchronized Hover Crosshair
      if (this.hoverTime !== null && this.hoverTime >= tMin && this.hoverTime <= tMax) {
        const hx = Math.round(xOf(this.hoverTime)) + 0.5;
        ctx.strokeStyle = col.crosshair;
        ctx.lineWidth = 1;
        ctx.setLineDash([4, 4]);
        ctx.beginPath();
        ctx.moveTo(hx, topPad);
        ctx.lineTo(hx, topPad + ph);
        ctx.stroke();
        ctx.setLineDash([]);
      }

      ctx.restore();

      if (showAnchorLine) {
        const ax = Math.round(PAD_L + this.followAnchorRatio * pw) + 0.5;
        ctx.fillStyle = "#8B5CF6";
        ctx.beginPath();
        ctx.moveTo(ax, topPad + ph);
        ctx.lineTo(ax - 4.5, topPad + ph + 6);
        ctx.lineTo(ax + 4.5, topPad + ph + 6);
        ctx.closePath();
        ctx.fill();
      }
    }

    _drawCursorLine(ctx, px, top, ph, color, tag) {
      const rx = Math.round(px) + 0.5;
      ctx.strokeStyle = color;
      ctx.lineWidth = 1.3;
      ctx.setLineDash([5, 3]);
      ctx.beginPath();
      ctx.moveTo(rx, top);
      ctx.lineTo(rx, top + ph);
      ctx.stroke();
      ctx.setLineDash([]);

      // Clean handle tab at top
      ctx.fillStyle = color;
      ctx.fillRect(rx - 11, top, 22, 13);
      ctx.fillStyle = "#0B0C0E";
      ctx.font = 'bold 9.5px "JetBrains Mono", Consolas, monospace';
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(tag, rx, top + 6.5);
    }

    _bindEvents(plotId, canvas) {
      let dragMode = null; // "t1" | "t2" | "pan_xy" | "pan_x" | "pan_y"
      let didPanX = false;
      let startX = 0;
      let startY = 0;
      let startTMin = 0;
      let startTMax = 0;
      let startYRange = null;

      const timeAtX = (mx) => {
        const pw = Math.max(10, canvas.clientWidth - PAD_L - PAD_R);
        const ratio = (mx - PAD_L) / pw;
        return this.viewTMin + ratio * (this.viewTMax - this.viewTMin);
      };

      const pxOfTime = (t) => {
        const pw = Math.max(10, canvas.clientWidth - PAD_L - PAD_R);
        return PAD_L + ((t - this.viewTMin) / Math.max(1e-9, this.viewTMax - this.viewTMin)) * pw;
      };

      canvas.addEventListener("mousedown", (e) => {
        if (e.button !== 0) return;
        const rect = canvas.getBoundingClientRect();
        const mx = e.clientX - rect.left;
        const my = e.clientY - rect.top;
        const pane = this.panes.get(plotId);
        if (!pane) return;

        const topPad = pane.lastTopPad || PAD_T;
        const ph = pane.lastPh || Math.max(10, canvas.clientHeight - topPad - PAD_B);

        startX = mx;
        startY = my;
        startTMin = this.viewTMin;
        startTMax = this.viewTMax;
        startYRange = (pane.yCustom || pane.yAuto).slice();
        didPanX = false;

        if (mx < PAD_L) {
          dragMode = "pan_y";
          return;
        }
        if (my > topPad + ph) {
          dragMode = "pan_x";
          return;
        }

        if (this.cursorEnabled && my >= topPad && my <= topPad + ph) {
          const x1 = pxOfTime(this.cursorT1);
          const x2 = pxOfTime(this.cursorT2);
          if (Math.abs(mx - x1) <= 8) {
            dragMode = "t1";
            return;
          }
          if (Math.abs(mx - x2) <= 8) {
            dragMode = "t2";
            return;
          }
        }
        dragMode = "pan_xy";
      });

      window.addEventListener("mousemove", (e) => {
        const rect = canvas.getBoundingClientRect();
        const mx = e.clientX - rect.left;
        const my = e.clientY - rect.top;
        const inside = mx >= 0 && mx <= rect.width && my >= 0 && my <= rect.height;
        const pane = this.panes.get(plotId);
        const topPad = (pane && pane.lastTopPad) || PAD_T;
        const ph = (pane && pane.lastPh) || Math.max(10, canvas.clientHeight - topPad - PAD_B);

        if (!dragMode) {
          if (!inside) return;
          if (mx < PAD_L) {
            canvas.style.cursor = "ns-resize";
          } else if (my > topPad + ph) {
            canvas.style.cursor = "ew-resize";
          } else {
            if (this.cursorEnabled && my >= topPad) {
              const x1 = pxOfTime(this.cursorT1);
              const x2 = pxOfTime(this.cursorT2);
              canvas.style.cursor =
                Math.abs(mx - x1) <= 8 || Math.abs(mx - x2) <= 8 ? "ew-resize" : "crosshair";
            } else {
              canvas.style.cursor = "crosshair";
            }
            this.hoverTime = timeAtX(mx);
            if (this.cb.onHoverChanged) {
              this.cb.onHoverChanged(this.hoverTime);
            }
            this.scheduleRender();
          }
          return;
        }

        const pw = Math.max(10, canvas.clientWidth - PAD_L - PAD_R);

        if (dragMode === "t1") {
          this.cursorT1 = Math.max(0, timeAtX(mx));
          if (this.cb.onCursorsMoved) this.cb.onCursorsMoved(this.cursorT1, this.cursorT2, false);
          this.scheduleRender();
        } else if (dragMode === "t2") {
          this.cursorT2 = Math.max(0, timeAtX(mx));
          if (this.cb.onCursorsMoved) this.cb.onCursorsMoved(this.cursorT1, this.cursorT2, false);
          this.scheduleRender();
        } else if (dragMode === "pan_xy" || dragMode === "pan_x" || dragMode === "pan_y") {
          let movedAny = false;
          if ((dragMode === "pan_xy" || dragMode === "pan_y") && pane && startYRange) {
            if (Math.abs(my - startY) > 1) {
              const ySpan = startYRange[1] - startYRange[0];
              const dy = ((my - startY) / ph) * ySpan;
              pane.yCustom = [startYRange[0] + dy, startYRange[1] + dy];
              movedAny = true;
            }
          }
          if (dragMode === "pan_xy" || dragMode === "pan_x") {
            if (didPanX || Math.abs(mx - startX) > 3) {
              didPanX = true;
              const span = startTMax - startTMin;
              const dt = -((mx - startX) / pw) * span;
              this.viewTMin = startTMin + dt;
              this.viewTMax = startTMax + dt;
              movedAny = true;
              if (this.cb.onViewportChanged) {
                this.cb.onViewportChanged(this.viewTMin, this.viewTMax, false, "pan");
              }
            }
          }
          if (movedAny) {
            canvas.style.cursor = "grabbing";
            this.scheduleRender();
          }
        }
      });

      window.addEventListener("mouseup", () => {
        if (dragMode === "t1" || dragMode === "t2") {
          if (this.cb.onCursorsMoved) this.cb.onCursorsMoved(this.cursorT1, this.cursorT2, true);
        } else if ((dragMode === "pan_xy" || dragMode === "pan_x") && didPanX) {
          if (this.cb.onViewportChanged) {
            this.cb.onViewportChanged(this.viewTMin, this.viewTMax, true, "pan");
          }
        }
        if (dragMode) {
          canvas.style.cursor = "";
        }
        dragMode = null;
        didPanX = false;
      });

      canvas.addEventListener("mouseleave", () => {
        if (!dragMode) {
          this.hoverTime = null;
          this.scheduleRender();
        }
      });

      canvas.addEventListener("dblclick", () => {
        this.resetZoomFull();
      });

      canvas.addEventListener(
        "wheel",
        (e) => {
          e.preventDefault();
          const rect = canvas.getBoundingClientRect();
          const mx = e.clientX - rect.left;
          const my = e.clientY - rect.top;
          const pane = this.panes.get(plotId);
          if (!pane) return;

          const topPad = pane.lastTopPad || PAD_T;
          const ph = pane.lastPh || Math.max(10, canvas.clientHeight - topPad - PAD_B);
          const factor = e.deltaY > 0 ? 1.18 : 1 / 1.18;

          // Zone rules:
          // - Left Y-axis ruler (mx < PAD_L) or Shift+Wheel => Zoom Y only
          // - Bottom X-axis ruler (my > topPad + ph)        => Zoom X only
          // - Inside waveform plot box                      => Zoom BOTH X and Y together!
          const onYRuler = mx < PAD_L || e.shiftKey;
          const onXRuler = !onYRuler && my > topPad + ph;
          const doZoomY = onYRuler || !onXRuler;
          const doZoomX = onXRuler || !onYRuler;

          if (doZoomY) {
            const [y0, y1] = pane.yCustom || pane.yAuto;
            const yRatio = 1 - Math.max(0, Math.min(1, (my - topPad) / ph));
            const yPivot = y0 + yRatio * (y1 - y0);
            pane.yCustom = [yPivot - (yPivot - y0) * factor, yPivot + (y1 - yPivot) * factor];
          }

          if (doZoomX) {
            const pw = Math.max(10, canvas.clientWidth - PAD_L - PAD_R);
            const xRatio = this.followEnabled
              ? Math.max(0.1, Math.min(1, this.followAnchorRatio))
              : Math.max(0, Math.min(1, (mx - PAD_L) / pw));
            const tPivot = this.viewTMin + xRatio * (this.viewTMax - this.viewTMin);
            const newMin = tPivot - (tPivot - this.viewTMin) * factor;
            const newMax = tPivot + (this.viewTMax - tPivot) * factor;
            if (newMax - newMin >= 0.0005) {
              this.viewTMin = newMin;
              this.viewTMax = newMax;
              if (this.cb.onViewportChanged) {
                this.cb.onViewportChanged(this.viewTMin, this.viewTMax, true, "zoom");
              }
            }
          }

          this.scheduleRender();
        },
        { passive: false }
      );
    }
  }

  /**
   * Standalone Precision Curve Plotter for Multi-Slice Overlay & FFT Spectrum
   */
  function renderStandalonePlot(canvas, curves, opts) {
    if (!canvas || canvas.clientWidth < 20 || canvas.clientHeight < 20) return;
    const col = themeColors();
    const { ctx, w, h } = fitCanvasHiDPI(canvas);
    const pw = Math.max(10, w - PAD_L - PAD_R);
    const ph = Math.max(10, h - PAD_T - PAD_B);

    ctx.fillStyle = col.canvas;
    ctx.fillRect(0, 0, w, h);

    let xMin = Infinity,
      xMax = -Infinity,
      yMin = Infinity,
      yMax = -Infinity;

    for (const c of curves || []) {
      const tx = c.t || [];
      const vy = c.y || [];
      for (let i = 0; i < tx.length; i++) {
        if (tx[i] < xMin) xMin = tx[i];
        if (tx[i] > xMax) xMax = tx[i];
        if (vy[i] < yMin) yMin = vy[i];
        if (vy[i] > yMax) yMax = vy[i];
      }
    }

    if (!isFinite(xMin) || !isFinite(xMax)) {
      xMin = 0;
      xMax = 10;
      yMin = 0;
      yMax = 1;
    } else {
      if (Math.abs(xMax - xMin) < 1e-6) xMax = xMin + 1;
      if (opts && opts.zeroBaseline) yMin = 0;
      if (Math.abs(yMax - yMin) < 1e-6) {
        yMin -= 1;
        yMax += 1;
      } else {
        const pad = (yMax - yMin) * 0.09;
        if (!(opts && opts.zeroBaseline)) yMin -= pad;
        yMax += pad;
      }
    }

    const xOf = (x) => PAD_L + ((x - xMin) / (xMax - xMin)) * pw;
    const yOf = (y) => PAD_T + (1 - (y - yMin) / (yMax - yMin)) * ph;

    const xStep = niceStep(xMax - xMin, Math.floor(pw / 95));
    const yStep = niceStep(yMax - yMin, Math.floor(ph / 45));
    const xUnit = (opts && opts.xUnit) || "s";

    ctx.font = '10.5px "JetBrains Mono", Consolas, monospace';
    ctx.lineWidth = 1;
    ctx.strokeStyle = col.gridMajor;
    ctx.fillStyle = col.textSec;

    ctx.textAlign = "center";
    ctx.textBaseline = "top";
    for (let xv = Math.ceil(xMin / xStep) * xStep; xv <= xMax + xStep * 0.001; xv += xStep) {
      const px = Math.round(xOf(xv)) + 0.5;
      if (px < PAD_L || px > PAD_L + pw) continue;
      ctx.beginPath();
      ctx.moveTo(px, PAD_T);
      ctx.lineTo(px, PAD_T + ph);
      ctx.stroke();
      ctx.fillText(fmtAxisNum(xv, xStep) + xUnit, px, PAD_T + ph + 5);
    }

    ctx.textAlign = "right";
    ctx.textBaseline = "middle";
    for (let yv = Math.ceil(yMin / yStep) * yStep; yv <= yMax + yStep * 0.001; yv += yStep) {
      const py = Math.round(yOf(yv)) + 0.5;
      if (py < PAD_T || py > PAD_T + ph) continue;
      ctx.beginPath();
      ctx.moveTo(PAD_L, py);
      ctx.lineTo(PAD_L + pw, py);
      ctx.stroke();
      ctx.fillText(fmtAxisNum(yv, yStep), PAD_L - 6, py);
    }

    ctx.strokeStyle = col.border;
    ctx.strokeRect(PAD_L + 0.5, PAD_T + 0.5, pw, ph);

    ctx.save();
    ctx.beginPath();
    ctx.rect(PAD_L, PAD_T, pw, ph);
    ctx.clip();

    for (const c of curves || []) {
      const tx = c.t || [];
      const vy = c.y || [];
      if (!tx.length) continue;
      ctx.strokeStyle = c.color || "#0EA5E9";
      ctx.lineWidth = c.width || 1.7;
      if (c.line_style === "dash") ctx.setLineDash([7, 4]);
      else if (c.line_style === "dot") ctx.setLineDash([2, 3]);
      else ctx.setLineDash([]);

      ctx.beginPath();
      for (let i = 0; i < tx.length; i++) {
        const px = xOf(tx[i]);
        const py = yOf(vy[i]);
        if (i === 0) ctx.moveTo(px, py);
        else ctx.lineTo(px, py);
      }
      ctx.stroke();

      if (opts && opts.fillArea) {
        ctx.lineTo(xOf(tx[tx.length - 1]), PAD_T + ph);
        ctx.lineTo(xOf(tx[0]), PAD_T + ph);
        ctx.closePath();
        ctx.fillStyle = "rgba(59, 130, 246, 0.14)";
        ctx.fill();
      }
    }
    ctx.restore();
  }

  /**
   * 1:1 Aspect-Locked X-Y Vector Lissajous Renderer
   */
  function renderXYScope(canvas, xArr, yArr) {
    if (!canvas || canvas.clientWidth < 20 || canvas.clientHeight < 20) return;
    const col = themeColors();
    const { ctx, w, h } = fitCanvasHiDPI(canvas);
    ctx.fillStyle = col.canvas;
    ctx.fillRect(0, 0, w, h);

    const pad = 18;
    const side = Math.max(20, Math.min(w, h) - pad * 2);
    const ox = (w - side) * 0.5;
    const oy = (h - side) * 0.5;

    let maxAbs = 1;
    const n = Math.min((xArr || []).length, (yArr || []).length);
    for (let i = 0; i < n; i++) {
      maxAbs = Math.max(maxAbs, Math.abs(xArr[i]), Math.abs(yArr[i]));
    }
    maxAbs *= 1.12;

    const xOf = (v) => ox + ((v + maxAbs) / (2 * maxAbs)) * side;
    const yOf = (v) => oy + (1 - (v + maxAbs) / (2 * maxAbs)) * side;

    // Crosshair axes + reference circle
    ctx.strokeStyle = col.gridMajor;
    ctx.lineWidth = 1;
    ctx.strokeRect(ox + 0.5, oy + 0.5, side, side);
    ctx.beginPath();
    ctx.moveTo(ox, oy + side * 0.5);
    ctx.lineTo(ox + side, oy + side * 0.5);
    ctx.moveTo(ox + side * 0.5, oy);
    ctx.lineTo(ox + side * 0.5, oy + side);
    ctx.stroke();

    ctx.beginPath();
    ctx.arc(ox + side * 0.5, oy + side * 0.5, side * 0.42, 0, Math.PI * 2);
    ctx.setLineDash([3, 3]);
    ctx.stroke();
    ctx.setLineDash([]);

    if (n > 1) {
      ctx.strokeStyle = "#0EA5E9";
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      for (let i = 0; i < n; i++) {
        const px = xOf(xArr[i]);
        const py = yOf(yArr[i]);
        if (i === 0) ctx.moveTo(px, py);
        else ctx.lineTo(px, py);
      }
      ctx.stroke();

      // Current vector tip dot
      ctx.fillStyle = "#F59E0B";
      ctx.beginPath();
      ctx.arc(xOf(xArr[n - 1]), yOf(yArr[n - 1]), 3.5, 0, Math.PI * 2);
      ctx.fill();
    }
  }

  function escapeHtml(s) {
    return String(s)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");
  }

  window.OscEngine = {
    MultiScopeGroup,
    renderStandalonePlot,
    renderXYScope,
  };
})();
