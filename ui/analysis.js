/* FluxScope v2.0 — Static Post-Analysis Workbench Controller (6-Tab Functional Mode) */

(function () {
  function esc(s) {
    return String(s ?? "")
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  function themePalette() {
    const cs = getComputedStyle(document.documentElement);
    const g = (k, fb) => cs.getPropertyValue(k).trim() || fb;
    return {
      canvas: g("--bg-canvas", "#141416"),
      panel: g("--bg-panel", "#202125"),
      card: g("--bg-card", "#1A1B1E"),
      gridMajor: g("--bg-grid-major", "#25262B"),
      gridMinor: g("--bg-grid-minor", "#1B1C20"),
      border: g("--border", "#2D2F36"),
      text: g("--text-primary", "#E6E7EB"),
      textSec: g("--text-secondary", "#989CA6"),
      muted: g("--text-muted", "#666A73"),
      accent: g("--accent", "#3B82F6"),
      ok: g("--ok", "#10B981"),
      warn: g("--warn", "#F59E0B"),
      danger: g("--danger", "#EF4444"),
      crosshair: g("--crosshair", "rgba(230,231,235,0.32)"),
    };
  }

  function niceStep(span, targetTicks) {
    if (span <= 0 || !isFinite(span)) return 1;
    const raw = span / Math.max(1, targetTicks);
    const mag = Math.pow(10, Math.floor(Math.log10(raw)));
    const norm = raw / mag;
    const mult = norm >= 5 ? 5 : norm >= 2 ? 2 : 1;
    return mult * mag;
  }

  function fmtNum(v, step) {
    if (Math.abs(v) < 1e-11) return "0";
    if (Math.abs(v) >= 1e5 || (Math.abs(v) > 0 && Math.abs(v) < 1e-3)) {
      return v.toExponential(1);
    }
    const dec = step < 0.01 ? 4 : step < 0.1 ? 3 : step < 1 ? 2 : step < 10 ? 1 : 0;
    return Number(v.toFixed(dec)).toString();
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

  // ==================== SIGNAL MATH & ALGORITHMS ====================

  function analyzeStepResponse(run, fbkKey, refKey, bandFrac) {
    const fbk = getCh(run, fbkKey);
    const ref = getCh(run, refKey);
    if (!fbk || !fbk.data || fbk.data.length < 16) return null;
    const y = fbk.data;
    const t = run.t;
    const n = y.length;

    const n0 = Math.max(3, Math.floor(n * 0.06));
    let y0 = 0;
    for (let i = 0; i < n0; i++) y0 += y[i];
    y0 /= n0;

    const sStart = Math.floor(n * 0.8);
    let yInf = 0;
    let ssMin = Infinity;
    let ssMax = -Infinity;
    for (let i = sStart; i < n; i++) {
      yInf += y[i];
      if (y[i] < ssMin) ssMin = y[i];
      if (y[i] > ssMax) ssMax = y[i];
    }
    yInf /= Math.max(1, n - sStart);

    const stepAmp = yInf - y0;
    if (Math.abs(stepAmp) < 1e-4) {
      return { valid: false, y0, yInf };
    }
    const sign = stepAmp >= 0 ? 1 : -1;

    let t0 = t[0];
    if (ref && ref.key !== fbk.key && ref.data && ref.data.length === n) {
      const ry = ref.data;
      const rMid = (ry[0] + ry[n - 1]) * 0.5;
      if (Math.abs(ry[n - 1] - ry[0]) > 1e-4) {
        for (let i = 1; i < n; i++) {
          if ((ry[i - 1] - rMid) * (ry[i] - rMid) <= 0 && ry[i] !== ry[i - 1]) {
            t0 = t[i];
            break;
          }
        }
      }
    } else {
      const th03 = y0 + 0.03 * stepAmp;
      for (let i = 1; i < n; i++) {
        if (sign * (y[i] - th03) >= 0) {
          t0 = t[i];
          break;
        }
      }
    }

    const y10 = y0 + 0.1 * stepAmp;
    const y90 = y0 + 0.9 * stepAmp;
    let t10 = null;
    let t90 = null;
    let peakVal = y0;
    let tPeak = t0;

    for (let i = 0; i < n; i++) {
      if (t[i] < t0) continue;
      if (t10 === null && sign * (y[i] - y10) >= 0) t10 = t[i];
      if (t90 === null && sign * (y[i] - y90) >= 0) t90 = t[i];
      if (sign * (y[i] - peakVal) > 0) {
        peakVal = y[i];
        tPeak = t[i];
      }
    }

    const riseMs = t10 !== null && t90 !== null ? Math.max(0, (t90 - t10) * 1000) : 0;
    const peakMs = Math.max(0, (tPeak - t0) * 1000);
    const overshootPct = Math.max(0, ((peakVal - yInf) / stepAmp) * 100);

    const tol = Math.abs(stepAmp) * bandFrac;
    let lastOutIdx = 0;
    for (let i = 0; i < n; i++) {
      if (Math.abs(y[i] - yInf) > tol) lastOutIdx = i;
    }
    const settleMs =
      lastOutIdx < n - 3 ? Math.max(0, (t[lastOutIdx] - t0) * 1000) : (t[n - 1] - t0) * 1000;

    let sqSum = 0;
    const hasRef = ref && ref.data && ref.data.length === n;
    for (let i = 0; i < n; i++) {
      const target = hasRef ? ref.data[i] : yInf;
      const d = y[i] - target;
      sqSum += d * d;
    }
    const rmse = Math.sqrt(sqSum / n);

    return {
      valid: true,
      y0,
      yInf,
      t0,
      t10,
      t90,
      tPeak,
      peakVal,
      tol,
      tSettle: t0 + settleMs / 1000,
      riseMs,
      peakMs,
      overshootPct,
      settleMs,
      ripplePp: ssMax - ssMin,
      rmse,
    };
  }

  function computeFftSpectrum(samples, hz, winType) {
    let p2 = 1;
    while (p2 * 2 <= samples.length && p2 < 8192) p2 *= 2;
    if (p2 < 32) return { freqs: [], amps: [], dc: 0, thdPct: 0, peaks: [] };

    const start = samples.length - p2;
    let mean = 0;
    for (let i = 0; i < p2; i++) mean += samples[start + i];
    mean /= p2;

    const re = new Float64Array(p2);
    const im = new Float64Array(p2);
    let winSum = 0;
    for (let i = 0; i < p2; i++) {
      let w = 1.0;
      const phase = (2 * Math.PI * i) / (p2 - 1);
      if (winType === "hann") {
        w = 0.5 * (1 - Math.cos(phase));
      } else if (winType === "flattop") {
        w =
          0.21557895 -
          0.41663158 * Math.cos(phase) +
          0.277263158 * Math.cos(2 * phase) -
          0.083578947 * Math.cos(3 * phase) +
          0.006947368 * Math.cos(4 * phase);
      }
      re[i] = (samples[start + i] - mean) * w;
      winSum += w;
    }

    let j = 0;
    for (let i = 1; i < p2; i++) {
      let bit = p2 >> 1;
      while (j & bit) {
        j ^= bit;
        bit >>= 1;
      }
      j ^= bit;
      if (i < j) {
        const tr = re[i];
        re[i] = re[j];
        re[j] = tr;
      }
    }

    for (let len = 2; len <= p2; len <<= 1) {
      const ang = (-2 * Math.PI) / len;
      const wlenR = Math.cos(ang);
      const wlenI = Math.sin(ang);
      for (let i = 0; i < p2; i += len) {
        let wr = 1;
        let wi = 0;
        const halfLen = len >> 1;
        for (let k = 0; k < halfLen; k++) {
          const uR = re[i + k];
          const uI = im[i + k];
          const vR = re[i + k + halfLen] * wr - im[i + k + halfLen] * wi;
          const vI = re[i + k + halfLen] * wi + im[i + k + halfLen] * wr;
          re[i + k] = uR + vR;
          im[i + k] = uI + vI;
          re[i + k + halfLen] = uR - vR;
          im[i + k + halfLen] = uI - vI;
          const nxtR = wr * wlenR - wi * wlenI;
          wi = wr * wlenI + wi * wlenR;
          wr = nxtR;
        }
      }
    }

    const half = p2 >> 1;
    const freqs = new Float64Array(half);
    const amps = new Float32Array(half);
    const scale = 2.0 / Math.max(1e-6, winSum);
    for (let k = 0; k < half; k++) {
      freqs[k] = (k * hz) / p2;
      amps[k] = k === 0 ? 0 : Math.hypot(re[k], im[k]) * scale;
    }

    const peaks = [];
    for (let k = 2; k < half - 2; k++) {
      if (amps[k] > amps[k - 1] && amps[k] >= amps[k + 1] && amps[k] > 1e-5) {
        peaks.push({ bin: k, freq: freqs[k], amp: amps[k] });
      }
    }
    peaks.sort((a, b) => b.amp - a.amp);

    let thdPct = 0;
    if (peaks.length > 0 && peaks[0].amp > 1e-5) {
      const f1 = peaks[0].freq;
      const a1 = peaks[0].amp;
      let harmSq = 0;
      for (let h = 2; h <= 15; h++) {
        const targetBin = Math.round((f1 * h * p2) / hz);
        if (targetBin >= half - 1) break;
        let bestA = 0;
        for (let b = Math.max(1, targetBin - 1); b <= Math.min(half - 1, targetBin + 1); b++) {
          if (amps[b] > bestA) bestA = amps[b];
        }
        harmSq += bestA * bestA;
      }
      thdPct = (Math.sqrt(harmSq) / a1) * 100;
    }

    return { freqs, amps, dc: mean, thdPct, peaks: peaks.slice(0, 8) };
  }

  function runDigitalFilter(raw, hz, type, paramVal) {
    const n = raw.length;
    const out = new Float32Array(n);
    if (n === 0) return { out, cCode: "", delayMs: 0, attenPct: 0, rawPp: 0, filtPp: 0 };

    let cCode = "";
    let delayMs = 0;

    if (type === "iir1") {
      const fc = Math.max(0.5, paramVal);
      const rc = 1 / (2 * Math.PI * fc);
      const dt = 1 / hz;
      const alpha = dt / (rc + dt);
      let y = raw[0];
      for (let i = 0; i < n; i++) {
        y += alpha * (raw[i] - y);
        out[i] = y;
      }
      delayMs = rc * 1000;
      cCode = `// 一阶低通 IIR (fs=${hz}Hz, fc=${fc.toFixed(1)}Hz)\nconst float lpf_alpha = ${alpha.toFixed(6)}f;\ny_filt += lpf_alpha * (x_raw - y_filt);`;
    } else if (type === "biquad_lpf") {
      const fc = Math.max(1, Math.min(hz * 0.45, paramVal));
      const ita = 1.0 / Math.tan((Math.PI * fc) / hz);
      const q = Math.SQRT2;
      const b0 = 1.0 / (1.0 + q * ita + ita * ita);
      const b1 = 2 * b0;
      const b2 = b0;
      const a1 = 2.0 * (ita * ita - 1.0) * b0;
      const a2 = -(1.0 - q * ita + ita * ita) * b0;
      let x1 = raw[0];
      let x2 = raw[0];
      let y1 = raw[0];
      let y2 = raw[0];
      for (let i = 0; i < n; i++) {
        const x0 = raw[i];
        const y0 = b0 * x0 + b1 * x1 + b2 * x2 + a1 * y1 + a2 * y2;
        out[i] = y0;
        x2 = x1;
        x1 = x0;
        y2 = y1;
        y1 = y0;
      }
      delayMs = (1000 * 0.225) / fc;
      cCode = `// 二阶巴特沃斯低通 (fs=${hz}Hz, fc=${fc.toFixed(1)}Hz)\ny0 = ${b0.toFixed(5)}f*x0 + ${b1.toFixed(5)}f*x1 + ${b2.toFixed(5)}f*x2\n   + (${a1.toFixed(5)}f)*y1 + (${a2.toFixed(5)}f)*y2;`;
    } else if (type === "movavg") {
      const win = Math.max(2, Math.round(paramVal));
      let sum = 0;
      for (let i = 0; i < n; i++) {
        sum += raw[i];
        if (i >= win) sum -= raw[i - win];
        out[i] = sum / Math.min(i + 1, win);
      }
      delayMs = ((win - 1) * 0.5 * 1000) / hz;
      cCode = `// ${win}点环形滑动均值滤波 (fs=${hz}Hz)\nsum += x_new - ring_buf[idx];\nring_buf[idx] = x_new; idx = (idx + 1) % ${win};\ny_filt = sum * ${(1 / win).toFixed(6)}f;`;
    } else if (type === "notch") {
      const f0 = Math.max(2, Math.min(hz * 0.45, paramVal));
      const Q = 6.0;
      const w0 = (2 * Math.PI * f0) / hz;
      const alpha = Math.sin(w0) / (2 * Q);
      const a0 = 1 + alpha;
      const b0 = 1 / a0;
      const b1 = (-2 * Math.cos(w0)) / a0;
      const b2 = 1 / a0;
      const a1 = (-2 * Math.cos(w0)) / a0;
      const a2 = (1 - alpha) / a0;
      let x1 = raw[0];
      let x2 = raw[0];
      let y1 = raw[0];
      let y2 = raw[0];
      for (let i = 0; i < n; i++) {
        const x0 = raw[i];
        const y0 = b0 * x0 + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
        out[i] = y0;
        x2 = x1;
        x1 = x0;
        y2 = y1;
        y1 = y0;
      }
      delayMs = (1000 / (Math.PI * f0 * Q)) * 0.5;
      cCode = `// 二阶陷波器 Notch (f0=${f0.toFixed(1)}Hz, Q=${Q})\ny0 = ${b0.toFixed(5)}f*x0 + (${b1.toFixed(5)}f)*x1 + ${b2.toFixed(5)}f*x2\n   - (${a1.toFixed(5)}f)*y1 - (${a2.toFixed(5)}f)*y2;`;
    } else {
      const fc = Math.max(0.5, paramVal);
      const rc = 1 / (2 * Math.PI * fc);
      const alpha = (1 / hz) / (rc + 1 / hz);
      const tmp = new Float32Array(n);
      let y = raw[0];
      for (let i = 0; i < n; i++) {
        y += alpha * (raw[i] - y);
        tmp[i] = y;
      }
      y = tmp[n - 1];
      for (let i = n - 1; i >= 0; i--) {
        y += alpha * (tmp[i] - y);
        out[i] = y;
      }
      delayMs = 0;
      cCode = `// 零相位双向滤波 (FiltFilt 离线专用，无相位滞后)\n// 仅用于上位机事后提取真值基准曲线`;
    }

    const s0 = Math.floor(n * 0.7);
    let rMin = Infinity;
    let rMax = -Infinity;
    let fMin = Infinity;
    let fMax = -Infinity;
    for (let i = s0; i < n; i++) {
      if (raw[i] < rMin) rMin = raw[i];
      if (raw[i] > rMax) rMax = raw[i];
      if (out[i] < fMin) fMin = out[i];
      if (out[i] > fMax) fMax = out[i];
    }
    const rawPp = isFinite(rMax - rMin) ? Math.max(1e-6, rMax - rMin) : 1e-6;
    const filtPp = isFinite(fMax - fMin) ? fMax - fMin : 0;
    const attenPct = Math.max(0, (1 - filtPp / rawPp) * 100);

    return { out, cCode, delayMs, attenPct, rawPp, filtPp };
  }

  function getCh(run, key) {
    if (!run || !Array.isArray(run.channels) || run.channels.length === 0) return null;
    return run.channels.find((c) => c.key === key) || run.channels[0];
  }

  // ==================== INTERACTIVE 2D SCIENTIFIC CHART ENGINE ====================

  function attachInteractiveChart(canvas, getChartState, onUserViewport, onShiftDragCurve) {
    let dragMode = null; // "pan_xy" | "pan_x" | "pan_y" | "shift_curve"
    let sx = 0;
    let sy = 0;
    let startBounds = null;

    canvas.addEventListener("mousedown", (e) => {
      if (e.button !== 0 || !canvas._geom) return;
      const g = canvas._geom;
      const rect = canvas.getBoundingClientRect();
      const mx = e.clientX - rect.left;
      const my = e.clientY - rect.top;
      sx = e.clientX;
      sy = e.clientY;
      startBounds = [g.xMin, g.xMax, g.yMin, g.yMax];

      if (e.shiftKey && onShiftDragCurve) {
        dragMode = "shift_curve";
        onShiftDragCurve("start", 0, 0);
      } else if (mx < g.padL) {
        dragMode = "pan_y";
      } else if (my > g.padT + g.ph) {
        dragMode = "pan_x";
      } else {
        dragMode = "pan_xy";
      }
    });

    window.addEventListener("mousemove", (e) => {
      const g = canvas._geom;
      if (!g) return;
      const rect = canvas.getBoundingClientRect();
      const mx = e.clientX - rect.left;
      const my = e.clientY - rect.top;
      const inside = mx >= 0 && mx <= rect.width && my >= 0 && my <= rect.height;

      if (!dragMode) {
        if (!inside) return;
        if (mx < g.padL) {
          canvas.style.cursor = "ns-resize";
        } else if (my > g.padT + g.ph) {
          canvas.style.cursor = "ew-resize";
        } else {
          canvas.style.cursor = "crosshair";
          const st = getChartState();
          st.hoverX = g.xMin + ((mx - g.padL) / g.pw) * (g.xMax - g.xMin);
          onUserViewport(false);
        }
        return;
      }

      const dxVal = ((e.clientX - sx) / g.pw) * (startBounds[1] - startBounds[0]);
      const dyVal = -((e.clientY - sy) / g.ph) * (startBounds[3] - startBounds[2]);

      if (dragMode === "shift_curve" && onShiftDragCurve) {
        canvas.style.cursor = "grabbing";
        onShiftDragCurve("move", dxVal, dyVal);
        return;
      }

      const st = getChartState();
      let [x0, x1, y0, y1] = startBounds;
      if (dragMode === "pan_xy" || dragMode === "pan_x") {
        x0 -= dxVal;
        x1 -= dxVal;
      }
      if (dragMode === "pan_xy" || dragMode === "pan_y") {
        y0 -= dyVal;
        y1 -= dyVal;
      }
      canvas.style.cursor = "grabbing";
      st.bounds = [x0, x1, y0, y1];
      onUserViewport(false);
    });

    window.addEventListener("mouseup", () => {
      if (!dragMode) return;
      if (dragMode === "shift_curve" && onShiftDragCurve) {
        onShiftDragCurve("end", 0, 0);
      }
      canvas.style.cursor = "";
      dragMode = null;
    });

    canvas.addEventListener("mouseleave", () => {
      if (!dragMode) {
        const st = getChartState();
        if (st.hoverX !== null) {
          st.hoverX = null;
          onUserViewport(false);
        }
      }
    });

    canvas.addEventListener("dblclick", () => {
      const st = getChartState();
      st.bounds = null;
      onUserViewport(true);
    });

    canvas.addEventListener(
      "wheel",
      (e) => {
        e.preventDefault();
        const g = canvas._geom;
        if (!g) return;
        const rect = canvas.getBoundingClientRect();
        const mx = e.clientX - rect.left;
        const my = e.clientY - rect.top;
        const factor = e.deltaY > 0 ? 1.18 : 1 / 1.18;

        const onY = mx < g.padL || e.shiftKey;
        const onX = !onY && my > g.padT + g.ph;
        let [x0, x1, y0, y1] = [g.xMin, g.xMax, g.yMin, g.yMax];

        if (!onY) {
          const xr = Math.max(0, Math.min(1, (mx - g.padL) / g.pw));
          const xp = x0 + xr * (x1 - x0);
          x0 = xp - (xp - x0) * factor;
          x1 = xp + (x1 - xp) * factor;
        }
        if (!onX) {
          const yr = 1 - Math.max(0, Math.min(1, (my - g.padT) / g.ph));
          const yp = y0 + yr * (y1 - y0);
          y0 = yp - (yp - y0) * factor;
          y1 = yp + (y1 - yp) * factor;
        }
        const st = getChartState();
        st.bounds = [x0, x1, y0, y1];
        onUserViewport(false);
      },
      { passive: false }
    );
  }

  function renderScientificCanvas(canvas, title, seriesList, chartSt, opts) {
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const w = Math.max(120, canvas.clientWidth || 640);
    const h = Math.max(120, canvas.clientHeight || 320);
    if (canvas.width !== Math.floor(w * dpr) || canvas.height !== Math.floor(h * dpr)) {
      canvas.width = Math.floor(w * dpr);
      canvas.height = Math.floor(h * dpr);
    }
    const ctx = canvas.getContext("2d", { alpha: false });
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const pal = themePalette();

    ctx.fillStyle = pal.canvas;
    ctx.fillRect(0, 0, w, h);

    const padL = 62;
    const padR = 16;
    const padB = 24;

    // Bake title & wrapped legend chips onto Canvas (so right-click Save Image / PNG export has full legend!)
    let lx = padL;
    let ly = 6;
    const chipH = 16;
    ctx.font = '600 11px "Segoe UI Variable Text", "Microsoft YaHei UI", sans-serif';
    if (title) {
      const tw = Math.ceil(ctx.measureText(title).width) + 12;
      ctx.fillStyle = pal.gridMinor;
      ctx.strokeStyle = pal.border;
      ctx.lineWidth = 1;
      ctx.fillRect(lx + 0.5, ly + 0.5, tw, chipH);
      ctx.strokeRect(lx + 0.5, ly + 0.5, tw, chipH);
      ctx.fillStyle = pal.textSec;
      ctx.textAlign = "left";
      ctx.textBaseline = "middle";
      ctx.fillText(title, lx + 6, ly + chipH * 0.5 + 0.5);
      lx += tw + 8;
    }

    for (const s of seriesList) {
      let valTag = "";
      if (chartSt && chartSt.hoverX !== null && s.x && s.x.length > 0 && !opts?.isXY) {
        const idx = binaryNearestIdx(s.x, chartSt.hoverX);
        const hv = s.y[idx];
        if (hv !== undefined && isFinite(hv)) {
          valTag = ` ${Number(hv).toFixed(2)}`;
        }
      }
      ctx.font = '11px "Segoe UI Variable Text", "Microsoft YaHei UI", sans-serif';
      const baseW = ctx.measureText(s.label).width;
      let valW = 0;
      if (valTag) {
        ctx.font = '600 11px "JetBrains Mono", Consolas, monospace';
        valW = ctx.measureText(valTag).width;
      }
      const itemW = Math.ceil(16 + baseW + valW + 8);
      if (lx + itemW > w - padR && lx > padL + 4) {
        if (ly + 19 + chipH <= Math.floor(h * 0.32)) {
          lx = padL;
          ly += 19;
        } else {
          break;
        }
      }

      ctx.strokeStyle = s.color;
      ctx.lineWidth = 2.2;
      if (s.dash) ctx.setLineDash(s.dash);
      ctx.beginPath();
      ctx.moveTo(lx, ly + 8);
      ctx.lineTo(lx + 12, ly + 8);
      ctx.stroke();
      ctx.setLineDash([]);

      ctx.font = '11px "Segoe UI Variable Text", "Microsoft YaHei UI", sans-serif';
      ctx.fillStyle = pal.text;
      ctx.textAlign = "left";
      ctx.textBaseline = "middle";
      ctx.fillText(s.label, lx + 15, ly + chipH * 0.5 + 0.5);
      if (valTag) {
        ctx.font = '600 11px "JetBrains Mono", Consolas, monospace';
        ctx.fillText(valTag, lx + 15 + baseW, ly + chipH * 0.5 + 0.5);
      }
      lx += itemW + 6;
    }

    const padT = Math.max(28, ly + chipH + 6);
    const pw = Math.max(20, w - padL - padR);
    const ph = Math.max(24, h - padT - padB);

    let xMin = Infinity;
    let xMax = -Infinity;
    let yMin = Infinity;
    let yMax = -Infinity;
    for (const s of seriesList) {
      if (!s.x || !s.y) continue;
      for (let i = 0; i < s.x.length; i++) {
        const xv = s.x[i];
        const yv = s.y[i];
        if (!isFinite(xv) || !isFinite(yv)) continue;
        if (xv < xMin) xMin = xv;
        if (xv > xMax) xMax = xv;
        if (yv < yMin) yMin = yv;
        if (yv > yMax) yMax = yv;
      }
    }
    if (!isFinite(xMin) || !isFinite(yMin)) {
      xMin = 0;
      xMax = 1;
      yMin = -1;
      yMax = 1;
    }
    if (Math.abs(xMax - xMin) < 1e-6) {
      xMin -= 0.5;
      xMax += 0.5;
    }
    if (Math.abs(yMax - yMin) < 1e-6) {
      const p = Math.max(1, Math.abs(yMax) * 0.15);
      yMin -= p;
      yMax += p;
    } else {
      const p = (yMax - yMin) * 0.09;
      yMin -= p;
      yMax += p;
    }

    if (opts && opts.defaultBounds && (!chartSt || !chartSt.bounds)) {
      [xMin, xMax, yMin, yMax] = opts.defaultBounds(xMin, xMax, yMin, yMax, pw, ph);
    }
    if (chartSt && chartSt.bounds) {
      [xMin, xMax, yMin, yMax] = chartSt.bounds;
    }

    canvas._geom = { padL, padT, pw, ph, xMin, xMax, yMin, yMax };
    const xOf = (x) => padL + ((x - xMin) / Math.max(1e-12, xMax - xMin)) * pw;
    const yOf = (y) => padT + (1 - (y - yMin) / Math.max(1e-12, yMax - yMin)) * ph;

    // Gridlines
    const xStep = niceStep(xMax - xMin, Math.floor(pw / 95));
    const yStep = niceStep(yMax - yMin, Math.floor(ph / 45));
    ctx.font = '10.5px "JetBrains Mono", "Cascadia Mono", Consolas, monospace';
    ctx.lineWidth = 1;

    for (let xv = Math.ceil(xMin / xStep) * xStep; xv <= xMax + xStep * 1e-4; xv += xStep) {
      const px = Math.round(xOf(xv)) + 0.5;
      if (px < padL || px > padL + pw) continue;
      ctx.strokeStyle = Math.abs(xv) < xStep * 1e-4 ? pal.border : pal.gridMajor;
      ctx.beginPath();
      ctx.moveTo(px, padT);
      ctx.lineTo(px, padT + ph);
      ctx.stroke();
      ctx.fillStyle = pal.textSec;
      ctx.textAlign = "center";
      ctx.textBaseline = "top";
      ctx.fillText(fmtNum(xv, xStep) + (opts?.xUnit || ""), px, padT + ph + 5);
    }

    for (let yv = Math.ceil(yMin / yStep) * yStep; yv <= yMax + yStep * 1e-4; yv += yStep) {
      const py = Math.round(yOf(yv)) + 0.5;
      if (py < padT || py > padT + ph) continue;
      ctx.strokeStyle = Math.abs(yv) < yStep * 1e-4 ? pal.border : pal.gridMajor;
      ctx.beginPath();
      ctx.moveTo(padL, py);
      ctx.lineTo(padL + pw, py);
      ctx.stroke();
      ctx.fillStyle = pal.textSec;
      ctx.textAlign = "right";
      ctx.textBaseline = "middle";
      ctx.fillText(fmtNum(yv, yStep), padL - 6, py);
    }

    ctx.strokeStyle = pal.border;
    ctx.strokeRect(padL + 0.5, padT + 0.5, pw, ph);

    ctx.save();
    ctx.beginPath();
    ctx.rect(padL, padT, pw, ph);
    ctx.clip();

    if (opts && opts.onDrawUnderlay) {
      opts.onDrawUnderlay({ ctx, xOf, yOf, padL, padT, pw, ph, pal });
    }

    if (seriesList.length === 0) {
      ctx.fillStyle = pal.textSec;
      ctx.globalAlpha = 0.65;
      ctx.font = '12px "Segoe UI Variable Text", "Microsoft YaHei UI", sans-serif';
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(
        "暂无可分析实验波形 — 请先在『示波』打开 CSV/采集数据，或点击左侧『+ 导入CSV』",
        padL + pw * 0.5,
        padT + ph * 0.5
      );
      ctx.globalAlpha = 1.0;
    }

    for (const s of seriesList) {
      if (!s.x || s.x.length === 0) continue;
      ctx.strokeStyle = s.color;
      ctx.globalAlpha = s.alpha ?? 1.0;
      ctx.lineWidth = s.width || 1.75;
      ctx.lineJoin = "bevel";
      if (s.dash) ctx.setLineDash(s.dash);
      ctx.beginPath();
      let pen = false;
      for (let i = 0; i < s.x.length; i++) {
        const xv = s.x[i];
        const yv = s.y[i];
        if (!isFinite(xv) || !isFinite(yv)) {
          pen = false;
          continue;
        }
        const px = xOf(xv);
        const py = Math.max(-4096, Math.min(h + 4096, yOf(yv)));
        if (!pen) {
          ctx.moveTo(px, py);
          pen = true;
        } else {
          ctx.lineTo(px, py);
        }
      }
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.globalAlpha = 1.0;
    }

    if (opts && opts.onDrawOverlay) {
      opts.onDrawOverlay({ ctx, xOf, yOf, padL, padT, pw, ph, pal });
    }

    if (chartSt && chartSt.hoverX !== null && !opts?.isXY && chartSt.hoverX >= xMin && chartSt.hoverX <= xMax) {
      const hx = Math.round(xOf(chartSt.hoverX)) + 0.5;
      ctx.strokeStyle = pal.crosshair;
      ctx.lineWidth = 1;
      ctx.setLineDash([4, 4]);
      ctx.beginPath();
      ctx.moveTo(hx, padT);
      ctx.lineTo(hx, padT + ph);
      ctx.stroke();
      ctx.setLineDash([]);
    }

    ctx.restore();
  }

  // ==================== MAIN ANALYSIS CONTROLLER ====================

  function createAnalysisController(hooks) {
    // hooks: { invoke, showToast, getCursorRange }
    const invoke = hooks.invoke;
    const showToast = hooks.showToast;

    const st = {
      activeTab: "overlay", // "overlay" | "step" | "fft" | "filter" | "xy" | "stats"
      slicesMeta: [],
      liveInfo: { available: false, frames: 0, sample_rate_hz: 1000, duration_s: 0, source_label: "", data_seq: 0 },
      liveChecked: true,
      liveManualDt: 0,
      liveManualDy: 0,
      focusedId: null,
      bundleCache: new Map(), // sliceId -> parsed Run bundle (with Float64Array / Float32Array)
      cachedLiveSeq: -1,

      overlayScopeMode: "multi_run", // "multi_run" | "multi_ch"
      overlayCh: "speed_est",
      multiChSelected: [],
      normMode: "raw",
      alignMode: "rising50",
      alignThresh: 500,
      showDiff: false,

      stepFbkCh: "speed_est",
      stepRefCh: "speed_ref",
      settleBand: 0.02,

      fftCh: "Ia",
      fftWin: "hann",
      fftScale: "linear",
      polePairs: 7,

      filterCh: "speed_est",
      filterType: "iir1",
      filterCutoff: 25,

      xyX: "Vd",
      xyY: "Vq",
      xyEqualAspect: true,

      charts: {
        overlay: { bounds: null, hoverX: null },
        step: { bounds: null, hoverX: null },
        fft: { bounds: null, hoverX: null },
        filter: { bounds: null, hoverX: null },
        xy: { bounds: null, hoverX: null },
      },
    };

    function allRunEntries() {
      const list = [];
      if (st.liveInfo && st.liveInfo.available) {
        list.push({
          id: "__live_buffer__",
          isLive: true,
          name: "当前示波缓冲 (实时快照)",
          note: st.liveInfo.source_label || "示波器内存数据",
          color: "#0EA5E9",
          enabled: st.slicesMeta.length === 0 ? true : st.liveChecked,
          sample_rate_hz: st.liveInfo.sample_rate_hz || 1000,
          sample_count: st.liveInfo.frames || 0,
          offset_x: st.liveManualDt,
          offset_y: st.liveManualDy,
        });
      }
      for (const m of st.slicesMeta) {
        list.push({
          id: m.slice_id,
          isLive: false,
          name: m.name,
          note: m.note || "",
          color: m.color || "#3B82F6",
          enabled: !!m.enabled,
          sample_rate_hz: m.sample_rate_hz || 1000,
          sample_count: m.sample_count || 0,
          offset_x: m.offset_x || 0,
          offset_y: m.offset_y || 0,
        });
      }
      return list;
    }

    async function ensureBundleLoaded(id) {
      if (id === "__live_buffer__") {
        if (st.bundleCache.has("__live_buffer__") && st.cachedLiveSeq === st.liveInfo.data_seq) {
          return st.bundleCache.get("__live_buffer__");
        }
      } else if (st.bundleCache.has(id)) {
        return st.bundleCache.get(id);
      }

      try {
        const raw = await invoke("get_analysis_run_cmd", { sliceId: id, maxSamples: 16384 });
        const bundle = {
          id: raw.slice_id,
          name: raw.name,
          note: raw.note || "",
          color: raw.color || "#3B82F6",
          hz: raw.hz || 1000,
          rawCount: raw.raw_count || 0,
          t: Float64Array.from(raw.t || []),
          channels: (raw.channels || []).map((c) => ({
            key: c.key,
            name: c.name,
            unit: c.unit || "",
            color: c.color || "#3B82F6",
            data: Float32Array.from(c.data || []),
          })),
        };
        st.bundleCache.set(id, bundle);
        if (id === "__live_buffer__") st.cachedLiveSeq = st.liveInfo.data_seq;
        return bundle;
      } catch (_) {
        // Fallback if window was not restarted after binary upgrade
        try {
          if (id === "__live_buffer__") {
            const vp = await invoke("get_viewport", { pixelWidth: 4096 });
            const trs = vp.traces || [];
            if (trs.length > 0 && trs[0].t && trs[0].t.length > 0) {
              const bundle = {
                id: "__live_buffer__",
                name: "当前示波缓冲 (实时快照)",
                note: vp.source_label || "",
                color: "#0EA5E9",
                hz: vp.sample_rate_hz || 1000,
                rawCount: vp.buffered_frames || trs[0].t.length,
                t: Float64Array.from(trs[0].t),
                channels: trs.map((c) => ({
                  key: c.key,
                  name: c.name || c.key,
                  unit: c.unit || "",
                  color: c.color || "#3B82F6",
                  data: Float32Array.from(c.y || []),
                })),
              };
              st.bundleCache.set(id, bundle);
              st.cachedLiveSeq = st.liveInfo.data_seq;
              return bundle;
            }
          }
        } catch (_e2) {}
        return null;
      }
    }

    async function refreshAllData() {
      try {
        const ov = await invoke("get_analysis_overview_cmd");
        st.slicesMeta = ov.slices || [];
        st.liveInfo = ov.live_buffer || { available: false, frames: 0, data_seq: 0 };
      } catch (_) {
        try {
          const [ovOld, vp] = await Promise.all([
            invoke("get_overlay_data", { pixelWidth: 2048 }),
            invoke("get_viewport", { pixelWidth: 512 }),
          ]);
          st.slicesMeta = ovOld.slices || [];
          st.liveInfo = {
            available: (vp.buffered_frames || 0) > 4,
            frames: vp.buffered_frames || 0,
            sample_rate_hz: vp.sample_rate_hz || 1000,
            source_label: vp.source_label || "",
            data_seq: vp.data_seq || 0,
          };
        } catch (_e2) {
          st.slicesMeta = [];
          st.liveInfo = { available: false, frames: 0, data_seq: 0 };
        }
      }

      // Prune deleted slices from cache
      const validIds = new Set(st.slicesMeta.map((s) => s.slice_id));
      validIds.add("__live_buffer__");
      for (const k of Array.from(st.bundleCache.keys())) {
        if (!validIds.has(k)) st.bundleCache.delete(k);
      }

      const entries = allRunEntries();
      if (entries.length > 0) {
        if (!st.focusedId || !entries.some((e) => e.id === st.focusedId)) {
          // Prefer latest saved slice if present, else live buffer
          const nonLive = entries.filter((e) => !e.isLive);
          st.focusedId = nonLive.length > 0 ? nonLive[nonLive.length - 1].id : entries[0].id;
        }
      } else {
        st.focusedId = null;
      }

      // Ensure focused & checked bundles are loaded in memory
      const neededIds = new Set();
      if (st.focusedId) neededIds.add(st.focusedId);
      for (const e of entries) {
        if (e.enabled) neededIds.add(e.id);
      }
      await Promise.all(Array.from(neededIds).map((id) => ensureBundleLoaded(id)));

      // Sync latest meta fields onto cached bundles
      for (const e of entries) {
        const b = st.bundleCache.get(e.id);
        if (b) {
          b.name = e.name;
          b.note = e.note;
          b.color = e.color;
          b.manualDt = e.offset_x || 0;
          b.manualDy = e.offset_y || 0;
        }
      }

      renderSidebarList();
      populateChannelSelects();
      renderActiveTab();
    }

    function getFocusedBundle() {
      return (st.focusedId && st.bundleCache.get(st.focusedId)) || null;
    }

    function getCheckedBundles() {
      const entries = allRunEntries();
      const checked = entries
        .filter((e) => e.enabled)
        .map((e) => st.bundleCache.get(e.id))
        .filter(Boolean);
      if (checked.length > 0) return checked;
      const fb = getFocusedBundle();
      return fb ? [fb] : [];
    }

    function populateChannelSelects() {
      const b = getFocusedBundle();
      if (!b || !b.channels.length) return;
      const chs = b.channels;

      if (st.multiChSelected.length === 0) {
        st.multiChSelected = chs.slice(0, Math.min(4, chs.length)).map((c) => c.key);
      }

      const fill = (id, curKey, fallbackIdx) => {
        const sel = document.getElementById(id);
        if (!sel) return curKey;
        sel.innerHTML = chs
          .map(
            (c) =>
              `<option value="${esc(c.key)}">${esc(c.name)}${
                c.unit ? " (" + esc(c.unit) + ")" : ""
              }</option>`
          )
          .join("");
        if (chs.some((c) => c.key === curKey)) {
          sel.value = curKey;
          return curKey;
        }
        const pick = chs[Math.min(fallbackIdx || 0, chs.length - 1)].key;
        sel.value = pick;
        return pick;
      };

      st.overlayCh = fill("anlSelOverlayCh", st.overlayCh, 1);
      st.stepFbkCh = fill("anlSelStepFbk", st.stepFbkCh, 1);
      st.stepRefCh = fill("anlSelStepRef", st.stepRefCh, 0);
      st.fftCh = fill("anlSelFftCh", st.fftCh, Math.min(3, chs.length - 1));
      st.filterCh = fill("anlSelFilterCh", st.filterCh, 1);
      st.xyX = fill("anlSelXyX", st.xyX, Math.min(8, chs.length - 1));
      st.xyY = fill("anlSelXyY", st.xyY, Math.min(9, chs.length - 1));

      const mRow = document.getElementById("anlMultiChRow");
      if (mRow) {
        mRow.innerHTML =
          `<span class="tb-label" style="margin-right:4px">勾选叠放通道:</span>` +
          chs
            .map((c) => {
              const on = st.multiChSelected.includes(c.key);
              return `<label class="anl-ch-chip">
                <input type="checkbox" data-anl-mch="${esc(c.key)}" ${on ? "checked" : ""} />
                <span style="width:7px;height:7px;border-radius:50%;background:${esc(c.color)}"></span>
                <span>${esc(c.name)}</span>
              </label>`;
            })
            .join("");
      }
    }

    function renderSidebarList() {
      const entries = allRunEntries();
      const cntEl = document.getElementById("lblAnlRunCount");
      if (cntEl) cntEl.textContent = `${st.slicesMeta.length} 归档`;

      const host = document.getElementById("anlRunsList");
      if (!host) return;
      if (entries.length === 0) {
        host.innerHTML = `<div class="tb-label" style="padding:18px 10px;text-align:center;white-space:normal;line-height:1.6">
          当前暂无实验数据。<br/>可点击上方 <strong>+ 导入CSV</strong>，或在『示波』页载入数据后点击 <strong>+ 抓 T1~T2 / + 抓全缓冲</strong>。
        </div>`;
        return;
      }

      host.innerHTML = entries
        .map((r) => {
          const isFocus = r.id === st.focusedId;
          const dur =
            r.sample_rate_hz > 0 ? (r.sample_count / r.sample_rate_hz).toFixed(2) : "0.00";
          const dtMs = Math.round((r.offset_x || 0) * 1000);
          return `
          <div class="anl-run-card ${isFocus ? "focused" : ""}" data-run-focus="${esc(r.id)}">
            <div style="display:flex;align-items:center;justify-content:space-between;gap:5px">
              <div style="display:flex;align-items:center;gap:6px;min-width:0;flex:1">
                <input type="checkbox" data-run-chk="${esc(r.id)}" ${
            r.enabled ? "checked" : ""
          } title="勾选参与多轮叠图与阶跃PK" />
                ${
                  r.isLive
                    ? `<span style="width:10px;height:10px;border-radius:50%;background:${esc(
                        r.color
                      )};flex-shrink:0"></span>
                       <span style="font-weight:600;font-size:12px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${esc(
                         r.name
                       )}</span>`
                    : `<input type="color" data-run-col="${esc(r.id)}" value="${esc(
                        r.color
                      )}" style="width:18px;height:18px;padding:0;border:none;background:none;cursor:pointer;flex-shrink:0" title="曲线颜色" />
                       <input type="text" data-run-name="${esc(r.id)}" value="${esc(
                        r.name
                      )}" class="anl-inline-title" />`
                }
              </div>
              <div style="display:flex;align-items:center;gap:3px;flex-shrink:0">
                ${
                  r.isLive
                    ? `<button class="btn" data-pin-live="1" style="height:20px;padding:0 6px;font-size:11px" title="将当前缓冲存为永久实验记录">固化</button>`
                    : `<button class="btn btn-icon" data-exp-run="${esc(
                        r.id
                      )}" style="height:20px;min-width:20px;padding:0 4px" title="导出为 CSV">
                         <svg class="svg-ic" style="width:11px;height:11px"><use href="#ic-download"/></svg>
                       </button>
                       <button class="btn btn-icon btn-danger-outline" data-del-run="${esc(
                         r.id
                       )}" style="height:20px;min-width:20px;padding:0 4px" title="删除">✕</button>`
                }
              </div>
            </div>
            ${
              r.isLive
                ? `<div class="tb-label" style="margin-top:4px;overflow:hidden;text-overflow:ellipsis">${esc(
                    r.note
                  )}</div>`
                : `<input type="text" data-run-note="${esc(r.id)}" value="${esc(
                    r.note
                  )}" class="anl-note-inp" placeholder="调参备注 (如 Kp=0.8, Ki=120)..." />`
            }
            <div class="mono tb-label" style="margin-top:4px;display:flex;justify-content:space-between;font-size:11px">
              <span>${Math.round(r.sample_rate_hz)}Hz · ${dur}s (${r.sample_count.toLocaleString()}点)</span>
              <span>Δt:${dtMs >= 0 ? "+" + dtMs : dtMs}ms</span>
            </div>
          </div>`;
        })
        .join("");
    }

    function computeAutoShiftSec(bundle, chKey) {
      const manual = bundle.manualDt || 0;
      if (st.alignMode === "manual") return manual;
      const ch = getCh(bundle, chKey);
      if (!ch || ch.data.length < 4) return manual;
      const y = ch.data;
      const t = bundle.t;
      let minV = Infinity;
      let maxV = -Infinity;
      let maxIdx = 0;
      for (let i = 0; i < y.length; i++) {
        if (y[i] < minV) minV = y[i];
        if (y[i] > maxV) {
          maxV = y[i];
          maxIdx = i;
        }
      }
      if (st.alignMode === "peak") {
        return -t[maxIdx] + manual;
      }
      const target = st.alignMode === "threshold" ? st.alignThresh : (minV + maxV) * 0.5;
      for (let i = 1; i < y.length; i++) {
        if ((y[i - 1] < target && y[i] >= target) || (y[i - 1] > target && y[i] <= target)) {
          const frac = (target - y[i - 1]) / (y[i] - y[i - 1] || 1);
          const tCross = t[i - 1] + frac * (t[i] - t[i - 1]);
          return -tCross + manual;
        }
      }
      return manual;
    }

    // ==================== 6 TAB RENDERERS ====================

    function renderOverlayTab() {
      const isMultiRun = st.overlayScopeMode === "multi_run";
      const selCh = document.getElementById("anlSelOverlayCh");
      const mRow = document.getElementById("anlMultiChRow");
      const thWrap = document.getElementById("anlWrapThresh");
      if (selCh) selCh.style.display = isMultiRun ? "" : "none";
      if (mRow) mRow.style.display = isMultiRun ? "none" : "flex";
      if (thWrap) thWrap.style.display = st.alignMode === "threshold" ? "inline-flex" : "none";

      const series = [];
      if (isMultiRun) {
        const bundles = getCheckedBundles();
        for (const b of bundles) {
          const ch = getCh(b, st.overlayCh);
          if (!ch) continue;
          const shift = computeAutoShiftSec(b, st.overlayCh);
          const dy = b.manualDy || 0;
          let yLo = Infinity;
          let yHi = -Infinity;
          for (let i = 0; i < ch.data.length; i++) {
            if (ch.data[i] < yLo) yLo = ch.data[i];
            if (ch.data[i] > yHi) yHi = ch.data[i];
          }
          const span = Math.max(1e-6, yHi - yLo);
          const xArr = new Float64Array(b.t.length);
          const yArr = new Float32Array(b.t.length);
          for (let i = 0; i < b.t.length; i++) {
            xArr[i] = b.t[i] + shift;
            yArr[i] =
              st.normMode === "norm100" ? ((ch.data[i] - yLo) / span) * 100 : ch.data[i] + dy;
          }
          series.push({
            label: `${b.name} · ${ch.name}${
              st.normMode === "norm100" ? " (%)" : ch.unit ? " (" + ch.unit + ")" : ""
            }`,
            color: b.color,
            width: b.id === st.focusedId ? 2.2 : 1.65,
            x: xArr,
            y: yArr,
          });
        }
      } else {
        const fb = getFocusedBundle();
        if (fb) {
          for (const key of st.multiChSelected) {
            const ch = getCh(fb, key);
            if (!ch) continue;
            let yLo = Infinity;
            let yHi = -Infinity;
            for (let i = 0; i < ch.data.length; i++) {
              if (ch.data[i] < yLo) yLo = ch.data[i];
              if (ch.data[i] > yHi) yHi = ch.data[i];
            }
            const span = Math.max(1e-6, yHi - yLo);
            const yArr = new Float32Array(ch.data.length);
            for (let i = 0; i < ch.data.length; i++) {
              yArr[i] = st.normMode === "norm100" ? ((ch.data[i] - yLo) / span) * 100 : ch.data[i];
            }
            series.push({
              label: `${ch.name}${
                st.normMode === "norm100" ? " [0~100%]" : ch.unit ? " (" + ch.unit + ")" : ""
              }`,
              color: ch.color,
              width: 1.85,
              x: fb.t,
              y: yArr,
            });
          }
        }
      }

      const diffEl = document.getElementById("anlLblDiffInfo");
      if (st.showDiff && series.length >= 2) {
        const sA = series[0];
        const sB = series[1];
        const len = Math.min(sA.x.length, sB.x.length);
        const dx = new Float64Array(len);
        const dy = new Float32Array(len);
        let maxAbs = 0;
        let sq = 0;
        for (let i = 0; i < len; i++) {
          dx[i] = sA.x[i];
          const idxB = binaryNearestIdx(sB.x, sA.x[i]);
          const d = sA.y[i] - sB.y[idxB];
          dy[i] = d;
          if (Math.abs(d) > maxAbs) maxAbs = Math.abs(d);
          sq += d * d;
        }
        series.push({
          label: `Δ 差值 (曲线1 - 曲线2)`,
          color: "#EC4899",
          width: 1.6,
          dash: [5, 4],
          x: dx,
          y: dy,
        });
        if (diffEl) {
          diffEl.textContent = `Max|Δ|: ${maxAbs.toFixed(3)}  |  RMSE: ${Math.sqrt(
            sq / Math.max(1, len)
          ).toFixed(3)}`;
        }
      } else if (diffEl) {
        diffEl.textContent = st.showDiff ? "需至少勾选 2 条曲线" : "";
      }

      renderScientificCanvas(
        document.getElementById("canvasOverlay"),
        isMultiRun ? "多轮实验同通道对比" : "单次实验多通道叠放",
        series,
        st.charts.overlay,
        {
          xUnit: "s",
          onDrawOverlay({ ctx, xOf, padT, ph, pal }) {
            if (isMultiRun && st.alignMode !== "manual") {
              const zx = Math.round(xOf(0)) + 0.5;
              ctx.strokeStyle = pal.accent;
              ctx.lineWidth = 1.2;
              ctx.setLineDash([4, 3]);
              ctx.beginPath();
              ctx.moveTo(zx, padT);
              ctx.lineTo(zx, padT + ph);
              ctx.stroke();
              ctx.setLineDash([]);
            }
          },
        }
      );
    }

    function renderStepTab() {
      const fb = getFocusedBundle();
      const bundles = getCheckedBundles();
      const series = [];
      let focusM = null;

      if (fb) {
        focusM = analyzeStepResponse(fb, st.stepFbkCh, st.stepRefCh, st.settleBand);
        const fbk = getCh(fb, st.stepFbkCh);
        const ref = getCh(fb, st.stepRefCh);
        if (ref && fbk && ref.key !== fbk.key) {
          series.push({
            label: `${fb.name} · ${ref.name} (参考给定)`,
            color: "#64748B",
            dash: [5, 4],
            width: 1.5,
            x: fb.t,
            y: ref.data,
          });
        }
        if (fbk) {
          series.push({
            label: `${fb.name} · ${fbk.name} (实测响应)`,
            color: fb.color,
            width: 2.1,
            x: fb.t,
            y: fbk.data,
          });
        }
      }

      renderScientificCanvas(
        document.getElementById("cvsAnlStep"),
        "阶跃动态响应特征标定",
        series,
        st.charts.step,
        {
          xUnit: "s",
          onDrawUnderlay({ ctx, xOf, yOf, padL, padT, pw, ph, pal }) {
            if (!focusM || !focusM.valid) return;
            const yTop = yOf(focusM.yInf + focusM.tol);
            const yBot = yOf(focusM.yInf - focusM.tol);
            ctx.fillStyle = "rgba(16, 185, 129, 0.09)";
            ctx.fillRect(padL, Math.min(yTop, yBot), pw, Math.abs(yBot - yTop));

            const drawTag = (tx, col, txt) => {
              if (tx === null || tx === undefined) return;
              const px = Math.round(xOf(tx)) + 0.5;
              ctx.strokeStyle = col;
              ctx.lineWidth = 1.2;
              ctx.setLineDash([4, 3]);
              ctx.beginPath();
              ctx.moveTo(px, padT);
              ctx.lineTo(px, padT + ph);
              ctx.stroke();
              ctx.setLineDash([]);
              ctx.fillStyle = col;
              ctx.font = '600 11px "JetBrains Mono", Consolas, monospace';
              ctx.textAlign = "left";
              ctx.fillText(txt, px + 4, padT + 12);
            };
            drawTag(focusM.t90, pal.accent, `tr=${focusM.riseMs.toFixed(1)}ms`);
            if (focusM.overshootPct > 0.3) {
              drawTag(focusM.tPeak, pal.danger, `σ=${focusM.overshootPct.toFixed(1)}%`);
            }
            drawTag(focusM.tSettle, pal.ok, `ts=${focusM.settleMs.toFixed(1)}ms`);
          },
        }
      );

      const rows = bundles.map((b) => ({
        b,
        m: analyzeStepResponse(b, st.stepFbkCh, st.stepRefCh, st.settleBand),
      }));
      const valids = rows.filter((r) => r.m && r.m.valid);
      const bestTr = Math.min(...valids.map((r) => r.m.riseMs));
      const bestOv = Math.min(...valids.map((r) => r.m.overshootPct));
      const bestTs = Math.min(...valids.map((r) => r.m.settleMs));
      const bestPp = Math.min(...valids.map((r) => r.m.ripplePp));

      const tbody = document.getElementById("tbodyAnlStepPk");
      if (tbody) {
        tbody.innerHTML =
          rows
            .map(({ b, m }) => {
              if (!m || !m.valid) {
                return `<tr><td style="font-family:inherit;font-weight:600">${esc(
                  b.name
                )}</td><td colspan="8" style="font-family:inherit;color:var(--text-muted)">所选通道未检测到阶跃变化</td></tr>`;
              }
              const win = (ok) =>
                ok && valids.length > 1 ? ` <span class="anl-win-tag">最优</span>` : "";
              return `<tr class="${b.id === st.focusedId ? "selected-row" : ""}">
                <td style="font-family:inherit;font-weight:600"><span style="display:inline-block;width:8px;height:8px;border-radius:50%;background:${esc(
                  b.color
                )};margin-right:6px"></span>${esc(b.name)}</td>
                <td style="font-family:inherit;color:var(--text-secondary)">${esc(b.note || "-")}</td>
                <td>${m.y0.toFixed(2)} → ${m.yInf.toFixed(2)}</td>
                <td>${m.riseMs.toFixed(1)} ms${win(Math.abs(m.riseMs - bestTr) < 0.05)}</td>
                <td>${m.peakMs.toFixed(1)} ms</td>
                <td>${m.overshootPct.toFixed(1)} %${win(Math.abs(m.overshootPct - bestOv) < 0.05)}</td>
                <td>${m.settleMs.toFixed(1)} ms${win(Math.abs(m.settleMs - bestTs) < 0.05)}</td>
                <td>${m.ripplePp.toFixed(3)}${win(Math.abs(m.ripplePp - bestPp) < 1e-3)}</td>
                <td>${m.rmse.toFixed(3)}</td>
              </tr>`;
            })
            .join("") || `<tr><td colspan="9" style="text-align:center;color:var(--text-muted)">暂无数据</td></tr>`;
      }
    }

    function renderFftTab() {
      const fb = getFocusedBundle();
      const ch = fb ? getCh(fb, st.fftCh) : null;
      const cardsEl = document.getElementById("anlFftKpis");
      const peaksEl = document.getElementById("anlFftPeaksList");
      if (!fb || !ch) {
        if (cardsEl) cardsEl.innerHTML = "";
        if (peaksEl) peaksEl.innerHTML = "";
        renderScientificCanvas(document.getElementById("cvsAnlFft"), "FFT 频谱", [], st.charts.fft, { xUnit: "Hz" });
        return;
      }

      const res = computeFftSpectrum(ch.data, fb.hz, st.fftWin);

      // Estimate average RPM if a RPM channel exists
      const spdCh = fb.channels.find(
        (c) => c.key === "speed_est" || c.unit.toUpperCase() === "RPM" || c.name.includes("转速")
      );
      let avgRpm = 0;
      if (spdCh && spdCh.data.length > 0) {
        let s = 0;
        const s0 = Math.floor(spdCh.data.length * 0.5);
        for (let i = s0; i < spdCh.data.length; i++) s += Math.abs(spdCh.data[i]);
        avgRpm = s / Math.max(1, spdCh.data.length - s0);
      }
      const fm = avgRpm / 60;
      const fe = fm * st.polePairs;

      if (cardsEl) {
        cardsEl.innerHTML = `
          <div class="anl-kpi-box"><div class="tb-label">直流偏置 (DC)</div><div class="anl-kpi-val mono">${res.dc.toFixed(3)} ${esc(ch.unit)}</div></div>
          <div class="anl-kpi-box"><div class="tb-label">总谐波失真 (THD)</div><div class="anl-kpi-val mono" style="color:${res.thdPct > 8 ? "var(--warn)" : "var(--ok)"}">${res.thdPct.toFixed(2)} %</div></div>
          <div class="anl-kpi-box"><div class="tb-label">估算机械转频 1×fm</div><div class="anl-kpi-val mono">${fm > 0.1 ? fm.toFixed(1) + " Hz" : "--"}</div></div>
          <div class="anl-kpi-box"><div class="tb-label">电基频 1×fe (Pn=${st.polePairs})</div><div class="anl-kpi-val mono">${fe > 0.1 ? fe.toFixed(1) + " Hz" : "--"}</div></div>
        `;
      }

      const yDisp = new Float32Array(res.amps.length);
      for (let i = 0; i < res.amps.length; i++) {
        yDisp[i] = st.fftScale === "db" ? 20 * Math.log10(Math.max(1e-6, res.amps[i])) : res.amps[i];
      }

      renderScientificCanvas(
        document.getElementById("cvsAnlFft"),
        `${fb.name} · ${ch.name} 加窗频谱`,
        [
          {
            label: `${ch.name} 幅值 (${st.fftScale === "db" ? "dB" : ch.unit || "amp"})`,
            color: "#3B82F6",
            width: 1.75,
            x: res.freqs,
            y: yDisp,
          },
        ],
        st.charts.fft,
        {
          xUnit: "Hz",
          onDrawUnderlay({ ctx, xOf, padT, ph, pal }) {
            if (fe <= 1) return;
            for (const mult of [1, 2, 5, 6, 7]) {
              const ft = fe * mult;
              if (ft >= fb.hz * 0.5) continue;
              const px = Math.round(xOf(ft)) + 0.5;
              ctx.strokeStyle = "rgba(245, 158, 11, 0.32)";
              ctx.setLineDash([3, 3]);
              ctx.beginPath();
              ctx.moveTo(px, padT);
              ctx.lineTo(px, padT + ph);
              ctx.stroke();
              ctx.setLineDash([]);
              ctx.fillStyle = pal.warn;
              ctx.font = '10.5px "JetBrains Mono", Consolas, monospace';
              ctx.textAlign = "left";
              ctx.fillText(`${mult}fe`, px + 3, padT + 12);
            }
          },
        }
      );

      const explainFreq = (f) => {
        if (fe > 1.5) {
          const eo = f / fe;
          if (Math.abs(eo - 1) < 0.08) return "1×fe 电基波分量";
          if (Math.abs(eo - 2) < 0.08) return "2×fe（常见于两相电流采样增益不对称）";
          if (Math.abs(eo - 5) < 0.12 || Math.abs(eo - 7) < 0.12) return `${Math.round(eo)}×fe（逆变死区非线性 / 反电势谐波）`;
          if (Math.abs(eo - 6) < 0.12) return "6×fe（定子5·7次谐波映射至dq轴脉动）";
          if (Math.abs(eo - 12) < 0.15) return "12×fe（二阶死区谐波脉动）";
        }
        if (fm > 0.5 && Math.abs(f / fm - 1) < 0.12) return "1×fm 机械转频（对中偏差 / 偏心负载）";
        return fe > 1.5 ? `约 ${(f / fe).toFixed(2)}×fe` : `采样率占比 ${((f / fb.hz) * 100).toFixed(1)}%`;
      };

      if (peaksEl) {
        peaksEl.innerHTML =
          res.peaks
            .map(
              (p, i) => `<div class="anl-peak-item">
              <div class="mono" style="display:flex;justify-content:space-between;font-weight:600">
                <span>#${i + 1} · ${p.freq.toFixed(2)} Hz</span>
                <span style="color:var(--accent)">${p.amp.toFixed(4)} ${esc(ch.unit)}</span>
              </div>
              <div class="tb-label" style="white-space:normal;margin-top:2px">${explainFreq(p.freq)}</div>
            </div>`
            )
            .join("") || `<div class="tb-label">未检出显著谐波尖峰</div>`;
      }
    }

    function renderFilterTab() {
      const fb = getFocusedBundle();
      const ch = fb ? getCh(fb, st.filterCh) : null;
      const rng = document.getElementById("anlRngCutoff");
      const lblP = document.getElementById("anlLblFilterParam");
      const valP = document.getElementById("anlValFilterParam");
      const metricsEl = document.getElementById("anlFilterMetrics");
      const codeEl = document.getElementById("anlPreFilterCode");

      if (!fb || !ch) {
        if (codeEl) codeEl.textContent = "// 请先载入实验数据";
        renderScientificCanvas(document.getElementById("cvsAnlFilter"), "离线滤波器沙盒", [], st.charts.filter, { xUnit: "s" });
        return;
      }

      if (st.filterType === "movavg") {
        rng.min = "2";
        rng.max = "100";
        if (st.filterCutoff > 100) st.filterCutoff = 16;
        lblP.textContent = "滑动平均窗口点数 N";
        valP.textContent = `${Math.round(st.filterCutoff)} 点`;
      } else if (st.filterType === "notch") {
        rng.min = "2";
        rng.max = String(Math.max(10, Math.floor(fb.hz * 0.45)));
        lblP.textContent = "陷波中心频率 f0 (Hz)";
        valP.textContent = `${st.filterCutoff} Hz`;
      } else {
        rng.min = "1";
        rng.max = String(Math.max(10, Math.floor(fb.hz * 0.45)));
        lblP.textContent = "截止频率 fc (Hz)";
        valP.textContent = `${st.filterCutoff} Hz`;
      }
      rng.value = String(st.filterCutoff);

      const res = runDigitalFilter(ch.data, fb.hz, st.filterType, st.filterCutoff);
      if (codeEl) codeEl.textContent = res.cCode;

      if (metricsEl) {
        metricsEl.innerHTML = `
          <div style="display:flex;justify-content:space-between"><span class="tb-label">稳态纹波 P-P 衰减:</span><span class="mono" style="font-weight:600">${res.rawPp.toFixed(3)} → <strong style="color:var(--ok)">${res.filtPp.toFixed(3)} (-${res.attenPct.toFixed(0)}%)</strong></span></div>
          <div style="display:flex;justify-content:space-between"><span class="tb-label">等效群延迟滞后:</span><span class="mono" style="font-weight:600">${res.delayMs.toFixed(2)} ms</span></div>
          <div style="display:flex;justify-content:space-between"><span class="tb-label">当前采样基准频率:</span><span class="mono">${Math.round(fb.hz)} Hz</span></div>
        `;
      }

      renderScientificCanvas(
        document.getElementById("cvsAnlFilter"),
        `${fb.name} · 离线滤波前后同屏对比`,
        [
          {
            label: `原始信号 · ${ch.name}`,
            color: "#94A3B8",
            alpha: 0.55,
            width: 1.3,
            x: fb.t,
            y: ch.data,
          },
          {
            label: `滤波后预览 · ${ch.name}`,
            color: "#10B981",
            width: 2.2,
            x: fb.t,
            y: res.out,
          },
        ],
        st.charts.filter,
        { xUnit: "s" }
      );
    }

    function renderXyTab() {
      const fb = getFocusedBundle();
      const cx = fb ? getCh(fb, st.xyX) : null;
      const cy = fb ? getCh(fb, st.xyY) : null;
      const boxEl = document.getElementById("anlXyStats");

      if (!fb || !cx || !cy) {
        if (boxEl) boxEl.innerHTML = "";
        renderScientificCanvas(document.getElementById("cvsAnlXy"), "X-Y 轨迹", [], st.charts.xy, { isXY: true });
        return;
      }

      let xMin = Infinity;
      let xMax = -Infinity;
      let yMin = Infinity;
      let yMax = -Infinity;
      let rMax = 0;
      const len = Math.min(cx.data.length, cy.data.length);
      for (let i = 0; i < len; i++) {
        const x = cx.data[i];
        const y = cy.data[i];
        if (x < xMin) xMin = x;
        if (x > xMax) xMax = x;
        if (y < yMin) yMin = y;
        if (y > yMax) yMax = y;
        const r = Math.hypot(x, y);
        if (r > rMax) rMax = r;
      }

      renderScientificCanvas(
        document.getElementById("cvsAnlXy"),
        `${fb.name} · X:${cx.name} vs Y:${cy.name}`,
        [
          {
            label: `${cx.name} — ${cy.name}`,
            color: fb.color,
            width: 1.65,
            x: cx.data.subarray(0, len),
            y: cy.data.subarray(0, len),
          },
        ],
        st.charts.xy,
        {
          isXY: true,
          xUnit: cx.unit ? " " + cx.unit : "",
          defaultBounds: st.xyEqualAspect
            ? (_x0, _x1, _y0, _y1, pw, ph) => {
                const lim = Math.max(1, rMax * 1.12);
                const aspect = pw / Math.max(1, ph);
                return aspect >= 1 ? [-lim * aspect, lim * aspect, -lim, lim] : [-lim, lim, -lim / aspect, lim / aspect];
              }
            : null,
          onDrawUnderlay({ ctx, xOf, yOf, pal }) {
            if (!st.xyEqualAspect || rMax <= 0) return;
            const ox = xOf(0);
            const oy = yOf(0);
            const rx = Math.abs(xOf(rMax) - ox);
            const ry = Math.abs(yOf(rMax) - oy);
            ctx.strokeStyle = pal.danger;
            ctx.globalAlpha = 0.35;
            ctx.setLineDash([4, 4]);
            ctx.beginPath();
            ctx.ellipse(ox, oy, rx, ry, 0, 0, Math.PI * 2);
            ctx.stroke();
            ctx.setLineDash([]);
            ctx.globalAlpha = 1.0;
          },
        }
      );

      if (boxEl) {
        boxEl.innerHTML = `
          <div style="font-weight:600;font-size:12.5px">${esc(fb.name)} 包络特征</div>
          <div class="anl-kpi-box"><div class="tb-label">X 轴 (${esc(cx.name)}) 极值区间</div><div class="anl-kpi-val mono">${xMin.toFixed(3)} ~ ${xMax.toFixed(3)} ${esc(cx.unit)}</div></div>
          <div class="anl-kpi-box"><div class="tb-label">Y 轴 (${esc(cy.name)}) 极值区间</div><div class="anl-kpi-val mono">${yMin.toFixed(3)} ~ ${yMax.toFixed(3)} ${esc(cy.unit)}</div></div>
          <div class="anl-kpi-box"><div class="tb-label">最大合成模长 √(X²+Y²)</div><div class="anl-kpi-val mono" style="color:var(--accent)">${rMax.toFixed(3)}</div></div>
          <div class="anl-kpi-box"><div class="tb-label">X / Y 跨度峰峰值</div><div class="anl-kpi-val mono">ΔX ${(xMax - xMin).toFixed(2)} | ΔY ${(yMax - yMin).toFixed(2)}</div></div>
        `;
      }
    }

    function renderStatsTab() {
      const fb = getFocusedBundle();
      const titleEl = document.getElementById("anlLblStatsTitle");
      const tbody = document.getElementById("tbodyAnlFullStats");
      if (!tbody) return;
      if (!fb) {
        if (titleEl) titleEl.textContent = "--";
        tbody.innerHTML = `<tr><td colspan="7" style="text-align:center;color:var(--text-muted)">暂无数据</td></tr>`;
        return;
      }
      if (titleEl) titleEl.textContent = `${fb.name} (${fb.t.length.toLocaleString()} 点)`;
      tbody.innerHTML = fb.channels
        .map((ch) => {
          const arr = ch.data;
          const n = Math.max(1, arr.length);
          let sum = 0;
          let sq = 0;
          let mn = Infinity;
          let mx = -Infinity;
          for (let i = 0; i < arr.length; i++) {
            const v = arr[i];
            sum += v;
            sq += v * v;
            if (v < mn) mn = v;
            if (v > mx) mx = v;
          }
          const mean = sum / n;
          const rms = Math.sqrt(sq / n);
          const std = Math.sqrt(Math.max(0, sq / n - mean * mean));
          return `<tr>
            <td style="font-family:inherit;font-weight:600"><span style="display:inline-block;width:8px;height:8px;border-radius:50%;background:${esc(
              ch.color
            )};margin-right:6px"></span>${esc(ch.name)}${ch.unit ? " (" + esc(ch.unit) + ")" : ""}</td>
            <td>${mean.toFixed(3)}</td>
            <td>${rms.toFixed(3)}</td>
            <td>${mn.toFixed(3)}</td>
            <td>${mx.toFixed(3)}</td>
            <td>${(mx - mn).toFixed(3)}</td>
            <td>${std.toFixed(3)}</td>
          </tr>`;
        })
        .join("");
    }

    function renderActiveTab() {
      document.querySelectorAll("[data-anl-tab]").forEach((b) => {
        b.classList.toggle("active", b.dataset.anlTab === st.activeTab);
      });
      document.querySelectorAll("[data-anl-pane]").forEach((p) => {
        p.classList.toggle("active", p.dataset.anlPane === st.activeTab);
      });

      if (st.activeTab === "overlay") renderOverlayTab();
      else if (st.activeTab === "step") renderStepTab();
      else if (st.activeTab === "fft") renderFftTab();
      else if (st.activeTab === "filter") renderFilterTab();
      else if (st.activeTab === "xy") renderXyTab();
      else if (st.activeTab === "stats") renderStatsTab();
    }

    // ==================== EVENT WIRING ====================

    function bindEvents() {
      // Top functional tab switcher
      document.getElementById("anlTabBar").addEventListener("click", (e) => {
        const btn = e.target.closest("[data-anl-tab]");
        if (!btn) return;
        st.activeTab = btn.dataset.anlTab;
        renderActiveTab();
      });

      // Left Dataset Pool actions: +抓T1~T2, +抓全缓冲, +导入CSV
      document.getElementById("btnAnlGrabT1T2").addEventListener("click", async () => {
        const [t1, t2] = hooks.getCursorRange();
        const lo = Math.min(t1, t2);
        const hi = Math.max(t1, t2);
        const name = `Run #${st.slicesMeta.length + 1} (${lo.toFixed(2)}s~${hi.toFixed(2)}s)`;
        try {
          const meta = await invoke("cut_slice_cmd", { t1: lo, t2: hi, name });
          st.focusedId = meta.slice_id;
          await refreshAllData();
          showToast(`已抓取选区入库: ${meta.name}`);
        } catch (err) {
          showToast(String(err));
        }
      });

      const grabFullAction = async () => {
        const name = `Run #${st.slicesMeta.length + 1} (全缓冲快照)`;
        try {
          let meta;
          try {
            meta = await invoke("capture_full_slice_cmd", { name });
          } catch (e1) {
            if (String(e1).includes("capture_full_slice_cmd")) {
              meta = await invoke("cut_slice_cmd", { t1: -1e9, t2: 1e9, name });
            } else {
              throw e1;
            }
          }
          st.focusedId = meta.slice_id;
          await refreshAllData();
          showToast(`已抓取全部缓冲入库: ${meta.name}`);
        } catch (err) {
          showToast(String(err));
        }
      };
      document.getElementById("btnAnlGrabFull").addEventListener("click", grabFullAction);

      document.getElementById("btnAnlImportCsv").addEventListener("click", async () => {
        const path = await invoke("pick_file_dialog", { mode: "open", filterType: "csv" });
        if (!path) return;
        try {
          const meta = await invoke("import_csv_slice_cmd", { path });
          st.focusedId = meta.slice_id;
          await refreshAllData();
          showToast(`已导入实验 CSV: ${meta.name}`);
        } catch (err) {
          showToast("导入失败: " + err);
        }
      });

      // Dataset list interactions
      const runsList = document.getElementById("anlRunsList");
      runsList.addEventListener("click", async (e) => {
        if (e.target.closest("[data-pin-live]")) {
          e.stopPropagation();
          await grabFullAction();
          return;
        }
        const delBtn = e.target.closest("[data-del-run]");
        if (delBtn) {
          e.stopPropagation();
          await invoke("delete_slice_cmd", { sliceId: delBtn.dataset.delRun });
          await refreshAllData();
          return;
        }
        const expBtn = e.target.closest("[data-exp-run]");
        if (expBtn) {
          e.stopPropagation();
          const sid = expBtn.dataset.expRun;
          const m = st.slicesMeta.find((x) => x.slice_id === sid);
          const path = await invoke("pick_file_dialog", {
            mode: "save",
            defaultName: `${m ? m.name : sid}.csv`,
            filterType: "csv",
          });
          if (path) {
            await invoke("export_slice_csv_cmd", { sliceId: sid, path });
            showToast("实验数据已导出为 CSV");
          }
          return;
        }
        if (e.target.matches("input")) {
          e.stopPropagation();
          return;
        }
        const card = e.target.closest("[data-run-focus]");
        if (card && st.focusedId !== card.dataset.runFocus) {
          st.focusedId = card.dataset.runFocus;
          await ensureBundleLoaded(st.focusedId);
          renderSidebarList();
          populateChannelSelects();
          renderActiveTab();
        }
      });

      runsList.addEventListener("change", async (e) => {
        const t = e.target;
        if (t.dataset.runChk) {
          const id = t.dataset.runChk;
          if (id === "__live_buffer__") {
            st.liveChecked = t.checked;
            await ensureBundleLoaded(id);
            renderActiveTab();
          } else {
            await invoke("update_slice_cmd", { sliceId: id, patch: { enabled: t.checked } });
            await refreshAllData();
          }
        } else if (t.dataset.runName) {
          await invoke("update_slice_cmd", { sliceId: t.dataset.runName, patch: { name: t.value } });
          await refreshAllData();
        } else if (t.dataset.runNote) {
          await invoke("update_slice_cmd", { sliceId: t.dataset.runNote, patch: { note: t.value } });
          await refreshAllData();
        } else if (t.dataset.runCol) {
          await invoke("update_slice_cmd", { sliceId: t.dataset.runCol, patch: { color: t.value } });
          await refreshAllData();
        }
      });

      // Tab 1 Controls
      document.getElementById("anlSelScopeMode").addEventListener("change", (e) => {
        st.overlayScopeMode = e.target.value;
        st.charts.overlay.bounds = null;
        if (st.overlayScopeMode === "multi_ch") {
          st.normMode = "norm100";
          document.getElementById("anlSelNormMode").value = "norm100";
        }
        renderOverlayTab();
      });
      document.getElementById("anlSelOverlayCh").addEventListener("change", (e) => {
        st.overlayCh = e.target.value;
        st.charts.overlay.bounds = null;
        renderOverlayTab();
      });
      document.getElementById("anlSelNormMode").addEventListener("change", (e) => {
        st.normMode = e.target.value;
        st.charts.overlay.bounds = null;
        renderOverlayTab();
      });
      document.getElementById("anlMultiChRow").addEventListener("change", (e) => {
        const k = e.target.dataset.anlMch;
        if (!k) return;
        if (e.target.checked && !st.multiChSelected.includes(k)) st.multiChSelected.push(k);
        else if (!e.target.checked) st.multiChSelected = st.multiChSelected.filter((x) => x !== k);
        st.charts.overlay.bounds = null;
        renderOverlayTab();
      });
      document.getElementById("anlSelAlignMode").addEventListener("change", (e) => {
        st.alignMode = e.target.value;
        st.charts.overlay.bounds = null;
        renderOverlayTab();
      });
      document.getElementById("anlInpThresh").addEventListener("input", (e) => {
        st.alignThresh = parseFloat(e.target.value) || 0;
        st.charts.overlay.bounds = null;
        renderOverlayTab();
      });
      document.getElementById("btnAnlResetOffsets").addEventListener("click", async () => {
        st.liveManualDt = 0;
        st.liveManualDy = 0;
        for (const m of st.slicesMeta) {
          if (m.offset_x !== 0 || m.offset_y !== 0) {
            await invoke("update_slice_cmd", {
              sliceId: m.slice_id,
              patch: { offset_x: 0.0, offset_y: 0.0 },
            });
          }
        }
        st.charts.overlay.bounds = null;
        await refreshAllData();
        showToast("已复位所有手动曲线偏移与缩放");
      });
      document.getElementById("anlChkDiff").addEventListener("change", (e) => {
        st.showDiff = e.target.checked;
        st.charts.overlay.bounds = null;
        renderOverlayTab();
      });

      // Tab 2 Controls
      document.getElementById("anlSelStepFbk").addEventListener("change", (e) => {
        st.stepFbkCh = e.target.value;
        st.charts.step.bounds = null;
        renderStepTab();
      });
      document.getElementById("anlSelStepRef").addEventListener("change", (e) => {
        st.stepRefCh = e.target.value;
        st.charts.step.bounds = null;
        renderStepTab();
      });
      document.getElementById("anlSelSettleBand").addEventListener("change", (e) => {
        st.settleBand = parseFloat(e.target.value) || 0.02;
        renderStepTab();
      });

      // Tab 3 Controls
      document.getElementById("anlSelFftCh").addEventListener("change", (e) => {
        st.fftCh = e.target.value;
        st.charts.fft.bounds = null;
        renderFftTab();
      });
      document.getElementById("anlSelFftWin").addEventListener("change", (e) => {
        st.fftWin = e.target.value;
        renderFftTab();
      });
      document.getElementById("anlSelFftScale").addEventListener("change", (e) => {
        st.fftScale = e.target.value;
        st.charts.fft.bounds = null;
        renderFftTab();
      });
      document.getElementById("anlInpPoles").addEventListener("input", (e) => {
        st.polePairs = Math.max(1, parseInt(e.target.value, 10) || 7);
        renderFftTab();
      });

      // Tab 4 Controls
      document.getElementById("anlSelFilterCh").addEventListener("change", (e) => {
        st.filterCh = e.target.value;
        st.charts.filter.bounds = null;
        renderFilterTab();
      });
      document.getElementById("anlSelFilterType").addEventListener("change", (e) => {
        st.filterType = e.target.value;
        st.charts.filter.bounds = null;
        renderFilterTab();
      });
      document.getElementById("anlRngCutoff").addEventListener("input", (e) => {
        st.filterCutoff = parseFloat(e.target.value) || 20;
        renderFilterTab();
      });
      document.getElementById("btnAnlCopyFilterCode").addEventListener("click", async () => {
        const code = document.getElementById("anlPreFilterCode").textContent;
        await navigator.clipboard.writeText(code);
        showToast("已复制滤波器 C 代码");
      });

      // Tab 5 Controls
      document.getElementById("anlSelXyX").addEventListener("change", (e) => {
        st.xyX = e.target.value;
        st.charts.xy.bounds = null;
        renderXyTab();
      });
      document.getElementById("anlSelXyY").addEventListener("change", (e) => {
        st.xyY = e.target.value;
        st.charts.xy.bounds = null;
        renderXyTab();
      });
      document.getElementById("anlChkXyEqual").addEventListener("change", (e) => {
        st.xyEqualAspect = e.target.checked;
        st.charts.xy.bounds = null;
        renderXyTab();
      });

      // Export PNG Report Card
      document.getElementById("btnAnlExportPng").addEventListener("click", async () => {
        const pane = document.querySelector("[data-anl-pane].active");
        const srcCvs = pane ? pane.querySelector("canvas") : null;
        if (!srcCvs) {
          showToast("请切换到带波形图的分析页签导出图片");
          return;
        }
        const fb = getFocusedBundle();
        const path = await invoke("pick_file_dialog", {
          mode: "save",
          defaultName: `FluxScope_${st.activeTab}_${fb ? fb.name : "report"}.png`.replace(/[\\/:*?"<>|]/g, "_"),
          filterType: "png",
        });
        if (!path) return;
        try {
          await invoke("save_png_file_cmd", { path, dataUrl: srcCvs.toDataURL("image/png") });
          showToast("已保存分析图卡 PNG");
        } catch (err) {
          showToast("保存失败: " + err);
        }
      });

      // Interactive 2D Zoom/Pan + Shift-Curve-Drag on all 5 analysis canvases
      let startCurveDt = 0;
      let startCurveDy = 0;
      attachInteractiveChart(
        document.getElementById("canvasOverlay"),
        () => st.charts.overlay,
        () => renderOverlayTab(),
        async (phase, dx, dy) => {
          const fb = getFocusedBundle();
          if (!fb) return;
          if (phase === "start") {
            startCurveDt = fb.manualDt || 0;
            startCurveDy = fb.manualDy || 0;
          } else if (phase === "move") {
            fb.manualDt = startCurveDt + dx;
            fb.manualDy = startCurveDy + dy;
            if (fb.id === "__live_buffer__") {
              st.liveManualDt = fb.manualDt;
              st.liveManualDy = fb.manualDy;
            }
            renderOverlayTab();
          } else if (phase === "end") {
            if (fb.id !== "__live_buffer__") {
              await invoke("update_slice_cmd", {
                sliceId: fb.id,
                patch: { offset_x: fb.manualDt, offset_y: fb.manualDy },
              });
              await refreshAllData();
            } else {
              renderSidebarList();
            }
          }
        }
      );
      attachInteractiveChart(document.getElementById("cvsAnlStep"), () => st.charts.step, () => renderStepTab(), null);
      attachInteractiveChart(document.getElementById("cvsAnlFft"), () => st.charts.fft, () => renderFftTab(), null);
      attachInteractiveChart(document.getElementById("cvsAnlFilter"), () => st.charts.filter, () => renderFilterTab(), null);
      attachInteractiveChart(document.getElementById("cvsAnlXy"), () => st.charts.xy, () => renderXyTab(), null);
    }

    return {
      init() {
        bindEvents();
      },
      async refresh() {
        await refreshAllData();
      },
      async focusSliceAndOpen(sliceId) {
        st.focusedId = sliceId;
        await refreshAllData();
      },
      async applyRemoteState(av) {
        if (!av || typeof av !== "object") {
          await refreshAllData();
          return;
        }
        if (av.focused_slice_id) {
          st.focusedId = av.focused_slice_id;
        }
        if (av.tab && ["overlay", "step", "fft", "filter", "xy", "stats"].includes(av.tab)) {
          st.activeTab = av.tab;
        }
        const syncSel = (id, val) => {
          const el = document.getElementById(id);
          if (el && val !== undefined && val !== null && val !== "") el.value = String(val);
        };
        if (av.overlay_scope_mode) {
          st.overlayScopeMode = av.overlay_scope_mode;
          syncSel("anlSelScopeMode", av.overlay_scope_mode);
          st.charts.overlay.bounds = null;
        }
        if (av.overlay_channel) {
          st.overlayCh = av.overlay_channel;
          st.charts.overlay.bounds = null;
        }
        if (Array.isArray(av.multi_channels) && av.multi_channels.length > 0) {
          st.multiChSelected = av.multi_channels.slice();
          st.charts.overlay.bounds = null;
        }
        if (av.norm_mode) {
          st.normMode = av.norm_mode;
          syncSel("anlSelNormMode", av.norm_mode);
          st.charts.overlay.bounds = null;
        }
        if (av.align_mode) {
          st.alignMode = av.align_mode;
          syncSel("anlSelAlignMode", av.align_mode);
          st.charts.overlay.bounds = null;
        }
        if (typeof av.align_threshold === "number") {
          st.alignThresh = av.align_threshold;
          syncSel("anlInpThresh", av.align_threshold);
          st.charts.overlay.bounds = null;
        }
        if (typeof av.show_diff === "boolean") {
          st.showDiff = av.show_diff;
          const chk = document.getElementById("anlChkDiff");
          if (chk) chk.checked = av.show_diff;
          st.charts.overlay.bounds = null;
        }
        if (av.step_fbk) {
          st.stepFbkCh = av.step_fbk;
          st.charts.step.bounds = null;
        }
        if (av.step_ref) {
          st.stepRefCh = av.step_ref;
          st.charts.step.bounds = null;
        }
        if (typeof av.settle_band === "number") {
          st.settleBand = av.settle_band;
          syncSel("anlSelSettleBand", av.settle_band);
        }
        if (av.fft_channel) {
          st.fftCh = av.fft_channel;
          st.charts.fft.bounds = null;
        }
        if (av.fft_window) {
          st.fftWin = av.fft_window;
          syncSel("anlSelFftWin", av.fft_window);
        }
        if (av.fft_scale) {
          st.fftScale = av.fft_scale;
          syncSel("anlSelFftScale", av.fft_scale);
          st.charts.fft.bounds = null;
        }
        if (typeof av.pole_pairs === "number") {
          st.polePairs = Math.max(1, Math.round(av.pole_pairs));
          syncSel("anlInpPoles", st.polePairs);
        }
        if (av.filter_channel) {
          st.filterCh = av.filter_channel;
          st.charts.filter.bounds = null;
        }
        if (av.filter_type) {
          st.filterType = av.filter_type;
          syncSel("anlSelFilterType", av.filter_type);
          st.charts.filter.bounds = null;
        }
        if (typeof av.filter_cutoff === "number") {
          st.filterCutoff = av.filter_cutoff;
        }
        if (av.xy_x) {
          st.xyX = av.xy_x;
          st.charts.xy.bounds = null;
        }
        if (av.xy_y) {
          st.xyY = av.xy_y;
          st.charts.xy.bounds = null;
        }
        if (typeof av.xy_equal === "boolean") {
          st.xyEqualAspect = av.xy_equal;
          const chkXy = document.getElementById("anlChkXyEqual");
          if (chkXy) chkXy.checked = av.xy_equal;
          st.charts.xy.bounds = null;
        }
        await refreshAllData();
      },
      redrawCanvases() {
        renderActiveTab();
      },
    };
  }

  window.FocAnalysis = { createAnalysisController };
})();
