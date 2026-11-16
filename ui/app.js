/* FluxScope v2.0 — Main Application Controller */

(function () {
  const invoke = window.__TAURI__.core.invoke;
  const listen = window.__TAURI__.event.listen;

  const state = {
    profile: null,
    activePage: "scope",
    lastDataSeq: 0,
    isStreaming: false,
    paused: false,
    followEnabled: true,
    followAnchorRatio: 1.0, // [0.10, 1.00]
    followSpanSec: 5.0, // leftmost edge -> latest-frame anchor duration (s)
    followCustomSec: 10.0,
    customViewRange: null, // [tMin, tMax] when followEnabled is false
    fetchInFlight: false,
    pendingViewportFetch: false,
    pendingViewportPreserve: true,
    viewResetToken: 0,
    selectedSliceId: null,
    slicesCache: [],
    overlayCurvesCache: [],
    lastFftResult: null,
    editingDictTarget: null,
    lastViewport: null,
    lastHoverTime: null,
    readoutInFlight: false,
    pendingReadoutTime: undefined,
    hasNativeReadoutCmd: true,
    customReplayMode: false,
    lastCsvPath: (function () {
      try {
        return sessionStorage.getItem("foc_last_csv") || "";
      } catch (_) {
        return "";
      }
    })(),
  };

  const scopeGroup = new window.OscEngine.MultiScopeGroup({
    onViewportChanged(tMin, tMax, commitFetch, reason) {
      if (reason === "reset" || tMin === null || tMax === null) {
        state.followEnabled = false;
        state.customViewRange = null;
        syncTimebaseFollowUI();
        scheduleSaveFollowPrefs();
      } else if (reason === "pan") {
        state.followEnabled = false;
        state.customViewRange = [tMin, tMax];
        syncTimebaseFollowUI();
        scheduleSaveFollowPrefs();
      } else if (reason === "zoom") {
        const span = Math.max(0.0005, tMax - tMin);
        state.followSpanSec = Math.max(0.0005, span * state.followAnchorRatio);
        if (!state.followEnabled) {
          state.customViewRange = [tMin, tMax];
        }
        syncTimebaseFollowUI();
        scheduleSaveFollowPrefs();
      } else {
        state.customViewRange = [tMin, tMax];
      }
      if (commitFetch !== false) {
        requestViewportUpdate(true);
      } else {
        throttledPanFetch();
      }
    },
    onCursorsMoved(t1, t2, commitBackend) {
      updateCursorReadoutText(t1, t2);
      if (commitBackend) {
        invoke("set_cursors_cmd", { enabled: true, t1, t2 }).catch(() => {});
      }
      const src = (state.profile && state.profile.drawer_data_source) || "latest";
      if (src === "t1") scheduleReadoutAt(t1);
      else if (src === "t2") scheduleReadoutAt(t2);
    },
    onHoverChanged(tHover) {
      state.lastHoverTime = tHover;
      const src = (state.profile && state.profile.drawer_data_source) || "latest";
      if (src === "hover" && tHover !== null) {
        scheduleReadoutAt(tHover);
      }
    },
  });

  const widgetCtrl = window.FocWidgets.createWidgetController({
    getProfile: () => state.profile,
    getActivePage: () => state.activePage,
    onPersist: (rebuildPlots) => persistProfile(rebuildPlots, { fromWidgets: true }),
    onDrawerResized: () => scopeGroup.scheduleRender(),
  });

  const analysisCtrl = window.FocAnalysis.createAnalysisController({
    invoke,
    showToast,
    getCursorRange: () => [scopeGroup.cursorT1, scopeGroup.cursorT2],
  });

  let panTimer = null;
  function throttledPanFetch() {
    if (panTimer) return;
    panTimer = setTimeout(() => {
      panTimer = null;
      requestViewportUpdate(true);
    }, 55);
  }

  let followSaveTimer = null;
  function scheduleSaveFollowPrefs() {
    if (!state.profile) return;
    state.profile.follow_enabled = !!state.followEnabled;
    state.profile.follow_anchor_ratio = state.followAnchorRatio;
    state.profile.follow_span_sec = state.followSpanSec;
    state.profile.follow_custom_sec = state.followCustomSec;
    clearTimeout(followSaveTimer);
    followSaveTimer = setTimeout(() => {
      invoke("save_profile", { profile: state.profile }).catch(() => {});
    }, 280);
  }

  function syncTimebaseFollowUI() {
    state.followAnchorRatio = Math.max(0.1, Math.min(1.0, Number(state.followAnchorRatio) || 1.0));
    state.followSpanSec = Math.max(0.0005, Number(state.followSpanSec) || 5.0);
    state.followCustomSec = Math.max(0.01, Number(state.followCustomSec) || 10.0);

    scopeGroup.followEnabled = !!state.followEnabled;
    scopeGroup.followAnchorRatio = state.followAnchorRatio;

    const btnFollow = document.getElementById("btnFollowToggle");
    if (btnFollow) btnFollow.classList.toggle("is-active", !!state.followEnabled);

    const pct = Math.round(state.followAnchorRatio * 100);
    const fillEl = document.getElementById("followAnchorFill");
    const thumbEl = document.getElementById("followAnchorThumb");
    const lblPct = document.getElementById("lblFollowAnchorPct");
    if (fillEl) fillEl.style.width = pct + "%";
    if (thumbEl) thumbEl.style.left = pct + "%";
    if (lblPct) lblPct.textContent = pct + "%";

    const btn5s = document.getElementById("btnScale5s");
    if (btn5s) {
      btn5s.classList.toggle(
        "is-active",
        !!state.followEnabled && Math.abs(state.followSpanSec - 5.0) < 0.015
      );
    }

    const grpCust = document.querySelector(".scale-custom-group");
    if (grpCust) {
      grpCust.classList.toggle(
        "is-active",
        !!state.followEnabled && Math.abs(state.followSpanSec - state.followCustomSec) < 0.015
      );
    }

    const inpCust = document.getElementById("inpScaleCustom");
    if (inpCust && document.activeElement !== inpCust) {
      inpCust.value = String(Number(state.followCustomSec.toFixed(3)));
    }
  }

  function showToast(msg) {
    const el = document.getElementById("toastBox");
    el.textContent = msg;
    el.classList.add("show");
    clearTimeout(el._tid);
    el._tid = setTimeout(() => el.classList.remove("show"), 2600);
  }

  function esc(s) {
    return String(s ?? "")
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  function applyTheme(theme) {
    const t = theme === "light" ? "light" : "dark";
    document.documentElement.setAttribute("data-theme", t);
    const useEl = document.querySelector("#icTheme use");
    if (useEl) {
      useEl.setAttribute("href", t === "dark" ? "#ic-sun" : "#ic-moon");
    }
    scopeGroup.scheduleRender();
    if (state.profile) {
      widgetCtrl.refreshLayoutLock();
    }
    if (state.activePage === "lab") {
      analysisCtrl.redrawCanvases();
    }
  }

  function applyDrawerOpen(open) {
    const dr = document.getElementById("rightDrawer");
    const sp = document.getElementById("drawerWidthSplitter");
    const btn = document.getElementById("btnToggleDrawer");
    const visible = !!open && (state.activePage === "scope" || state.activePage === "config");
    if (dr) dr.classList.toggle("collapsed", !visible);
    if (sp) sp.classList.toggle("collapsed", !visible);
    if (btn) btn.classList.toggle("is-active", !!open);
    setTimeout(() => {
      scopeGroup.scheduleRender();
      if (visible && state.profile) {
        widgetCtrl.refreshLayoutLock();
      }
    }, 20);
  }

  function applyLayoutLockUI(locked) {
    const btn = document.getElementById("btnLockLayout");
    const lbl = document.getElementById("lblLock");
    const useEl = document.querySelector("#icLock use");
    btn.classList.toggle("is-active", !locked);
    lbl.textContent = locked ? "布局已锁" : "可调布局";
    if (useEl) useEl.setAttribute("href", locked ? "#ic-lock" : "#ic-unlock");

    document.getElementById("btnAddPlotQuick").disabled = locked;
    document.querySelectorAll(".plot-splitter").forEach((sp) => {
      sp.classList.toggle("locked", locked);
    });
    if (state.profile) {
      widgetCtrl.refreshLayoutLock();
    }
  }

  // ==================== DOM BUILDERS ====================

  function rebuildScopePlotPanes() {
    const stack = document.getElementById("plotsStack");
    stack.innerHTML = "";
    const plots = state.profile.plots || [];
    const specs = [];

    plots.forEach((p, idx) => {
      const pane = document.createElement("div");
      pane.className = "plot-pane";
      pane.style.flex = String(p.height_weight || 1);
      pane.dataset.plotId = p.id;

      const hdr = document.createElement("div");
      hdr.className = "plot-header-overlay";

      const wrap = document.createElement("div");
      wrap.className = "plot-canvas-wrap";
      const cvs = document.createElement("canvas");
      wrap.appendChild(cvs);

      pane.appendChild(hdr);
      pane.appendChild(wrap);
      stack.appendChild(pane);

      if (idx < plots.length - 1) {
        const sp = document.createElement("div");
        sp.className = "plot-splitter" + (state.profile.layout_locked ? " locked" : "");
        bindPlotSplitter(sp, pane, idx);
        stack.appendChild(sp);
      }

      specs.push({ id: p.id, title: p.title, canvas: cvs, legendEl: hdr });
    });

    scopeGroup.setPanes(specs);
  }

  function bindPlotSplitter(splitter, topPane, topIdx) {
    let dragging = false;
    let startY = 0;
    let startH1 = 0;
    let startH2 = 0;
    let botPane = null;

    splitter.addEventListener("mousedown", (e) => {
      if (state.profile.layout_locked) return;
      botPane = splitter.nextElementSibling;
      if (!botPane) return;
      dragging = true;
      startY = e.clientY;
      startH1 = topPane.getBoundingClientRect().height;
      startH2 = botPane.getBoundingClientRect().height;
      e.preventDefault();
    });

    window.addEventListener("mousemove", (e) => {
      if (!dragging || !botPane) return;
      const dy = e.clientY - startY;
      const total = Math.max(220, startH1 + startH2);
      const nh1 = Math.max(100, Math.min(total - 100, startH1 + dy));
      const nh2 = total - nh1;
      topPane.style.flex = String(nh1);
      botPane.style.flex = String(nh2);
      state.profile.plots[topIdx].height_weight = Number((nh1 / 250).toFixed(3));
      state.profile.plots[topIdx + 1].height_weight = Number((nh2 / 250).toFixed(3));
      scopeGroup.scheduleRender();
    });

    window.addEventListener("mouseup", () => {
      if (dragging) {
        dragging = false;
        persistProfile();
      }
    });
  }

  const PROTO_SNIPPETS = {
    justfloat: `// VOFA+ JustFloat 二进制小端协议 (同时支持穿插 printf 打印参数辨识文本)
typedef struct __attribute__((packed)) {
    float ch[27];          // 任意 N 个 float 通道 (IEEE-754 小端)
    uint8_t tail[4];       // 固定帧尾: {0x00, 0x00, 0x80, 0x7F}
} JustFloatFrame_t;

JustFloatFrame_t tx = { .tail = {0x00, 0x00, 0x80, 0x7F} };
tx.ch[0] = speed_ref;  tx.ch[1] = speed_est;  tx.ch[3] = iq_fbk;
UART_DMA_Send((uint8_t *)&tx, sizeof(tx));`,

    firewater: `// VOFA+ FireWater 逗号分隔明文协议 (直接兼容标准 printf, \\n 结尾)
printf("%.2f,%.2f,%.3f,%.2f\\n", speed_ref, speed_est, iq_fbk, vbus);

// 亦支持带任意前缀冒号格式:
// printf("foc: %.2f, %.2f, %.3f\\r\\n", speed_ref, speed_est, iq_fbk);`,

    rawdata: `// VOFA+ RawData 原始字节流模式 (不解析波形通道, 全部数据送入『终端』页)
printf("[MotorID] Rs=%.4f Ohm, Ls=%.6f H, Flux=%.5f Wb\\r\\n", rs, ls, flux);`,
  };

  const DEFAULT_STATE_ENUM = {
    "0": { label: "IDLE 待机", color: "#71717A" },
    "1": { label: "CALIB 零偏校准", color: "#0EA5E9" },
    "2": { label: "ALIGN 预定位", color: "#F59E0B" },
    "3": { label: "OPEN_LOOP 开环强拖", color: "#FB923C" },
    "4": { label: "CLOSED_LOOP 闭环FOC", color: "#10B981" },
    "5": { label: "BRAKING 主动刹车", color: "#A855F7" },
    "6": { label: "FAULT 故障保护", color: "#EF4444" },
    "7": { label: "MOTOR_ID 参数辨识", color: "#3B82F6" },
  };

  const DEFAULT_FAULT_BITMASK = {
    "0": "软件过流 (OVER_CURRENT)",
    "1": "母线过压 (OVER_VOLTAGE)",
    "2": "母线欠压 (UNDER_VOLTAGE)",
    "3": "过温保护 (OVER_TEMP)",
    "4": "超速保护 (OVER_SPEED)",
    "5": "堵转/失步 (STALL)",
    "6": "硬件CMPSS过流锁存 (HW_OVERCURRENT)",
    "7": "缺相故障 (PHASE_LOSS)",
    "8": "零偏校准失败 (OFFSET_ERR)",
    "9": "通信丢失 (COMM_LOST)",
    "10": "MOS管自检异常 (MOSFET_FAIL)",
  };

  function protoLabel(p) {
    if (p === "firewater") return "FireWater";
    if (p === "rawdata") return "RawData";
    return "JustFloat";
  }

  function renderChannelTables(prof) {
    const plotSelectOpts = (curPid) =>
      `<option value="none" ${curPid === "none" ? "selected" : ""}>不绘制</option>` +
      prof.plots
        .map(
          (p) =>
            `<option value="${esc(p.id)}" ${curPid === p.id ? "selected" : ""}>${esc(p.title)}</option>`
        )
        .join("");

    document.getElementById("tbodyChannels").innerHTML = prof.channels
      .map(
        (ch, i) => `<tr>
        <td class="mono">CH${ch.index}</td>
        <td><input type="checkbox" data-ch-f="visible" data-i="${i}" ${ch.visible ? "checked" : ""} /></td>
        <td><input type="text" data-ch-f="name" data-i="${i}" value="${esc(ch.name)}" /></td>
        <td><input type="text" class="mono" data-ch-f="alias" data-i="${i}" value="${esc(ch.alias)}" /></td>
        <td><input type="text" class="mono" data-ch-f="unit" data-i="${i}" value="${esc(ch.unit)}" /></td>
        <td><input type="number" step="any" class="mono" data-ch-f="display_gain" data-i="${i}" value="${
          ch.display_gain ?? 1
        }" title="仅对示波器波形绘制高度乘算该倍数，不影响任何数值显示" /></td>
        <td><input type="color" data-ch-f="color" data-i="${i}" value="${esc(ch.color)}" /></td>
        <td><select data-ch-f="plot_id" data-i="${i}">${plotSelectOpts(ch.plot_id)}</select></td>
      </tr>`
      )
      .join("");

    document.getElementById("tbodyMath").innerHTML = prof.math_channels
      .map(
        (mc, i) => `<tr>
        <td class="mono">${esc(mc.id)}</td>
        <td><input type="checkbox" data-mc-f="visible" data-i="${i}" ${mc.visible ? "checked" : ""} /></td>
        <td><input type="text" data-mc-f="name" data-i="${i}" value="${esc(mc.name)}" /></td>
        <td><input type="text" class="mono" data-mc-f="alias" data-i="${i}" value="${esc(mc.alias)}" /></td>
        <td><input type="text" class="mono" data-mc-f="expression" data-i="${i}" value="${esc(mc.expression)}" /></td>
        <td><input type="text" class="mono" data-mc-f="unit" data-i="${i}" value="${esc(mc.unit)}" /></td>
        <td><input type="color" data-mc-f="color" data-i="${i}" value="${esc(mc.color)}" /></td>
        <td><select data-mc-f="plot_id" data-i="${i}">${plotSelectOpts(mc.plot_id)}</select></td>
        <td><button class="btn btn-icon btn-danger-outline" data-del-mc="${i}" style="height:23px">✕</button></td>
      </tr>`
      )
      .join("");
  }

  function syncAllConfigViews(opts) {
    const prof = state.profile;
    if (!prof) return;
    if (opts && opts.fromWidgets) {
      renderChannelTables(prof);
      return;
    }

    if (!prof.connection) {
      prof.connection = {
        interface_type: "serial",
        last_port: "",
        baudrate: prof.baudrate || 460800,
        data_bits: 8,
        stop_bits: 1,
        parity: "none",
        flow_control: "none",
        dtr: false,
        rts: false,
        protocol: "justfloat",
        raw_hex_display: false,
        udp_bind_addr: "0.0.0.0:1347",
        tcp_remote_addr: "127.0.0.1:1347",
      };
    }
    if (!prof.status_decode) {
      prof.status_decode = {
        enabled: true,
        state_enabled: true,
        state_channel: 6,
        state_enum_map: DEFAULT_STATE_ENUM,
        fault_enabled: true,
        fault_channel: 7,
        fault_mode: "bitmask",
        fault_map: DEFAULT_FAULT_BITMASK,
        isr_enabled: true,
        isr_channel: 20,
        isr_label: "20kHz中断",
        isr_full_scale: 3000,
        isr_unit: "cyc",
      };
    }

    const conn = prof.connection;

    applyTheme(prof.theme);
    applyDrawerOpen(prof.right_drawer_open);
    applyLayoutLockUI(prof.layout_locked);
    const selDs = document.getElementById("selDrawerSource");
    if (selDs) selDs.value = prof.drawer_data_source || "latest";

    state.followEnabled = prof.follow_enabled !== false;
    state.followAnchorRatio = Math.max(0.1, Math.min(1.0, Number(prof.follow_anchor_ratio) || 1.0));
    state.followSpanSec = Math.max(0.0005, Number(prof.follow_span_sec) || 5.0);
    state.followCustomSec = Math.max(0.01, Number(prof.follow_custom_sec) || 10.0);
    syncTimebaseFollowUI();

    // Sync Replay Speed Preset & Custom Input
    const rSpd = Number(prof.replay_speed) > 0 ? Number(prof.replay_speed) : 1.0;
    const presetVals = [0.1, 0.2, 0.5, 1, 2, 5, 10];
    const matchedPreset = presetVals.find((v) => Math.abs(v - rSpd) < 1e-6);
    const selSpd = document.getElementById("selReplaySpeed");
    const wrapCust = document.getElementById("wrapCustomReplaySpeed");
    const inpCust = document.getElementById("inpCustomReplaySpeed");
    if (selSpd && wrapCust && inpCust) {
      if (!state.customReplayMode && matchedPreset !== undefined) {
        selSpd.value = String(matchedPreset);
        wrapCust.style.display = "none";
      } else {
        state.customReplayMode = true;
        selSpd.value = "custom";
        wrapCust.style.display = "inline-flex";
        if (document.activeElement !== inpCust) {
          inpCust.value = String(rSpd);
        }
      }
    }

    const rOrigin = prof.replay_start_mode || "zero";
    const lblOrig = document.getElementById("lblReplayOrigin");
    if (lblOrig) {
      lblOrig.textContent = rOrigin === "t1" ? "T1" : rOrigin === "t2" ? "T2" : "0秒";
      lblOrig.style.color =
        rOrigin === "t1"
          ? "var(--cursor-t1)"
          : rOrigin === "t2"
          ? "var(--cursor-t2)"
          : "var(--text-primary)";
    }
    document.querySelectorAll("#menuReplayOrigin .mini-pop-item").forEach((item) => {
      item.classList.toggle("active", item.dataset.origin === rOrigin);
    });

    // Sync Connection Page (`page-conn`)
    const iface = conn.interface_type || "serial";
    document.querySelectorAll("#connIfaceTabs .seg-tab").forEach((b) => {
      b.classList.toggle("active", b.dataset.iface === iface);
    });
    document.getElementById("connSerialBox").style.display = iface === "serial" ? "flex" : "none";
    document.getElementById("connUdpBox").style.display = iface === "udp" ? "flex" : "none";
    document.getElementById("connTcpBox").style.display = iface === "tcp" ? "flex" : "none";

    const baudStr = String(conn.baudrate || prof.baudrate || 460800);
    document.getElementById("inpBaud").value = baudStr;
    document.querySelectorAll("#baudChips .chip-btn").forEach((b) => {
      b.classList.toggle("active", b.dataset.baud === baudStr);
    });
    document.getElementById("selDataBits").value = String(conn.data_bits || 8);
    document.getElementById("selStopBits").value = String(conn.stop_bits || 1);
    document.getElementById("selParity").value = conn.parity || "none";
    document.getElementById("selFlowCtrl").value = conn.flow_control || "none";
    document.getElementById("chkDtr").checked = !!conn.dtr;
    document.getElementById("chkRts").checked = !!conn.rts;
    document.getElementById("inpUdpAddr").value = conn.udp_bind_addr || "0.0.0.0:1347";
    document.getElementById("inpTcpAddr").value = conn.tcp_remote_addr || "127.0.0.1:1347";

    const proto = conn.protocol || "justfloat";
    document.querySelectorAll("#protoCardsList .proto-card").forEach((c) => {
      c.classList.toggle("active", c.dataset.proto === proto);
    });
    document.getElementById("lblActiveProtoBadge").textContent = `当前: ${protoLabel(proto)}`;
    document.getElementById("rawHexOptRow").style.display = proto === "rawdata" ? "block" : "none";
    document.getElementById("chkRawHex").checked = !!conn.raw_hex_display;
    document.getElementById("preProtoCode").textContent =
      PROTO_SNIPPETS[proto] || PROTO_SNIPPETS.justfloat;

    document.getElementById("inpNominalHz").value = String(prof.nominal_sample_hz || 1000);
    document.getElementById("chkConnSpikeFilter").checked = !!prof.spike_filter_enabled;
    document.getElementById("inpConnSpikeThresh").value = String(prof.spike_threshold);

    // Topbar Mode & Presets
    document.getElementById("selMode").value = prof.operating_mode;
    document.getElementById("motorIdQuickBar").style.display =
      prof.operating_mode === "motor_id" ? "flex" : "none";

    document.getElementById("lblProfileName").textContent = prof.profile_name;
    document.getElementById("chkSpikeFilter").checked = !!prof.spike_filter_enabled;
    document.getElementById("inpSpikeThresh").value = String(prof.spike_threshold);

    // Page 3: Plot Panels Bar
    const pBar = document.getElementById("plotPanelsBar");
    pBar.innerHTML = prof.plots
      .map(
        (p, i) => `<div style="display:flex;align-items:center;gap:6px;background:var(--bg-panel);border:1px solid var(--border);padding:4px 8px;border-radius:4px">
        <span class="mono tb-label">${esc(p.id)}</span>
        <input type="text" data-plot-title="${i}" value="${esc(p.title)}" style="width:170px" />
        ${
          prof.plots.length > 1
            ? `<button class="btn btn-icon btn-danger-outline" data-del-plot="${esc(p.id)}" style="height:22px" title="删除此波形窗">✕</button>`
            : ""
        }
      </div>`
      )
      .join("");

    // Page 3: Hardware & Math Channels Tables
    renderChannelTables(prof);

    // Modular Right-Drawer Widgets & Configurator
    widgetCtrl.syncAll();
  }

  async function persistProfile(rebuildPanes, opts) {
    try {
      const res = await invoke("save_profile", { profile: state.profile });
      const fromWidgets = !!(opts && opts.fromWidgets);
      if (res && res.profile) {
        if (fromWidgets) {
          if (res.profile.status_decode) {
            state.profile.status_decode = res.profile.status_decode;
          }
        } else {
          state.profile = res.profile;
        }
      }
      if (res.math_warnings && res.math_warnings.length > 0) {
        showToast("公式语法提示: " + res.math_warnings[0]);
      }
      if (rebuildPanes) {
        rebuildScopePlotPanes();
      }
      syncAllConfigViews(opts);
      await requestViewportUpdate(true);
    } catch (e) {
      showToast("保存配置失败: " + e);
    }
  }

  // ==================== VIEWPORT & TELEMETRY RENDER ====================

  function updateCursorReadoutText(t1, t2) {
    const lo = Math.min(t1, t2);
    const hi = Math.max(t1, t2);
    const dt = hi - lo;
    const hz = dt > 1e-6 ? (1 / dt).toFixed(2) + " Hz" : "--";
    document.getElementById("lblCursorDelta").textContent =
      `T1: ${lo.toFixed(3)}s  |  T2: ${hi.toFixed(3)}s  |  ΔT: ${dt.toFixed(3)}s (${hz})`;
  }

  async function requestViewportUpdate(preserveManualZoom) {
    if (state.fetchInFlight) {
      state.pendingViewportFetch = true;
      state.pendingViewportPreserve = state.pendingViewportPreserve && !!preserveManualZoom;
      return;
    }
    state.fetchInFlight = true;
    state.pendingViewportPreserve = true;
    const reqToken = state.viewResetToken;
    try {
      const pw = scopeGroup.getPixelWidth();
      const args = state.followEnabled
        ? {
            pixelWidth: pw,
            tMin: null,
            tMax: null,
            followTailSec: state.followSpanSec,
            followAnchorRatio: state.followAnchorRatio,
          }
        : {
            pixelWidth: pw,
            tMin: preserveManualZoom && state.customViewRange ? state.customViewRange[0] : null,
            tMax: preserveManualZoom && state.customViewRange ? state.customViewRange[1] : null,
            followTailSec: null,
            followAnchorRatio: null,
          };
      const vp = await invoke("get_viewport", args);
      if (reqToken !== state.viewResetToken) {
        state.pendingViewportFetch = true;
        state.pendingViewportPreserve = false;
        return;
      }
      state.lastDataSeq = vp.data_seq;
      state.isStreaming = vp.is_streaming;
      state.paused = vp.paused;

      // Auto-expand profile UI if live UART discovered extra channels
      if (vp.traces && state.profile) {
        scopeGroup.setData(
          vp,
          preserveManualZoom && !!state.customViewRange && !state.followEnabled
        );
      }
      const btnCur = document.getElementById("btnCursors");
      if (btnCur) {
        btnCur.classList.toggle("is-active", !!vp.cursor_enabled);
      }

      updateCursorReadoutText(vp.cursor_t1, vp.cursor_t2);
      renderDrawerTelemetry(vp);
      renderStatusBar(vp);
    } catch (err) {
      console.error("get_viewport error:", err);
    } finally {
      state.fetchInFlight = false;
      if (state.pendingViewportFetch) {
        const nextPreserve = state.pendingViewportPreserve;
        state.pendingViewportFetch = false;
        state.pendingViewportPreserve = true;
        requestViewportUpdate(nextPreserve);
      }
    }
  }

  function renderDrawerTelemetry(vp) {
    // Live hardware/network connection state (Serial / UDP / TCP)
    const isHwLive =
      vp.is_streaming &&
      (vp.source_label.startsWith("串口") ||
        vp.source_label.startsWith("UDP") ||
        vp.source_label.startsWith("TCP"));

    document.getElementById("ledSerial").classList.toggle("online", isHwLive);
    document.getElementById("railConnDot").classList.toggle("online", isHwLive);
    document.getElementById("lblQuickConn").textContent = isHwLive ? "已连接" : "未连接";

    const btnConnMain = document.getElementById("btnConnect");
    btnConnMain.classList.toggle("btn-danger-outline", isHwLive);
    btnConnMain.classList.toggle("btn-primary", !isHwLive);
    document.getElementById("lblConnect").textContent = isHwLive
      ? "断开当前通信连接"
      : "立即建立连接";

    // Connection page telemetry boxes
    document.getElementById("connStatState").textContent = isHwLive
      ? "在线接收中"
      : vp.is_streaming
      ? "CSV 流回放中"
      : "未连接";
    document.getElementById("connStatState").style.color = isHwLive
      ? "var(--ok)"
      : "var(--text-primary)";
    document.getElementById("connStatHz").textContent = `${vp.sample_rate_hz} Hz`;
    document.getElementById("connStatKbps").textContent = `${(vp.rx_kbps || 0).toFixed(1)} KB/s`;
    document.getElementById("connStatFrames").textContent =
      `${vp.buffered_frames.toLocaleString()} / ${vp.spike_rejected}`;

    // Pause button state
    const btnPause = document.getElementById("btnPause");
    btnPause.classList.toggle("is-active", vp.paused);
    document.getElementById("lblPause").textContent = vp.paused ? "继续" : "暂停";
    document
      .querySelector("#icPauseBtn use")
      .setAttribute("href", vp.paused ? "#ic-play" : "#ic-pause");

    state.lastViewport = vp;

    const src = (state.profile && state.profile.drawer_data_source) || "latest";
    if (vp.buffered_frames === 0) {
      applyDrawerReadout(vp, "无波形");
    } else if (src === "hover" && state.lastHoverTime !== null) {
      scheduleReadoutAt(state.lastHoverTime);
    } else if ((src === "t1" || src === "t2") && !state.hasNativeReadoutCmd) {
      scheduleReadoutAt(src === "t1" ? scopeGroup.cursorT1 : scopeGroup.cursorT2);
    } else {
      const tag =
        src === "latest"
          ? `${vp.full_t_max.toFixed(2)}s`
          : src === "hover"
          ? "移入取值"
          : `${(vp.sample_time ?? (src === "t1" ? vp.cursor_t1 : vp.cursor_t2)).toFixed(3)}s`;
      applyDrawerReadout(vp, tag);
    }
  }

  function applyDrawerReadout(snap, timeTag) {
    const lblTime = document.getElementById("lblDrawerTime");
    if (lblTime && timeTag !== undefined) {
      lblTime.textContent = timeTag;
    }
    widgetCtrl.ingestSnapshot(snap);
  }

  function nearestIdxJs(arr, t) {
    let lo = 0;
    let hi = arr.length - 1;
    if (hi <= 0) return 0;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (arr[mid] < t) lo = mid + 1;
      else hi = mid;
    }
    if (lo > 0 && Math.abs(arr[lo - 1] - t) < Math.abs(arr[lo] - t)) return lo - 1;
    return lo;
  }

  async function scheduleReadoutAt(targetT) {
    if (targetT === null || targetT === undefined || !isFinite(targetT)) return;
    if (state.readoutInFlight) {
      state.pendingReadoutTime = targetT;
      return;
    }
    state.readoutInFlight = true;
    try {
      if (state.hasNativeReadoutCmd) {
        try {
          const snap = await invoke("get_readout_at", { inspectTime: targetT });
          applyDrawerReadout(snap, snap.has_data ? `${snap.sample_time.toFixed(3)}s` : "无波形");
        } catch (err) {
          if (String(err).includes("get_readout_at")) {
            state.hasNativeReadoutCmd = false;
          } else {
            throw err;
          }
        }
      }

      // Live hot-reload fallback when running inside a window opened before get_readout_at was compiled
      if (!state.hasNativeReadoutCmd && state.lastViewport) {
        const base = Object.assign({}, state.lastViewport);
        const valByKey = Object.assign({}, base.channel_vals || {});
        for (const tr of base.traces || []) {
          if (tr.t && tr.t.length > 0) {
            const idx = nearestIdxJs(tr.t, targetT);
            valByKey[tr.key] = tr.y[idx];
          }
        }
        try {
          const stRes = await invoke("compute_stats_and_fft", {
            t1: Math.max(0, targetT - 0.0025),
            t2: targetT + 0.0025,
            fftChannel: null,
          });
          if (stRes && stRes.stats && Array.isArray(stRes.stats.channels)) {
            for (const item of stRes.stats.channels) {
              valByKey[item.key] = item.mean;
            }
          }
        } catch (_) {}

        if (state.profile) {
          (state.profile.channels || []).forEach((ch) => {
            const k = `CH${ch.index}`;
            if (valByKey[k] === undefined && ch.alias && valByKey[ch.alias] !== undefined) {
              valByKey[k] = valByKey[ch.alias];
            } else if (ch.alias && valByKey[ch.alias] === undefined && valByKey[k] !== undefined) {
              valByKey[ch.alias] = valByKey[k];
            }
          });
          (state.profile.math_channels || []).forEach((mc) => {
            if (valByKey[mc.id] === undefined && mc.alias && valByKey[mc.alias] !== undefined) {
              valByKey[mc.id] = valByKey[mc.alias];
            } else if (mc.alias && valByKey[mc.alias] === undefined && valByKey[mc.id] !== undefined) {
              valByKey[mc.alias] = valByKey[mc.id];
            }
          });
        }
        base.channel_vals = valByKey;
        applyDrawerReadout(base, `${targetT.toFixed(3)}s`);
      }
    } finally {
      state.readoutInFlight = false;
      if (state.pendingReadoutTime !== undefined) {
        const nextT = state.pendingReadoutTime;
        state.pendingReadoutTime = undefined;
        scheduleReadoutAt(nextT);
      }
    }
  }

  function renderStatusBar(vp) {
    document.getElementById("sbSource").textContent = `数据源: ${vp.source_label}`;
    document.getElementById("sbFrames").textContent = `缓冲: ${vp.buffered_frames.toLocaleString()} 帧`;
    document.getElementById("sbRate").textContent = `采样率: ${vp.sample_rate_hz} Hz`;
    document.getElementById("sbSpikes").textContent = `坏点拦截: ${vp.spike_rejected}`;
  }

  // ==================== PAGE 4: TERMINAL & BOOKMARKS ====================

  async function refreshTerminalView() {
    const res = await invoke("get_bookmarks_and_console");
    const bms = res.bookmarks || [];
    const bmEl = document.getElementById("bookmarkList");
    if (bms.length === 0) {
      bmEl.innerHTML = `<div class="tb-label" style="padding:10px">暂无状态跳变或故障事件；载入 CSV 或连接电机运行后将自动生成毫秒级定位书签。</div>`;
    } else {
      bmEl.innerHTML = bms
        .map(
          (b) => `<div class="bm-item" data-bm-t="${b.time_s}">
          <span class="bm-time mono">${b.time_s.toFixed(3)}s</span>
          <span style="color:${b.kind === "fault" ? "var(--danger)" : "var(--text-primary)"}">${esc(
            b.text
          )}</span>
        </div>`
        )
        .join("");
    }

    const lines = res.console_lines || [];
    const conEl = document.getElementById("consoleOutput");
    conEl.textContent = lines.join("\n");
    conEl.scrollTop = conEl.scrollHeight;
  }

  // ==================== EVENT WIRING ====================

  function switchPage(pageId) {
    state.activePage = pageId;
    try {
      sessionStorage.setItem("foc_active_page", pageId);
    } catch (_) {}
    document.querySelectorAll(".rail-btn[data-page]").forEach((b) => {
      b.classList.toggle("active", b.dataset.page === pageId);
    });
    document.querySelectorAll(".page-view").forEach((p) => {
      p.classList.toggle("active", p.id === `page-${pageId}`);
    });

    const tbDock = document.getElementById("bottomTimebaseDock");
    if (tbDock) {
      tbDock.style.display = pageId === "scope" ? "inline-flex" : "none";
    }
    applyDrawerOpen(state.profile ? state.profile.right_drawer_open : true);

    if (pageId === "scope") {
      widgetCtrl.refreshLayoutLock();
      setTimeout(() => scopeGroup.scheduleRender(), 10);
    } else if (pageId === "config") {
      widgetCtrl.syncAll();
    } else if (pageId === "lab") {
      analysisCtrl.refresh();
    } else if (pageId === "terminal") {
      refreshTerminalView();
    }
  }

  function populatePortSelect(ports) {
    const sel = document.getElementById("selPort");
    const prev = sel.value || (state.profile && state.profile.connection && state.profile.connection.last_port) || "";
    if (!ports || ports.length === 0) {
      sel.innerHTML = `<option value="">未检测到串口设备 (插入 USB 转串口后点刷新)</option>`;
      return;
    }
    sel.innerHTML = ports
      .map((p) => {
        const desc = p.description && p.description !== p.port ? `${p.port} — ${p.description}` : p.port;
        return `<option value="${esc(p.port)}" title="${esc(desc)}">${esc(desc)}</option>`;
      })
      .join("");
    if (ports.some((p) => p.port === prev)) {
      sel.value = prev;
    } else {
      sel.value = ports[0].port;
    }
  }

  async function init() {
    const boot = await invoke("get_bootstrap");
    state.profile = boot.profile;
    populatePortSelect(boot.ports);

    document.getElementById("txtAiPrompt").value = boot.ai_prompt;
    document.getElementById("lblMcpBadge").textContent = boot.mcp_online
      ? "127.0.0.1:18580 在线"
      : "离线文件模式";
    const ledMcpTop = document.getElementById("ledMcpTop");
    const lblMcpTop = document.getElementById("lblMcpTop");
    if (ledMcpTop) ledMcpTop.classList.toggle("online", !!boot.mcp_online);
    if (lblMcpTop) lblMcpTop.textContent = boot.mcp_online ? "MCP 已启动" : "MCP 未启动";
    document.getElementById("btnMcpStatus").addEventListener("click", () => switchPage("terminal"));
    const MCP_BRIEF_MAP = {
      foc_get_status: "读取运行状态、链路采样率、任意时刻全通道精确值、状态/故障解码与终端日志",
      foc_get_channels_and_plots: "读取硬件通道、派生公式、分屏窗口、14 类侧栏仪表与时基跟随完整配置",
      foc_configure_channels: "批量配置硬件通道名称、简称、单位、示波独立显示增益、颜色及解码字典",
      foc_upsert_math_channel: "新增或修改上位机实时向量派生公式通道",
      foc_remove_math_channel: "删除指定的派生计算通道",
      foc_manage_plots_and_layout: "管理分屏波形窗、右侧栏 14 类多优先级仪表控件、时基跟随与深浅主题",
      foc_query_waveform: "读取实时选区或归档切片的统计指标与降采样波形序列",
      foc_compute_fft: "执行加窗 FFT 频谱与 THD 分析，自动映射电机极对数谐波成因",
      foc_set_cursors_and_slice: "定位双游标 T1/T2，支持附带调参备注切入分析池或导出选区 CSV",
      foc_manage_slices: "管理分析实验池：抓全缓冲、导入外部 CSV、修改调参备注与曲线对齐",
      foc_post_analysis: "六合一静态分析引擎：阶跃整定 PK、谐波 THD、离线滤波扫参与生成 C 代码、轨迹包络",
      foc_navigate_and_view: "切换主界面页面、精确缩放示波视口或联动控制分析页签参数",
      foc_stream_and_serial_control: "控制串口/UDP/TCP 连接、CSV 加载与流回放、清空/读取终端及下发指令",
      foc_save_or_load_profile: "保存、加载、列出配置预设档案，或恢复默认通道配置",
      foc_capture_screenshot: "对主示波器或任一静态分析专页渲染并导出高清 PNG 截图",
      foc_launch_studio: "唤醒置顶或冷启动 FluxScope 桌面主窗口",
    };
    const toolCount = (boot.mcp_tools || []).length;
    const secHdrEl = document.getElementById("lblMcpSecHdr");
    if (secHdrEl) secHdrEl.textContent = `已内置的 ${toolCount} 个双向工具接口`;
    document.getElementById("mcpToolsList").innerHTML = (boot.mcp_tools || [])
      .map((t) => {
        const brief = MCP_BRIEF_MAP[t.name] || t.description;
        return `<tr>
          <td class="mono" style="color:var(--accent);font-weight:600;white-space:nowrap">${esc(t.name)}</td>
          <td style="color:var(--text-primary);line-height:1.5">${esc(brief)}</td>
        </tr>`;
      })
      .join("");

    widgetCtrl.init();
    analysisCtrl.init();
    rebuildScopePlotPanes();
    syncAllConfigViews();
    await requestViewportUpdate(false);
    await analysisCtrl.refresh();

    try {
      const savedPage = sessionStorage.getItem("foc_active_page");
      if (savedPage && savedPage !== "scope" && document.getElementById(`page-${savedPage}`)) {
        switchPage(savedPage);
      }
    } catch (_) {}

    // Navigation rail clicks
    document.querySelectorAll(".rail-btn[data-page]").forEach((btn) => {
      btn.addEventListener("click", () => switchPage(btn.dataset.page));
    });

    // Topbar & Connection Page shared connect/disconnect handler
    const isHwLiveNow = () =>
      state.isStreaming && document.getElementById("lblConnect").textContent.includes("断开");

    const doToggleConnect = async (fromTopbar) => {
      if (isHwLiveNow()) {
        await invoke("serial_action", { args: { action: "disconnect" } });
        showToast("已断开通信链路");
        requestViewportUpdate(true);
        return;
      }

      const conn = state.profile.connection;
      const iface = conn.interface_type || "serial";
      const protocol = conn.protocol || "justfloat";
      const raw_hex_display = !!conn.raw_hex_display;

      try {
        if (iface === "serial") {
          const port = document.getElementById("selPort").value;
          if (!port) {
            if (fromTopbar) {
              switchPage("conn");
              showToast("请在连接页选择通信端口，或切换 UDP/TCP 与协议");
            } else {
              showToast("未检测到可用串口，请插入设备后点『刷新端口』，或点击顶部『打开CSV』");
            }
            return;
          }
          const baudrate = parseInt(document.getElementById("inpBaud").value, 10) || 460800;
          conn.last_port = port;
          conn.baudrate = baudrate;
          state.profile.baudrate = baudrate;
          await invoke("serial_action", {
            args: {
              action: "connect_serial",
              port,
              baudrate,
              protocol,
              data_bits: Number(conn.data_bits) || 8,
              stop_bits: Number(conn.stop_bits) || 1,
              parity: conn.parity || "none",
              flow_control: conn.flow_control || "none",
              dtr: !!conn.dtr,
              rts: !!conn.rts,
              raw_hex_display,
            },
          });
          showToast(`已连接 ${port} @ ${baudrate} (${protoLabel(protocol)})`);
        } else if (iface === "udp") {
          const udp_bind_addr = document.getElementById("inpUdpAddr").value.trim() || "0.0.0.0:1347";
          conn.udp_bind_addr = udp_bind_addr;
          await invoke("serial_action", {
            args: { action: "connect_udp", udp_bind_addr, protocol, raw_hex_display },
          });
          showToast(`已开启 UDP 监听 ${udp_bind_addr} (${protoLabel(protocol)})`);
        } else if (iface === "tcp") {
          const tcp_remote_addr = document.getElementById("inpTcpAddr").value.trim() || "127.0.0.1:1347";
          conn.tcp_remote_addr = tcp_remote_addr;
          await invoke("serial_action", {
            args: { action: "connect_tcp", tcp_remote_addr, protocol, raw_hex_display },
          });
          showToast(`已连接 TCP ${tcp_remote_addr} (${protoLabel(protocol)})`);
        }

        state.customViewRange = null;
        state.followEnabled = true;
        syncTimebaseFollowUI();
        scheduleSaveFollowPrefs();
        if (protocol === "rawdata" && state.activePage === "conn") {
          switchPage("terminal");
        }
        requestViewportUpdate(false);
      } catch (e) {
        showToast(String(e));
      }
    };

    document.getElementById("btnQuickConnect").addEventListener("click", () => doToggleConnect(true));
    document.getElementById("btnConnect").addEventListener("click", () => doToggleConnect(false));
    document.getElementById("btnConnJumpScope").addEventListener("click", () => switchPage("scope"));

    document.getElementById("selDrawerSource").addEventListener("change", async (e) => {
      const mode = e.target.value;
      state.profile.drawer_data_source = mode;
      if ((mode === "t1" || mode === "t2") && !scopeGroup.cursorEnabled) {
        scopeGroup.cursorEnabled = true;
        document.getElementById("btnCursors").classList.add("is-active");
        await invoke("set_cursors_cmd", { enabled: true });
        scopeGroup.scheduleRender();
      }
      await persistProfile(false);
      if (mode === "hover" && state.lastHoverTime !== null) {
        scheduleReadoutAt(state.lastHoverTime);
      } else if (mode === "t1") {
        scheduleReadoutAt(scopeGroup.cursorT1);
      } else if (mode === "t2") {
        scheduleReadoutAt(scopeGroup.cursorT2);
      }
    });

    // Connection Page (`#page-conn`) settings handlers
    document.getElementById("btnRefreshPorts").addEventListener("click", async () => {
      const r = await invoke("serial_action", { args: { action: "list_ports" } });
      populatePortSelect(r.ports);
      showToast(`已扫描到 ${(r.ports || []).length} 个串口`);
    });

    document.getElementById("connIfaceTabs").addEventListener("click", (e) => {
      const tab = e.target.closest("[data-iface]");
      if (!tab) return;
      state.profile.connection.interface_type = tab.dataset.iface;
      persistProfile(false);
    });

    document.getElementById("selPort").addEventListener("change", (e) => {
      state.profile.connection.last_port = e.target.value;
      persistProfile(false);
    });

    document.getElementById("inpBaud").addEventListener("change", (e) => {
      const b = Math.max(300, parseInt(e.target.value, 10) || 460800);
      state.profile.connection.baudrate = b;
      state.profile.baudrate = b;
      persistProfile(false);
    });

    document.getElementById("baudChips").addEventListener("click", (e) => {
      const chip = e.target.closest("[data-baud]");
      if (!chip) return;
      const b = parseInt(chip.dataset.baud, 10) || 460800;
      state.profile.connection.baudrate = b;
      state.profile.baudrate = b;
      persistProfile(false);
    });

    document.getElementById("selDataBits").addEventListener("change", (e) => {
      state.profile.connection.data_bits = Number(e.target.value) || 8;
      persistProfile(false);
    });
    document.getElementById("selStopBits").addEventListener("change", (e) => {
      state.profile.connection.stop_bits = Number(e.target.value) || 1;
      persistProfile(false);
    });
    document.getElementById("selParity").addEventListener("change", (e) => {
      state.profile.connection.parity = e.target.value;
      persistProfile(false);
    });
    document.getElementById("selFlowCtrl").addEventListener("change", (e) => {
      state.profile.connection.flow_control = e.target.value;
      persistProfile(false);
    });
    document.getElementById("chkDtr").addEventListener("change", (e) => {
      state.profile.connection.dtr = e.target.checked;
      persistProfile(false);
    });
    document.getElementById("chkRts").addEventListener("change", (e) => {
      state.profile.connection.rts = e.target.checked;
      persistProfile(false);
    });
    document.getElementById("inpUdpAddr").addEventListener("change", (e) => {
      state.profile.connection.udp_bind_addr = e.target.value.trim() || "0.0.0.0:1347";
      persistProfile(false);
    });
    document.getElementById("inpTcpAddr").addEventListener("change", (e) => {
      state.profile.connection.tcp_remote_addr = e.target.value.trim() || "127.0.0.1:1347";
      persistProfile(false);
    });

    document.getElementById("protoCardsList").addEventListener("click", (e) => {
      if (e.target.closest("#rawHexOptRow")) return;
      const card = e.target.closest(".proto-card[data-proto]");
      if (!card) return;
      const nextProto = card.dataset.proto;
      if (state.profile.connection.protocol === nextProto) return;
      state.profile.connection.protocol = nextProto;
      persistProfile(false);
      showToast(
        `解析协议已切换为 ${protoLabel(nextProto)}` +
          (isHwLiveNow() ? "（重连后对当前活动连接生效）" : "")
      );
    });

    document.getElementById("chkRawHex").addEventListener("change", (e) => {
      state.profile.connection.raw_hex_display = e.target.checked;
      persistProfile(false);
    });

    document.getElementById("btnCopyProtoCode").addEventListener("click", async () => {
      const code = document.getElementById("preProtoCode").textContent;
      await navigator.clipboard.writeText(code);
      showToast("已复制下位机 C 语言发送示例");
    });

    document.getElementById("inpNominalHz").addEventListener("change", (e) => {
      state.profile.nominal_sample_hz = Math.max(1, parseFloat(e.target.value) || 1000);
      persistProfile(false);
    });
    document.getElementById("chkConnSpikeFilter").addEventListener("change", (e) => {
      state.profile.spike_filter_enabled = e.target.checked;
      persistProfile(false);
    });
    document.getElementById("inpConnSpikeThresh").addEventListener("change", (e) => {
      state.profile.spike_threshold = parseFloat(e.target.value) || 1000000;
      persistProfile(false);
    });

    const rememberCsvPath = (p) => {
      if (!p) return;
      state.lastCsvPath = p;
      try {
        sessionStorage.setItem("foc_last_csv", p);
      } catch (_) {}
    };

    document.getElementById("btnOpenCsv").addEventListener("click", async () => {
      const path = await invoke("pick_file_dialog", { mode: "open", filterType: "csv" });
      if (!path) return;
      rememberCsvPath(path);
      try {
        state.viewResetToken++;
        scopeGroup.clearAllTraces();
        const info = await invoke("serial_action", {
          args: { action: "load_csv", csv_path: path },
        });
        state.customViewRange = null;
        state.followEnabled = false;
        syncTimebaseFollowUI();
        scheduleSaveFollowPrefs();
        await requestViewportUpdate(false);
        if (state.activePage === "lab") await analysisCtrl.refresh();
        showToast(`已载入 ${info.frames_loaded.toLocaleString()} 帧 (${info.duration_s}s)`);
      } catch (e) {
        showToast("加载 CSV 失败: " + e);
      }
    });

    const getEffectiveReplaySpeed = () => {
      const sel = document.getElementById("selReplaySpeed");
      if (!sel) return 1.0;
      if (sel.value === "custom") {
        const v = parseFloat(document.getElementById("inpCustomReplaySpeed").value);
        return isFinite(v) && v > 0 ? Math.min(100, Math.max(0.01, v)) : 1.0;
      }
      const p = parseFloat(sel.value);
      return isFinite(p) && p > 0 ? p : 1.0;
    };

    const applyReplaySpeedUpdate = async (spd) => {
      const clamped = Math.min(100, Math.max(0.01, Number(spd) || 1.0));
      state.profile.replay_speed = clamped;
      try {
        await invoke("serial_action", {
          args: { action: "set_replay_speed", replay_speed: clamped },
        });
      } catch (_) {}
      await persistProfile(false);
      if (
        state.isStreaming &&
        document.getElementById("sbSource").textContent.includes("流回放")
      ) {
        showToast(`流回放倍速已切换为 ${clamped}x`);
      }
    };

    document.getElementById("selReplaySpeed").addEventListener("change", (e) => {
      if (e.target.value === "custom") {
        state.customReplayMode = true;
        const wrap = document.getElementById("wrapCustomReplaySpeed");
        const inp = document.getElementById("inpCustomReplaySpeed");
        wrap.style.display = "inline-flex";
        inp.value = String(state.profile.replay_speed || 1.0);
        inp.focus();
        inp.select();
      } else {
        state.customReplayMode = false;
        document.getElementById("wrapCustomReplaySpeed").style.display = "none";
        applyReplaySpeedUpdate(parseFloat(e.target.value) || 1.0);
      }
    });

    document.getElementById("inpCustomReplaySpeed").addEventListener("change", (e) => {
      const v = parseFloat(e.target.value);
      if (!isFinite(v) || v <= 0) {
        showToast("请输入大于 0 的有效倍速 (0.01 ~ 100)");
        e.target.value = String(state.profile.replay_speed || 1.0);
        return;
      }
      applyReplaySpeedUpdate(v);
    });

    document.getElementById("inpCustomReplaySpeed").addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.target.blur();
      }
    });

    const launchReplayWithOrigin = async (csvPathOpt, isRestart) => {
      const spd = getEffectiveReplaySpeed();
      const startMode = (state.profile && state.profile.replay_start_mode) || "zero";
      state.profile.replay_speed = spd;
      const originTag = startMode === "t1" ? "T1" : startMode === "t2" ? "T2" : "0秒";

      try {
        state.viewResetToken++;
        scopeGroup.clearAllTraces();
        let res;
        try {
          res = await invoke("serial_action", {
            args: {
              action: isRestart ? "restart_replay" : "replay_csv",
              csv_path: csvPathOpt || state.lastCsvPath || undefined,
              replay_speed: spd,
              start_mode: startMode,
            },
          });
        } catch (err) {
          // Fallback if running in an older un-restarted window without restart_replay
          if (String(err).includes("restart_replay") && (csvPathOpt || state.lastCsvPath)) {
            res = await invoke("serial_action", {
              args: {
                action: "replay_csv",
                csv_path: csvPathOpt || state.lastCsvPath,
                replay_speed: spd,
              },
            });
          } else {
            throw err;
          }
        }
        state.viewResetToken++;
        state.customViewRange = null;
        state.followEnabled = true;
        syncTimebaseFollowUI();
        scheduleSaveFollowPrefs();
        if (res && res.fallback_zero) {
          showToast(`${originTag} 超出文件末尾，已从 0秒 开始 ${spd}x 回放`);
        } else {
          showToast(`已从 ${originTag} 开启 ${spd}x 流回放`);
        }
        await requestViewportUpdate(false);
      } catch (e) {
        if (isRestart && String(e).includes("CSV")) {
          const picked = await invoke("pick_file_dialog", { mode: "open", filterType: "csv" });
          if (picked) {
            rememberCsvPath(picked);
            return launchReplayWithOrigin(picked, false);
          }
          return;
        }
        showToast("流回放失败: " + e);
      }
    };

    document.getElementById("btnReplayCsv").addEventListener("click", async () => {
      const path = await invoke("pick_file_dialog", { mode: "open", filterType: "csv" });
      if (!path) return;
      rememberCsvPath(path);
      await launchReplayWithOrigin(path, false);
    });

    document.getElementById("btnRestartReplay").addEventListener("click", async () => {
      await launchReplayWithOrigin(null, true);
    });

    const menuOrigin = document.getElementById("menuReplayOrigin");
    document.getElementById("btnReplayOrigin").addEventListener("click", (e) => {
      e.stopPropagation();
      menuOrigin.classList.toggle("open");
    });

    menuOrigin.addEventListener("click", async (e) => {
      const item = e.target.closest("[data-origin]");
      if (!item) return;
      e.stopPropagation();
      menuOrigin.classList.remove("open");
      const mode = item.dataset.origin;
      state.profile.replay_start_mode = mode;
      if ((mode === "t1" || mode === "t2") && !scopeGroup.cursorEnabled) {
        scopeGroup.cursorEnabled = true;
        document.getElementById("btnCursors").classList.add("is-active");
        await invoke("set_cursors_cmd", { enabled: true });
        scopeGroup.scheduleRender();
      }
      await persistProfile(false);
    });

    window.addEventListener("click", () => {
      menuOrigin.classList.remove("open");
    });

    document.getElementById("selMode").addEventListener("change", (e) => {
      state.profile.operating_mode = e.target.value;
      persistProfile(false);
    });

    document.getElementById("btnPause").addEventListener("click", async () => {
      await invoke("serial_action", { args: { action: state.paused ? "resume" : "pause" } });
      requestViewportUpdate(true);
    });

    document.getElementById("btnFitAll").addEventListener("click", () => {
      state.customViewRange = null;
      state.followEnabled = false;
      syncTimebaseFollowUI();
      scheduleSaveFollowPrefs();
      scopeGroup.resetZoomFull();
    });

    document.getElementById("btnClear").addEventListener("click", async () => {
      state.viewResetToken++;
      scopeGroup.clearAllTraces();
      await invoke("serial_action", { args: { action: "clear_buffer" } });
      state.customViewRange = null;
      await requestViewportUpdate(false);
      if (state.activePage === "lab") await analysisCtrl.refresh();
    });

    document.getElementById("btnCursors").addEventListener("click", async () => {
      const next = !scopeGroup.cursorEnabled;
      scopeGroup.cursorEnabled = next;
      document.getElementById("btnCursors").classList.toggle("is-active", next);
      await invoke("set_cursors_cmd", { enabled: next });
      scopeGroup.scheduleRender();
    });

    const handleCutSlice = async () => {
      const lo = Math.min(scopeGroup.cursorT1, scopeGroup.cursorT2);
      const hi = Math.max(scopeGroup.cursorT1, scopeGroup.cursorT2);
      const defaultName = `Run (${lo.toFixed(2)}s~${hi.toFixed(2)}s)`;
      try {
        const meta = await invoke("cut_slice_cmd", {
          t1: lo,
          t2: hi,
          name: defaultName,
        });
        switchPage("lab");
        await analysisCtrl.focusSliceAndOpen(meta.slice_id);
        showToast(`已切入静态分析台: ${meta.name}`);
      } catch (e) {
        showToast(String(e));
      }
    };
    document.getElementById("btnCutSlice").addEventListener("click", handleCutSlice);

    document.getElementById("btnExportSliceCsv").addEventListener("click", async () => {
      const lo = Math.min(scopeGroup.cursorT1, scopeGroup.cursorT2).toFixed(2);
      const hi = Math.max(scopeGroup.cursorT1, scopeGroup.cursorT2).toFixed(2);
      const path = await invoke("pick_file_dialog", {
        mode: "save",
        defaultName: `FOC选区_${lo}s_${hi}s.csv`,
        filterType: "csv",
      });
      if (!path) return;
      try {
        const res = await invoke("export_cursor_csv_cmd", {
          t1: scopeGroup.cursorT1,
          t2: scopeGroup.cursorT2,
          path,
        });
        showToast(`已导出 ${res.samples} 点选区 CSV`);
      } catch (e) {
        showToast("导出失败: " + e);
      }
    });

    document.getElementById("btnLockLayout").addEventListener("click", () => {
      state.profile.layout_locked = !state.profile.layout_locked;
      persistProfile(false);
    });

    document.getElementById("btnToggleTheme").addEventListener("click", () => {
      state.profile.theme = state.profile.theme === "dark" ? "light" : "dark";
      persistProfile(false);
    });

    document.getElementById("btnToggleDrawer").addEventListener("click", () => {
      state.profile.right_drawer_open = !state.profile.right_drawer_open;
      persistProfile(false);
    });

    // Bottom Centered Timebase Follow Dock (Toggle + VOFA Anchor Slider + 5s & Custom Presets)
    document.getElementById("btnFollowToggle").addEventListener("click", () => {
      state.followEnabled = !state.followEnabled;
      if (state.followEnabled) {
        const curSpan = scopeGroup.viewTMax - scopeGroup.viewTMin;
        if (isFinite(curSpan) && curSpan >= 0.0005) {
          state.followSpanSec = Math.max(0.0005, curSpan * state.followAnchorRatio);
        }
        state.customViewRange = null;
        syncTimebaseFollowUI();
        scheduleSaveFollowPrefs();
        requestViewportUpdate(false);
      } else {
        state.customViewRange = [scopeGroup.viewTMin, scopeGroup.viewTMax];
        syncTimebaseFollowUI();
        scheduleSaveFollowPrefs();
        scopeGroup.scheduleRender();
      }
    });

    const anchorTrack = document.getElementById("followAnchorTrack");
    const ratioFromClientX = (clientX) => {
      const r = anchorTrack.getBoundingClientRect();
      const raw = (clientX - r.left) / Math.max(1, r.width);
      return Math.max(0.1, Math.min(1.0, Math.round(raw * 100) / 100));
    };

    const applyLiveAnchorRatio = (nextRatio, commitNow) => {
      state.followAnchorRatio = nextRatio;
      state.followEnabled = true;
      state.customViewRange = null;
      syncTimebaseFollowUI();
      const headT = scopeGroup.fullTMax;
      const totalSpan = state.followSpanSec / state.followAnchorRatio;
      scopeGroup.viewTMin = headT - state.followSpanSec;
      scopeGroup.viewTMax = scopeGroup.viewTMin + totalSpan;
      scopeGroup.scheduleRender();
      if (commitNow) {
        scheduleSaveFollowPrefs();
        requestViewportUpdate(false);
      } else {
        throttledPanFetch();
      }
    };

    anchorTrack.addEventListener("mousedown", (e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      scopeGroup.anchorDragging = true;
      anchorTrack.classList.add("dragging");
      applyLiveAnchorRatio(ratioFromClientX(e.clientX), false);

      const onMove = (ev) => {
        applyLiveAnchorRatio(ratioFromClientX(ev.clientX), false);
      };
      const onUp = (ev) => {
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
        scopeGroup.anchorDragging = false;
        anchorTrack.classList.remove("dragging");
        applyLiveAnchorRatio(ratioFromClientX(ev.clientX), true);
      };
      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    });

    anchorTrack.addEventListener("dblclick", (e) => {
      e.preventDefault();
      const next = state.followAnchorRatio > 0.95 ? 0.5 : 1.0;
      applyLiveAnchorRatio(next, true);
    });

    document.getElementById("btnScale5s").addEventListener("click", () => {
      state.followSpanSec = 5.0;
      state.followEnabled = true;
      state.customViewRange = null;
      syncTimebaseFollowUI();
      scheduleSaveFollowPrefs();
      requestViewportUpdate(false);
    });

    const applyCustomScaleSec = () => {
      const inp = document.getElementById("inpScaleCustom");
      const v = parseFloat(inp.value);
      if (!isFinite(v) || v <= 0) {
        showToast("请输入大于 0 的秒数");
        inp.value = String(Number(state.followCustomSec.toFixed(3)));
        return;
      }
      const clamped = Math.max(0.01, Math.min(36000, v));
      state.followCustomSec = clamped;
      state.followSpanSec = clamped;
      state.followEnabled = true;
      state.customViewRange = null;
      syncTimebaseFollowUI();
      scheduleSaveFollowPrefs();
      requestViewportUpdate(false);
    };

    document.getElementById("btnScaleCustom").addEventListener("click", applyCustomScaleSec);
    document.getElementById("inpScaleCustom").addEventListener("change", applyCustomScaleSec);
    document.getElementById("inpScaleCustom").addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.target.blur();
      }
    });

    document.getElementById("btnMarkT1").addEventListener("click", async () => {
      const now = scopeGroup.fullTMax;
      scopeGroup.cursorEnabled = true;
      scopeGroup.cursorT1 = now;
      if (scopeGroup.cursorT2 <= now) scopeGroup.cursorT2 = now + 0.5;
      document.getElementById("btnCursors").classList.add("is-active");
      await invoke("set_cursors_cmd", {
        enabled: true,
        t1: scopeGroup.cursorT1,
        t2: scopeGroup.cursorT2,
      });
      requestViewportUpdate(true);
    });

    document.getElementById("btnMarkT2").addEventListener("click", async () => {
      scopeGroup.cursorEnabled = true;
      scopeGroup.cursorT2 = scopeGroup.fullTMax;
      document.getElementById("btnCursors").classList.add("is-active");
      await invoke("set_cursors_cmd", {
        enabled: true,
        t1: scopeGroup.cursorT1,
        t2: scopeGroup.cursorT2,
      });
      requestViewportUpdate(true);
    });

    const addNewPlotPanel = () => {
      let k = state.profile.plots.length + 1;
      while (state.profile.plots.some((p) => p.id === `plot_${k}`)) k++;
      state.profile.plots.push({
        id: `plot_${k}`,
        title: `自定义波形窗 ${k}`,
        y_label: "",
        height_weight: 1.0,
      });
      persistProfile(true);
    };
    document.getElementById("btnAddPlotQuick").addEventListener("click", addNewPlotPanel);
    document.getElementById("btnAddPlotConfig").addEventListener("click", addNewPlotPanel);

    // Page 3: Config Table Handlers
    document.getElementById("chkSpikeFilter").addEventListener("change", (e) => {
      state.profile.spike_filter_enabled = e.target.checked;
      persistProfile(false);
    });
    document.getElementById("inpSpikeThresh").addEventListener("change", (e) => {
      state.profile.spike_threshold = parseFloat(e.target.value) || 1000000;
      persistProfile(false);
    });

    document.getElementById("btnSavePreset").addEventListener("click", async () => {
      const name = prompt("输入预设名称：", state.profile.profile_name);
      if (!name) return;
      const r = await invoke("profile_action", {
        args: { action: "save", profile_name: name },
      });
      state.profile = r.profile;
      syncAllConfigViews();
      showToast(`预设已保存: ${name}`);
    });

    document.getElementById("btnLoadPreset").addEventListener("click", async () => {
      const listRes = await invoke("profile_action", { args: { action: "list" } });
      const names = listRes.result.profiles || [];
      const pick = prompt(`输入要加载的预设名 (可选: ${names.join(", ")})：`, names[0] || "");
      if (!pick) return;
      try {
        const r = await invoke("profile_action", {
          args: { action: "load", profile_name: pick },
        });
        state.profile = r.profile;
        rebuildScopePlotPanes();
        syncAllConfigViews();
        requestViewportUpdate(true);
        showToast(`已加载预设: ${pick}`);
      } catch (e) {
        showToast(String(e));
      }
    });

    document.getElementById("btnResetDefault").addEventListener("click", async () => {
      const r = await invoke("profile_action", { args: { action: "reset_default" } });
      state.profile = r.profile;
      rebuildScopePlotPanes();
      syncAllConfigViews();
      requestViewportUpdate(true);
      showToast("已恢复默认 27 通道配置");
    });

    document.getElementById("plotPanelsBar").addEventListener("change", (e) => {
      if (e.target.dataset.plotTitle !== undefined) {
        state.profile.plots[Number(e.target.dataset.plotTitle)].title = e.target.value;
        persistProfile(true);
      }
    });
    document.getElementById("plotPanelsBar").addEventListener("click", (e) => {
      const del = e.target.closest("[data-del-plot]");
      if (del && state.profile.plots.length > 1) {
        const pid = del.dataset.delPlot;
        state.profile.plots = state.profile.plots.filter((p) => p.id !== pid);
        state.profile.channels.forEach((c) => {
          if (c.plot_id === pid) c.plot_id = "none";
        });
        state.profile.math_channels.forEach((m) => {
          if (m.plot_id === pid) m.plot_id = "none";
        });
        persistProfile(true);
      }
    });

    document.getElementById("btnAddHwChannel").addEventListener("click", () => {
      const idx = state.profile.channels.length;
      state.profile.channels.push({
        index: idx,
        name: `通道 CH${idx}`,
        alias: `ch${idx}`,
        unit: "",
        color: "#0EA5E9",
        width: 1.5,
        display_gain: 1.0,
        visible: false,
        plot_id: "none",
        show_card: false,
        decimals: 2,
        decoder_type: "float",
        enum_map: {},
        bitmask_map: {},
      });
      persistProfile(false);
    });

    document.getElementById("tbodyChannels").addEventListener("change", (e) => {
      const t = e.target;
      const idx = Number(t.dataset.i);
      const fld = t.dataset.chF;
      if (!fld || isNaN(idx)) return;
      const ch = state.profile.channels[idx];
      if (t.type === "checkbox") {
        ch[fld] = t.checked;
      } else if (fld === "display_gain") {
        const gv = parseFloat(t.value);
        ch.display_gain = Number.isFinite(gv) ? gv : 1.0;
      } else {
        ch[fld] = t.value;
      }
      persistProfile(false);
    });

    // Math Channels
    const addMath = (name, alias, expression, unit) => {
      let k = 0;
      while (state.profile.math_channels.some((m) => m.id === `M${k}`)) k++;
      const pid = state.profile.plots[0] ? state.profile.plots[0].id : "none";
      state.profile.math_channels.push({
        id: `M${k}`,
        name,
        alias,
        expression,
        unit,
        color: "#EC4899",
        width: 1.6,
        visible: true,
        plot_id: pid,
        show_card: true,
        decimals: 3,
      });
      persistProfile(false);
    };

    document.getElementById("btnAddMathChannel").addEventListener("click", () => {
      addMath("自定义派生量", `math_${state.profile.math_channels.length}`, "CH0 * 1.0", "");
    });

    document.querySelectorAll("[data-math-tpl]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const kind = btn.dataset.mathTpl;
        if (kind === "mod") {
          addMath("电压调制比", "mod_idx", "sqrt(Vd^2 + Vq^2) / maximum(Vbus / 1.73205, 1.0)", "ratio");
        } else if (kind === "pwr") {
          addMath("逆变电功率", "p_elec", "1.5 * (Vq * Iq_fbk)", "W");
        } else if (kind === "lpf") {
          addMath("Iq低通滤波", "iq_lpf", "lpf(Iq_fbk, 0.08)", "A");
        }
      });
    });

    document.getElementById("tbodyMath").addEventListener("change", (e) => {
      const t = e.target;
      const idx = Number(t.dataset.i);
      const fld = t.dataset.mcF;
      if (!fld || isNaN(idx)) return;
      const mc = state.profile.math_channels[idx];
      mc[fld] = t.type === "checkbox" ? t.checked : t.value;
      persistProfile(false);
    });

    document.getElementById("tbodyMath").addEventListener("click", (e) => {
      const del = e.target.closest("[data-del-mc]");
      if (del) {
        state.profile.math_channels.splice(Number(del.dataset.delMc), 1);
        persistProfile(false);
      }
    });

    // Page 4: Bookmarks, Serial Console & Copy AI Prompt
    document.getElementById("bookmarkList").addEventListener("click", async (e) => {
      const item = e.target.closest("[data-bm-t]");
      if (!item) return;
      const ts = parseFloat(item.dataset.bmT);
      const t1 = Math.max(0, ts - 0.35);
      const t2 = ts + 0.65;
      await invoke("set_cursors_cmd", { enabled: true, t1, t2 });
      state.followEnabled = false;
      state.customViewRange = [Math.max(0, ts - 1.5), ts + 2.5];
      syncTimebaseFollowUI();
      switchPage("scope");
      await requestViewportUpdate(true);
    });

    document.querySelectorAll("[data-quick-tx]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        try {
          const r = await invoke("serial_action", {
            args: { action: "send_command", command: btn.dataset.quickTx },
          });
          showToast(r.status === "sent" ? `已发送指令: ${btn.dataset.quickTx.trim()}` : "通信未连接");
          if (state.activePage === "terminal") refreshTerminalView();
        } catch (err) {
          showToast(String(err));
        }
      });
    });

    const sendManualTx = async () => {
      const inp = document.getElementById("inpSerialTx");
      const txt = inp.value.trim();
      if (!txt) return;
      try {
        const r = await invoke("serial_action", {
          args: { action: "send_command", command: txt + "\n" },
        });
        inp.value = "";
        showToast(r.status === "sent" ? `已发送: ${txt}` : "通信未连接");
        refreshTerminalView();
      } catch (err) {
        showToast(String(err));
      }
    };
    document.getElementById("btnSendTx").addEventListener("click", sendManualTx);
    document.getElementById("inpSerialTx").addEventListener("keydown", (e) => {
      if (e.key === "Enter") sendManualTx();
    });

    document.getElementById("btnClearConsole").addEventListener("click", async () => {
      await invoke("clear_console_cmd");
      refreshTerminalView();
    });

    document.getElementById("btnCopyAiPrompt").addEventListener("click", async () => {
      const txt = document.getElementById("txtAiPrompt").value;
      await navigator.clipboard.writeText(txt);
      showToast("已复制通用 AI / MCP 安装指令");
    });

    document.querySelectorAll("[data-copy-text]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const val = btn.dataset.copyText || "";
        if (val) {
          await navigator.clipboard.writeText(val);
          showToast(`已复制: ${val}`);
        }
      });
    });

    // Global Keyboard Shortcuts
    window.addEventListener("keydown", (e) => {
      const tag = (e.target && e.target.tagName) || "";
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      if (e.key === "Tab") {
        e.preventDefault();
        state.profile.right_drawer_open = !state.profile.right_drawer_open;
        persistProfile(false);
      } else if (e.key === " ") {
        e.preventDefault();
        document.getElementById("btnPause").click();
      }
    });

    window.addEventListener("resize", () => {
      scopeGroup.scheduleRender();
      if (state.profile) widgetCtrl.refreshLayoutLock();
      if (state.activePage === "lab") analysisCtrl.redrawCanvases();
    });

    // Listen for live AI / MCP mutations from Rust HTTP server!
    await listen("mcp-updated", async (ev) => {
      const kind = (ev.payload && ev.payload.kind) || "";
      if (kind === "profile") {
        const b = await invoke("get_bootstrap");
        state.profile = b.profile;
        rebuildScopePlotPanes();
        syncAllConfigViews();
        await requestViewportUpdate(true);
        if (state.activePage === "lab") await analysisCtrl.refresh();
      } else if (kind === "fit_all") {
        const b = await invoke("get_bootstrap");
        state.profile = b.profile;
        syncAllConfigViews();
        state.followEnabled = false;
        state.customViewRange = null;
        syncTimebaseFollowUI();
        await requestViewportUpdate(false);
      } else if (kind === "zoom_cursors") {
        const vp = await invoke("get_viewport", { pixelWidth: 400 });
        const lo = Math.min(vp.cursor_t1, vp.cursor_t2);
        const hi = Math.max(vp.cursor_t1, vp.cursor_t2);
        const pad = Math.max(0.2, (hi - lo) * 0.3);
        state.followEnabled = false;
        state.customViewRange = [Math.max(0, lo - pad), hi + pad];
        syncTimebaseFollowUI();
        await requestViewportUpdate(true);
      } else if (kind === "slices") {
        await analysisCtrl.refresh();
      } else {
        await requestViewportUpdate(true);
      }
    });

    // Listen for rich MCP UI navigation, scope viewport & static analysis view commands
    await listen("mcp-ui-cmd", async (ev) => {
      const cmd = ev.payload || {};
      if (cmd.page && document.getElementById(`page-${cmd.page}`)) {
        switchPage(cmd.page);
      }
      if (cmd.scope_view && typeof cmd.scope_view === "object") {
        const sv = cmd.scope_view;
        if (typeof sv.follow_enabled === "boolean") state.followEnabled = sv.follow_enabled;
        if (typeof sv.follow_anchor_ratio === "number") state.followAnchorRatio = sv.follow_anchor_ratio;
        if (typeof sv.follow_span_sec === "number") state.followSpanSec = sv.follow_span_sec;
        if (sv.fit_all) {
          state.followEnabled = false;
          state.customViewRange = null;
          syncTimebaseFollowUI();
          scopeGroup.resetZoomFull();
        } else if (typeof sv.t_min === "number" && typeof sv.t_max === "number" && sv.t_max > sv.t_min) {
          state.followEnabled = false;
          state.customViewRange = [sv.t_min, sv.t_max];
          syncTimebaseFollowUI();
          await requestViewportUpdate(true);
        } else {
          syncTimebaseFollowUI();
          await requestViewportUpdate(false);
        }
      }
      if (cmd.analysis_view) {
        if (state.activePage !== "lab") switchPage("lab");
        await analysisCtrl.applyRemoteState(cmd.analysis_view);
      }
    });

    // Listen for MCP Screenshot Capture Request
    await listen("request-screenshot", async (ev) => {
      const target = (ev.payload && ev.payload.target) || "full_window";
      if (target === "overlay" || target.startsWith("analysis_")) {
        if (state.activePage !== "lab") switchPage("lab");
        const tabMap = {
          analysis_overlay: "overlay",
          analysis_step: "step",
          analysis_fft: "fft",
          analysis_filter: "filter",
          analysis_xy: "xy",
        };
        if (tabMap[target]) {
          await analysisCtrl.applyRemoteState({ tab: tabMap[target] });
        } else {
          await analysisCtrl.refresh();
        }
      } else if (state.activePage !== "scope") {
        switchPage("scope");
        await new Promise((r) => setTimeout(r, 35));
      }
      const dataUrl = composeWorkspaceScreenshot(target);
      await invoke("submit_screenshot_data", { dataUrl, target });
    });

    // Paced ~50Hz smooth streaming loop (prevents 165Hz/240Hz IPC saturation & drops to 300ms idle poll when static!)
    const liveLoop = async () => {
      const tStart = performance.now();
      try {
        const chk = await invoke("check_data_seq");
        state.isStreaming = chk.is_streaming;
        state.paused = chk.paused;
        if (chk.data_seq !== state.lastDataSeq && !chk.paused) {
          await requestViewportUpdate(true);
          if (state.activePage === "terminal") refreshTerminalView();
        }
      } catch (_) {}
      const elapsed = performance.now() - tStart;
      const waitMs =
        state.isStreaming && !state.paused ? Math.max(8, Math.round(20 - elapsed)) : 300;
      setTimeout(liveLoop, waitMs);
    };
    setTimeout(liveLoop, 80);
  }

  function composeWorkspaceScreenshot(target) {
    const out = document.createElement("canvas");
    const cs = getComputedStyle(document.documentElement);
    const bg = cs.getPropertyValue("--bg-canvas").trim() || "#141416";
    const fg = cs.getPropertyValue("--text-primary").trim() || "#E6E7EB";

    if (target === "overlay" || target.startsWith("analysis_")) {
      const activePaneCvs = document.querySelector("#page-lab .anl-pane.active canvas");
      const src = activePaneCvs || document.getElementById("canvasOverlay");
      out.width = src.width || 1000;
      out.height = src.height || 500;
      const ctx = out.getContext("2d");
      ctx.drawImage(src, 0, 0);
      return out.toDataURL("image/png");
    }

    // Stack all active oscilloscope canvases cleanly into a high-res composite PNG
    const panes = Array.from(document.querySelectorAll("#plotsStack canvas"));
    if (panes.length === 0) {
      out.width = 800;
      out.height = 400;
      return out.toDataURL("image/png");
    }
    const w = panes[0].width || 1200;
    const totalH = panes.reduce((acc, c) => acc + (c.height || 300), 0) + 40;
    out.width = w;
    out.height = totalH;
    const ctx = out.getContext("2d");
    ctx.fillStyle = bg;
    ctx.fillRect(0, 0, w, totalH);
    ctx.fillStyle = fg;
    ctx.font = 'bold 16px "Segoe UI", sans-serif';
    ctx.fillText(
      `FluxScope  |  ${document.getElementById("sbSource").textContent}  |  ${
        document.getElementById("lblCursorDelta").textContent
      }`,
      18,
      26
    );
    let y = 40;
    for (const c of panes) {
      ctx.drawImage(c, 0, y);
      y += c.height;
    }
    return out.toDataURL("image/png");
  }

  window.addEventListener("DOMContentLoaded", init);
})();
