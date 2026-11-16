/* FluxScope v2.0 — Modular Right-Drawer Widget System & Configurator */

(function () {
  const WIDGET_TYPES = [
    {
      type: "enum_badge",
      colorArch: "multi",
      name: "状态枚举徽章",
      brief: "整数值映射为彩色状态文字",
      effect: "将离散整数实时翻译为「彩色圆点 + 状态文本」胶囊，未匹配值自动显示首行兜底状态。",
      vars: "运行状态机（待机/闭环/故障）、控制模式（速度环/电流环/位置环）、挡位状态、校准步骤号。",
    },
    {
      type: "bitmask_led",
      colorArch: "multi",
      name: "位掩码状态灯阵",
      brief: "按位解码或独立条件指示灯",
      effect: "以矩阵灯格或文字胶囊展示多路布尔状态；支持按整数二进制位（Bit 0~31）解析，或每盏灯独立写条件。",
      vars: "硬件/软件保护故障字、GPIO 输入输出电平、霍尔/限位开关信号、系统状态标志寄存器。",
    },
    {
      type: "linear_bar",
      colorArch: "single",
      name: "线性进度条",
      brief: "单向或零位双向横向比例条",
      effect: "横向填充条搭配可选数值与百分比读数；支持左端单向（Min→Max）及中间零位正负双向（-Max↔+Max）。",
      vars: "温度、母线电压、占空比、CPU 负载率、油门开度，或正负双向电流、正反转扭矩、控制偏差。",
    },
    {
      type: "num_grid",
      colorArch: "multi",
      name: "多通道数值网格",
      brief: "多路数字读数 + 极值与分级变色",
      effect: "1~3 列紧凑数值卡片组，单格可选显示峰值 Pk、谷值 Min 或峰峰值 P-P，并支持单格多优先级超限变色。",
      vars: "转速、电流、电压、功率、温度、频率等需要同屏集中监控的任意多路标量。",
    },
    {
      type: "arc_gauge",
      colorArch: "single",
      name: "圆弧表盘",
      brief: "240° 环形量程弧 + 可选参考针",
      effect: "240° 环形进度表盘与中心读数，可绑定第二路参考通道在外圈绘制目标游标，支持单向或顶部零位双向。",
      vars: "电机转速（实测+目标给定双针）、相电流幅值、输出转矩、机械功率、负载率。",
    },
    {
      type: "three_phase",
      colorArch: "multi",
      name: "多路立柱图",
      brief: "2~6 路并排立柱幅值对比",
      effect: "多根垂直立柱实时升降对比，支持底部基线或中间零位正负双向展开。",
      vars: "三相占空比（Ua/Ub/Uc）、三相电流、多电机均衡负载、多点温度分布对比。",
    },
    {
      type: "angle_compass",
      colorArch: "single",
      name: "极坐标双针罗盘",
      brief: "360° 角度指针与双通道夹角差",
      effect: "360° 圆盘指针，内置角度/弧度/编码器脉冲换算；绑定对比通道时同盘双针显示并自动解算夹角差 Δθ。",
      vars: "转子电角度/机械角度、无感观测角度 vs 编码器角度误差、云台航向角、锁相环相位。",
    },
    {
      type: "xy_lissajous",
      colorArch: "single",
      name: "X-Y 轨迹图",
      brief: "双通道二维平面动态尾迹",
      effect: "以任意两路通道作为 X/Y 轴实时绘制李萨如平面轨迹，支持固定对称量程或自动包络缩放。",
      vars: "Uα-Uβ 电压圆、Iα-Iβ 电流圆、正余弦编码器（Sin/Cos）正交度校验、转速-转矩运行轨迹。",
    },
    {
      type: "spark_stat",
      colorArch: "single",
      name: "微趋势统计卡",
      brief: "实时读数 + 滚动微波形 + 峰峰值",
      effect: "在单行卡片内展示近期滚动趋势曲线（Sparkline），实时统计窗口内峰峰值 P-P 或极值区间。",
      vars: "母线电压纹波、稳态转速波动、传感器底噪、供电跌落监测、温度慢变趋势。",
    },
    {
      type: "quick_visibility",
      colorArch: "none",
      name: "曲线显隐面板",
      brief: "主示波器通道波形快捷勾选开关",
      effect: "多列紧凑勾选盒，一键切换示波器各通道曲线的显示与隐藏，可选附带实时尾数。",
      vars: "当前已绑定至主示波窗口的硬件通道与派生公式通道。",
    },
    {
      type: "svpwm_hex",
      colorArch: "single",
      name: "空间矢量六边形",
      brief: "六边形边界 + 线性圆 + 扇区调制比",
      effect: "绘制六边形逆变边界与内切线性圆，实时指示合成矢量方向、扇区号（S1~S6）与调制深度 M。",
      vars: "Vd/Vq + 电角度反 Park 换算、三相占空比重构，或 Uα/Uβ 静止轴电压与母线电压（监控弱磁与过调制）。",
    },
    {
      type: "id_iq_circle",
      colorArch: "single",
      name: "四象限矢量圆",
      brief: "正交四象限工作点 + 圆形限幅包络",
      effect: "四象限十字坐标配圆形包络边界，实时绘制二维矢量落点与幅值裕度。",
      vars: "Id-Iq 电流工作点（观察 MTPA / 弱磁轨迹与最大电流圆）、Vd-Vq 电压矢量、二维倾角或力矢量。",
    },
    {
      type: "isr_stacked",
      colorArch: "multi",
      name: "分段占比堆栈条",
      brief: "多阶段累加占总预算比例条",
      effect: "单条横向多色分段堆栈，直观显示各子项数值、总和占预算上限的百分比及剩余裕量。",
      vars: "控制中断各函数分段耗时（采样/观测/调制）占周期预算比例、多任务 CPU 占用拆解、功耗分布。",
    },
    {
      type: "ref_fbk_err",
      colorArch: "single",
      name: "双轨跟随误差条",
      brief: "双通道平行对比 + 实时差值 Δ",
      effect: "上下双轨同步展示目标与实测幅值，右上角实时计算并高亮跟随误差 Δ = 反馈 - 给定。",
      vars: "转速给定与反馈、电流环给定与反馈、伺服位置指令与实测、目标电压与实际母线电压。",
    },
  ];

  // Zero-eval Pure Recursive-Descent Expression Evaluator
  const EXPR_FUNCS = {
    abs: (a) => Math.abs(a[0] ?? 0),
    sqrt: (a) => Math.sqrt(Math.max(0, a[0] ?? 0)),
    hypot: (a) => Math.hypot(...a),
    min: (a) => Math.min(...a),
    max: (a) => Math.max(...a),
    sin: (a) => Math.sin(a[0] ?? 0),
    cos: (a) => Math.cos(a[0] ?? 0),
    atan2: (a) => Math.atan2(a[0] ?? 0, a[1] ?? 1),
  };

  function esc(s) {
    return String(s ?? "")
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  function mkRule(opts) {
    return Object.assign(
      {
        mode: "builder",
        leftVar: "val",
        op: ">=",
        rightMode: "const",
        rightConst: 50,
        rightVar: "CH0",
        expr: "val >= 80",
        color: "#EF4444",
        alertFrame: false,
        frameColor: "#EF4444",
      },
      opts || {}
    );
  }

  function tokenizeExpr(raw) {
    const s = String(raw || "")
      .replace(/＞/g, ">")
      .replace(/＜/g, "<")
      .replace(/＝/g, "=")
      .replace(/！/g, "!")
      .replace(/（/g, "(")
      .replace(/）/g, ")")
      .replace(/，/g, ",")
      .replace(/＆/g, "&")
      .replace(/｜/g, "|");
    const tokens = [];
    let i = 0;
    while (i < s.length) {
      const ch = s[i];
      if (/\s/.test(ch)) {
        i++;
        continue;
      }
      const rest = s.slice(i);
      const hexM = rest.match(/^0[xX][0-9a-fA-F]+/);
      if (hexM) {
        tokens.push({ t: "num", v: parseInt(hexM[0], 16) });
        i += hexM[0].length;
        continue;
      }
      const numM = rest.match(/^(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?/);
      if (numM) {
        tokens.push({ t: "num", v: parseFloat(numM[0]) });
        i += numM[0].length;
        continue;
      }
      const two = s.slice(i, i + 2);
      if (["&&", "||", ">=", "<=", "==", "!=", "<<", ">>"].includes(two)) {
        tokens.push({ t: "op", v: two });
        i += 2;
        continue;
      }
      if ("+-*/%><=!~&|^(),".includes(ch)) {
        tokens.push({ t: "op", v: ch === "=" ? "==" : ch });
        i++;
        continue;
      }
      const idM = rest.match(/^[a-zA-Z_][a-zA-Z0-9_]*/);
      if (idM) {
        tokens.push({ t: "id", v: idM[0] });
        i += idM[0].length;
        continue;
      }
      throw new Error(`非法字符 ${ch}`);
    }
    return tokens;
  }

  function createWidgetController(hooks) {
    // hooks: { getProfile, getActivePage, onPersist(rebuildPlots), onDrawerResized }
    const rt = {
      selectedId: null,
      activeHelpInput: null,
      vals: {},
      peaks: {},
      mins: {},
      tails: {},
      saveTimer: null,
    };

    function prof() {
      return hooks.getProfile();
    }

    function schedulePersist() {
      clearTimeout(rt.saveTimer);
      rt.saveTimer = setTimeout(() => {
        hooks.onPersist(false);
      }, 240);
    }

    function commitPersistNow() {
      clearTimeout(rt.saveTimer);
      hooks.onPersist(false);
    }

    function getChannelsCatalog() {
      const p = prof();
      if (!p) return [];
      const list = [];
      (p.channels || []).forEach((c) => {
        list.push({
          key: `CH${c.index}`,
          alias: c.alias || "",
          name: c.name || `CH${c.index}`,
          unit: c.unit || "",
          color: c.color || "#3B82F6",
          dec: c.decimals ?? 2,
          isMath: false,
          rawIdx: c.index,
          visible: !!c.visible,
          plot_id: c.plot_id || "none",
        });
      });
      (p.math_channels || []).forEach((m, idx) => {
        list.push({
          key: m.id,
          alias: m.alias || "",
          name: `∑ ${m.name}`,
          unit: m.unit || "",
          color: m.color || "#EC4899",
          dec: m.decimals ?? 3,
          isMath: true,
          rawIdx: idx,
          visible: !!m.visible,
          plot_id: m.plot_id || "none",
        });
      });
      return list;
    }

    function chMeta(key) {
      const cat = getChannelsCatalog();
      return (
        cat.find((c) => c.key === key || (c.alias && c.alias === key)) ||
        cat[0] || { key: key || "CH0", name: key || "CH0", unit: "", color: "#3B82F6", dec: 2 }
      );
    }

    function ensureWidgetsInitialized() {
      const p = prof();
      if (!p) return [];
      if (!Array.isArray(p.drawer_widgets)) {
        p.drawer_widgets = [];
      }
      if (p.drawer_widgets.length === 0 && !p.widgets_initialized) {
        p.drawer_widgets = buildClientFallbackWidgets(p);
        p.widgets_initialized = true;
      }
      if (
        !rt.selectedId ||
        !p.drawer_widgets.some((w) => String(w.id) === String(rt.selectedId))
      ) {
        rt.selectedId = p.drawer_widgets[0] ? String(p.drawer_widgets[0].id) : null;
      }
      return p.drawer_widgets;
    }

    function buildClientFallbackWidgets(p) {
      const sd = p.status_decode || {};
      const stCh = `CH${sd.state_channel ?? 6}`;
      const fltCh = `CH${sd.fault_channel ?? 7}`;
      const isrCh = `CH${sd.isr_channel ?? 20}`;
      const isrMax = Math.max(100, Number(sd.isr_full_scale) || 3000);
      const enumVec = Object.entries(sd.state_enum_map || {})
        .sort((a, b) => Number(a[0]) - Number(b[0]))
        .map(([k, v]) => ({ k, label: v.label || k, color: v.color || "#71717A" }));
      const fltVec = Object.entries(sd.fault_map || {})
        .sort((a, b) => Number(a[0]) - Number(b[0]))
        .map(([k, lbl]) => ({ key: k, label: String(lbl), color: "#EF4444", expr: "" }));
      const cards = [];
      (p.channels || []).forEach((c) => {
        if (c.show_card && c.index !== sd.state_channel && c.index !== sd.fault_channel && c.index !== sd.isr_channel) {
          cards.push({ ch: `CH${c.index}`, dec: c.decimals ?? 2, sub: "none", rules: [] });
        }
      });
      (p.math_channels || []).forEach((m) => {
        if (m.show_card) cards.push({ ch: m.id, dec: m.decimals ?? 3, sub: "none", rules: [] });
      });

      return [
        {
          id: "w_enum_1",
          type: "enum_badge",
          enabled: sd.enabled !== false && sd.state_enabled !== false,
          showHeader: true,
          title: "状态枚举徽章",
          widthPct: 50,
          height: null,
          channel: stCh,
          unmatchedLabel: "未知状态",
          unmatchedColor: "#8B5CF6",
          enumMap: enumVec,
          frameRules: [mkRule({ mode: "builder", leftVar: stCh, op: "==", rightMode: "const", rightConst: 6, color: "#EF4444" })],
        },
        {
          id: "w_bar_1",
          type: "linear_bar",
          enabled: sd.enabled !== false && sd.isr_enabled !== false,
          showHeader: true,
          title: "线性进度条",
          widthPct: 50,
          height: null,
          channel: isrCh,
          unitOverride: sd.isr_unit || "cyc",
          decimals: 0,
          min: 0,
          max: isrMax,
          originMode: "left",
          showValue: true,
          showPct: true,
          normalColorMode: "fixed",
          baseColor: "#10B981",
          colorRules: [
            mkRule({ mode: "builder", leftVar: "val", op: ">=", rightMode: "const", rightConst: Math.round(isrMax * 0.8), color: "#EF4444", alertFrame: true, frameColor: "#EF4444" }),
            mkRule({ mode: "builder", leftVar: "val", op: ">=", rightMode: "const", rightConst: Math.round(isrMax * 0.6), color: "#F59E0B" }),
          ],
        },
        {
          id: "w_led_1",
          type: "bitmask_led",
          enabled: sd.enabled !== false && sd.fault_enabled !== false,
          showHeader: true,
          title: "位掩码状态灯阵",
          widthPct: 100,
          height: null,
          channel: fltCh,
          decodeMode: "bitmask",
          displayStyle: "compact_pill",
          gridCols: 2,
          okText: "0x0000 正常",
          items: fltVec,
          frameRules: [mkRule({ mode: "builder", leftVar: fltCh, op: ">", rightMode: "const", rightConst: 0, color: "#EF4444" })],
        },
        {
          id: "w_num_1",
          type: "num_grid",
          enabled: true,
          showHeader: true,
          title: "多通道数值网格",
          widthPct: 100,
          height: null,
          cols: 2,
          cards,
          frameRules: [],
        },
        {
          id: "w_qv_1",
          type: "quick_visibility",
          enabled: true,
          showHeader: true,
          title: "曲线显隐面板",
          widthPct: 100,
          height: null,
          cols: 1,
          showLiveNum: true,
        },
      ];
    }

    function createDefaultWidgetInstance(typeKey) {
      const meta = WIDGET_TYPES.find((t) => t.type === typeKey) || WIDGET_TYPES[0];
      const uid = `w_${Date.now().toString(36)}_${Math.floor(Math.random() * 999)}`;
      const cat = getChannelsCatalog();
      const ch0 = (cat[0] && cat[0].key) || "CH0";
      const ch1 = (cat[1] && cat[1].key) || ch0;

      const base = {
        id: uid,
        type: typeKey,
        enabled: true,
        showHeader: true,
        title: meta.name,
        widthPct: ["arc_gauge", "three_phase", "angle_compass", "xy_lissajous", "svpwm_hex", "id_iq_circle"].includes(typeKey) ? 50 : 100,
        height: ["arc_gauge", "three_phase", "angle_compass", "xy_lissajous", "svpwm_hex", "id_iq_circle"].includes(typeKey) ? 158 : null,
      };

      switch (typeKey) {
        case "enum_badge":
          return Object.assign(base, {
            widthPct: 50,
            channel: "CH6",
            unmatchedLabel: "未知状态",
            unmatchedColor: "#8B5CF6",
            enumMap: [
              { k: "0", label: "IDLE 待机", color: "#71717A" },
              { k: "4", label: "RUN 运行", color: "#10B981" },
              { k: "6", label: "FAULT 保护", color: "#EF4444" },
            ],
            frameRules: [],
          });
        case "bitmask_led":
          return Object.assign(base, {
            channel: "CH7",
            decodeMode: "bitmask",
            displayStyle: "led_grid",
            gridCols: 2,
            okText: "0x0000 正常",
            items: [
              { key: "0", label: "指示 0", color: "#EF4444", expr: "CH0 > 0" },
              { key: "1", label: "指示 1", color: "#F59E0B", expr: "CH1 > 0" },
            ],
            frameRules: [],
          });
        case "linear_bar":
          return Object.assign(base, {
            channel: ch0,
            unitOverride: "",
            decimals: 1,
            min: 0,
            max: 100,
            originMode: "left",
            showValue: true,
            showPct: true,
            normalColorMode: "channel",
            baseColor: "#10B981",
            colorRules: [],
          });
        case "num_grid":
          return Object.assign(base, {
            cols: 2,
            cards: [
              { ch: ch0, dec: 2, sub: "none", rules: [] },
              { ch: ch1, dec: 2, sub: "peak", rules: [] },
            ],
            frameRules: [],
          });
        case "arc_gauge":
          return Object.assign(base, {
            channel: ch1,
            refChannel: ch0,
            min: 0,
            max: 3600,
            originMode: "left",
            decimals: 0,
            unitOverride: "",
            normalColorMode: "channel",
            baseColor: "#3B82F6",
            colorRules: [],
          });
        case "three_phase":
          return Object.assign(base, {
            min: 0,
            max: 100,
            baselineMode: "bottom",
            bars: [
              { label: "A", ch: "CH10", color: "#3B82F6" },
              { label: "B", ch: "CH11", color: "#10B981" },
              { label: "C", ch: "CH12", color: "#F59E0B" },
            ],
            frameRules: [],
          });
        case "angle_compass":
          return Object.assign(base, {
            channel: ch0,
            compareChannel: "",
            angleUnit: "deg360",
            encoderCpr: 16384,
            zeroDir: "top",
            normalColorMode: "fixed",
            baseColor: "#0EA5E9",
            colorRules: [],
          });
        case "xy_lissajous":
          return Object.assign(base, {
            xChannel: "CH8",
            yChannel: "CH9",
            scaleMode: "auto",
            fixedMax: 50,
            tailPoints: 64,
            normalColorMode: "fixed",
            baseColor: "#8B5CF6",
            colorRules: [],
          });
        case "spark_stat":
          return Object.assign(base, {
            channel: "CH26",
            windowPts: 60,
            statType: "pp",
            normalColorMode: "channel",
            baseColor: "#10B981",
            colorRules: [],
          });
        case "quick_visibility":
          return Object.assign(base, { cols: 1, showLiveNum: true });
        case "svpwm_hex":
          return Object.assign(base, {
            inputSource: "dq_theta",
            vdCh: "CH8",
            vqCh: "CH9",
            thetaCh: "CH14",
            vaCh: "CH8",
            vbCh: "CH9",
            dutyACh: "CH10",
            dutyBCh: "CH11",
            dutyCCh: "CH12",
            vbusCh: "CH26",
            normalColorMode: "fixed",
            baseColor: "#3B82F6",
            colorRules: [],
          });
        case "id_iq_circle":
          return Object.assign(base, {
            idCh: "CH2",
            iqCh: "CH3",
            imaxFixed: 120,
            normalColorMode: "fixed",
            baseColor: "#10B981",
            colorRules: [],
          });
        case "isr_stacked":
          return Object.assign(base, {
            budget: 3000,
            budgetUnit: "cyc",
            totalCh: "CH20",
            stages: [
              { label: "阶段1", ch: "CH16", color: "#3B82F6" },
              { label: "阶段2", ch: "CH17", color: "#8B5CF6" },
              { label: "阶段3", ch: "CH19", color: "#10B981" },
            ],
            frameRules: [],
          });
        case "ref_fbk_err":
        default:
          return Object.assign(base, {
            refCh: ch0,
            fbkCh: ch1,
            refColor: "#3B82F6",
            min: 0,
            max: 3600,
            normalColorMode: "fixed",
            baseColor: "#10B981",
            colorRules: [],
          });
      }
    }

    // ==================== EXPRESSION & RULE EVALUATION ====================
    function readChannelVal(keyOrAlias) {
      if (!keyOrAlias) return 0;
      if (rt.vals[keyOrAlias] !== undefined) return rt.vals[keyOrAlias];
      const cat = getChannelsCatalog();
      const hit = cat.find((c) => c.alias === keyOrAlias || c.key === keyOrAlias);
      return hit ? rt.vals[hit.key] ?? 0 : 0;
    }

    function evalCustomStr(exprStr, selfVal) {
      if (!exprStr || !String(exprStr).trim()) return { ok: true, hit: false };
      try {
        const tokens = tokenizeExpr(exprStr);
        if (tokens.length === 0) return { ok: true, hit: false };
        const varMap = {
          val: selfVal,
          abs_val: Math.abs(selfVal),
          pi: Math.PI,
          true: 1,
          false: 0,
        };
        const cat = getChannelsCatalog();
        for (const c of cat) {
          const v = rt.vals[c.key] ?? 0;
          varMap[c.key.toLowerCase()] = v;
          if (c.alias) varMap[c.alias.toLowerCase()] = v;
        }
        for (let k = 0; k < 64; k++) {
          const ck = "ch" + k;
          if (!(ck in varMap)) varMap[ck] = rt.vals["CH" + k] ?? 0;
        }

        let pos = 0;
        const peek = () => tokens[pos];
        const eatOp = (op) =>
          tokens[pos] && tokens[pos].t === "op" && tokens[pos].v === op ? (pos++, true) : false;

        function parseOr() {
          let l = parseAnd();
          while (eatOp("||")) {
            const r = parseAnd();
            l = Boolean(l) || Boolean(r) ? 1 : 0;
          }
          return l;
        }
        function parseAnd() {
          let l = parseBitOr();
          while (eatOp("&&")) {
            const r = parseBitOr();
            l = Boolean(l) && Boolean(r) ? 1 : 0;
          }
          return l;
        }
        function parseBitOr() {
          let l = parseBitXor();
          while (eatOp("|")) l = Math.trunc(l) | Math.trunc(parseBitXor());
          return l;
        }
        function parseBitXor() {
          let l = parseBitAnd();
          while (eatOp("^")) l = Math.trunc(l) ^ Math.trunc(parseBitAnd());
          return l;
        }
        function parseBitAnd() {
          let l = parseEq();
          while (eatOp("&")) l = Math.trunc(l) & Math.trunc(parseEq());
          return l;
        }
        function parseEq() {
          let l = parseRel();
          while (peek() && peek().t === "op" && (peek().v === "==" || peek().v === "!=")) {
            const op = tokens[pos++].v,
              r = parseRel();
            l = (op === "==" ? Math.abs(l - r) < 1e-5 : Math.abs(l - r) >= 1e-5) ? 1 : 0;
          }
          return l;
        }
        function parseRel() {
          let l = parseShift();
          while (peek() && peek().t === "op" && ["<", "<=", ">", ">="].includes(peek().v)) {
            const op = tokens[pos++].v,
              r = parseShift();
            l = (op === "<" ? l < r : op === "<=" ? l <= r : op === ">" ? l > r : l >= r) ? 1 : 0;
          }
          return l;
        }
        function parseShift() {
          let l = parseAdd();
          while (peek() && peek().t === "op" && (peek().v === "<<" || peek().v === ">>")) {
            const op = tokens[pos++].v,
              r = parseAdd();
            l = op === "<<" ? Math.trunc(l) << Math.trunc(r) : Math.trunc(l) >> Math.trunc(r);
          }
          return l;
        }
        function parseAdd() {
          let l = parseMul();
          while (peek() && peek().t === "op" && (peek().v === "+" || peek().v === "-")) {
            const op = tokens[pos++].v,
              r = parseMul();
            l = op === "+" ? l + r : l - r;
          }
          return l;
        }
        function parseMul() {
          let l = parseUnary();
          while (peek() && peek().t === "op" && ["*", "/", "%"].includes(peek().v)) {
            const op = tokens[pos++].v,
              r = parseUnary();
            l = op === "*" ? l * r : op === "/" ? (r === 0 ? 0 : l / r) : r === 0 ? 0 : l % r;
          }
          return l;
        }
        function parseUnary() {
          if (eatOp("+")) return +parseUnary();
          if (eatOp("-")) return -parseUnary();
          if (eatOp("!")) return !parseUnary() ? 1 : 0;
          if (eatOp("~")) return ~Math.trunc(parseUnary());
          return parsePrimary();
        }
        function parsePrimary() {
          const tk = peek();
          if (!tk) throw new Error("未写完");
          if (tk.t === "num") {
            pos++;
            return tk.v;
          }
          if (eatOp("(")) {
            const v = parseOr();
            if (!eatOp(")")) throw new Error("缺 )");
            return v;
          }
          if (tk.t === "id") {
            pos++;
            const name = tk.v.toLowerCase();
            if (eatOp("(")) {
              const fn = EXPR_FUNCS[name];
              if (!fn) throw new Error(`未知函数 ${tk.v}`);
              const args = [];
              if (!eatOp(")")) {
                args.push(parseOr());
                while (eatOp(",")) args.push(parseOr());
                if (!eatOp(")")) throw new Error("缺 )");
              }
              return fn(args);
            }
            if (!(name in varMap)) throw new Error(`未知变量 ${tk.v}`);
            return varMap[name];
          }
          throw new Error("语法错");
        }

        const out = parseOr();
        if (pos < tokens.length) throw new Error(`多余 ${tokens[pos].v}`);
        return { ok: true, hit: Boolean(out) };
      } catch (err) {
        return { ok: false, hit: false, err: err.message || "语法错" };
      }
    }

    function readVarValue(varKey, selfVal) {
      if (varKey === "val") return selfVal;
      if (varKey === "abs_val") return Math.abs(selfVal);
      return readChannelVal(varKey);
    }

    function evaluateConditionRule(r, selfVal) {
      if (!r) return { ok: true, hit: false };
      if (r.mode === "custom") return evalCustomStr(r.expr, selfVal);
      const lhs = readVarValue(r.leftVar || "val", selfVal);
      const rhs =
        r.rightMode === "var"
          ? readVarValue(r.rightVar || "CH0", selfVal)
          : Number(r.rightConst ?? 0);
      let hit = false;
      switch (r.op) {
        case ">":
          hit = lhs > rhs;
          break;
        case ">=":
          hit = lhs >= rhs;
          break;
        case "<":
          hit = lhs < rhs;
          break;
        case "<=":
          hit = lhs <= rhs;
          break;
        case "==":
          hit = Math.abs(lhs - rhs) < 1e-5;
          break;
        case "!=":
          hit = Math.abs(lhs - rhs) >= 1e-5;
          break;
      }
      return { ok: true, hit };
    }

    function getRuleArray(w, path, ensureInit) {
      if (path === "colorRules" || path === "frameRules") {
        if (!w[path] && ensureInit) w[path] = [];
        return w[path] || [];
      }
      if (path && path.startsWith("cardRules_")) {
        const ci = Number(path.slice("cardRules_".length));
        const c = w.cards && w.cards[ci];
        if (!c) return [];
        if (!c.rules && ensureInit) c.rules = [];
        return c.rules || [];
      }
      return [];
    }

    function evaluateSubCardWarn(c, selfVal) {
      const list = c.rules || [];
      for (let i = 0; i < list.length; i++) {
        const res = evaluateConditionRule(list[i], selfVal);
        if (res.ok && res.hit) {
          return { hit: true, color: list[i].color || "#EF4444", winnerIdx: i };
        }
      }
      return { hit: false, color: null, winnerIdx: -1 };
    }

    function resolveSingleWidgetAppearance(w, selfVal) {
      const defCol =
        w.normalColorMode === "channel"
          ? chMeta(w.channel || w.fbkCh || w.xChannel || w.vdCh).color
          : w.baseColor || "#3B82F6";
      if (Array.isArray(w.colorRules)) {
        for (const r of w.colorRules) {
          if (evaluateConditionRule(r, selfVal).hit) {
            return {
              color: r.color || "#EF4444",
              frameColor: r.alertFrame ? r.frameColor || r.color || "#EF4444" : null,
            };
          }
        }
      }
      return { color: defCol, frameColor: null };
    }

    function resolveMultiWidgetFrameColor(w, selfVal) {
      if (Array.isArray(w.frameRules)) {
        for (const r of w.frameRules) {
          if (evaluateConditionRule(r, selfVal).hit) return r.color || "#EF4444";
        }
      }
      return null;
    }

    // ==================== FLOATING `?` POPOVERS ====================
    function openAutoFlipPop(popEl, anchorBtn) {
      document.getElementById("exprHelpPop").classList.remove("open");
      document.getElementById("widgetGuidePop").classList.remove("open");
      popEl.style.visibility = "hidden";
      popEl.classList.add("open");
      const rect = anchorBtn.getBoundingClientRect();
      const popW = popEl.offsetWidth || 376;
      const popH = popEl.offsetHeight || 220;
      let left = rect.left + popW <= window.innerWidth - 12 ? rect.left : rect.right - popW;
      left = Math.max(12, Math.min(window.innerWidth - popW - 12, left));
      let top = rect.bottom + 7;
      if (top + popH > window.innerHeight - 12) top = rect.top - popH - 7;
      top = Math.max(12, Math.min(window.innerHeight - popH - 12, top));
      popEl.style.left = left + "px";
      popEl.style.top = top + "px";
      popEl.style.visibility = "visible";
    }

    function showWidgetGuide(typeKey, anchorBtn) {
      const meta = WIDGET_TYPES.find((t) => t.type === typeKey);
      if (!meta) return;
      document.getElementById("wgPopTitle").textContent = meta.name;
      document.getElementById("wgPopEffect").textContent = meta.effect;
      document.getElementById("wgPopVars").textContent = meta.vars;
      openAutoFlipPop(document.getElementById("widgetGuidePop"), anchorBtn);
    }

    // ==================== CONFIG PAGE: INSTANCE LIST & INSPECTOR ====================
    function chOpts(curVal, allowEmpty) {
      const cat = getChannelsCatalog();
      return (
        (allowEmpty ? `<option value="">-- 无 --</option>` : ``) +
        cat
          .map(
            (c) =>
              `<option value="${esc(c.key)}" ${c.key === curVal ? "selected" : ""}>${esc(
                c.key
              )} · ${esc(c.name)}</option>`
          )
          .join("")
      );
    }

    function varSelectOpts(curVal, includeValSelf) {
      let s = "";
      if (includeValSelf) {
        s += `<option value="val" ${curVal === "val" ? "selected" : ""}>当前值 (val)</option>`;
        s += `<option value="abs_val" ${curVal === "abs_val" ? "selected" : ""}>绝对值 |val|</option>`;
      }
      s += getChannelsCatalog()
        .map(
          (c) =>
            `<option value="${esc(c.key)}" ${c.key === curVal ? "selected" : ""}>${esc(
              c.key
            )} · ${esc(c.name)}</option>`
        )
        .join("");
      return s;
    }

    function renderRuleCardsHTML(rulesArr, arrName, roleMode) {
      const list = rulesArr || [];
      const total = list.length;
      const isSingle = roleMode === "single";

      return list
        .map((r, ri) => {
          const isCust = r.mode === "custom";
          const prioOpts = list
            .map((_, idx) => {
              const tag = idx === 0 && total > 1 ? `P1 最高` : `P${idx + 1}`;
              return `<option value="${idx}" ${idx === ri ? "selected" : ""}>${tag}</option>`;
            })
            .join("");

          return `
            <div class="wcfg-cond-card" data-rule-card="${arrName}:${ri}" style="--hit-col:${esc(
            r.color || "#EF4444"
          )}">
              <div style="display:flex;align-items:center;justify-content:space-between;gap:6px;flex-wrap:wrap">
                <div style="display:flex;align-items:center;gap:5px;flex-wrap:wrap">
                  <select class="wcfg-prio-sel mono" data-rprio-sel="${arrName}:${ri}" title="优先级（P1最高）">${prioOpts}</select>
                  ${
                    total > 1
                      ? `<button class="wcfg-prio-btn" data-rprio-move="${arrName}:${ri}:-1" ${
                          ri === 0 ? "disabled" : ""
                        } title="提高优先级">↑</button>
                         <button class="wcfg-prio-btn" data-rprio-move="${arrName}:${ri}:1" ${
                          ri === total - 1 ? "disabled" : ""
                        } title="降低优先级">↓</button>`
                      : ``
                  }
                  <div class="wcfg-mode-tabs">
                    <button class="wcfg-mode-btn ${!isCust ? "active" : ""}" data-rmode="${arrName}:${ri}:builder">下拉</button>
                    <button class="wcfg-mode-btn ${isCust ? "active" : ""}" data-rmode="${arrName}:${ri}:custom">表达式</button>
                  </div>
                  <span class="wcfg-hit-badge mono" data-hit-badge="${arrName}:${ri}">--</span>
                </div>

                <div style="display:flex;align-items:center;gap:6px">
                  <span style="font-size:11.5px;color:var(--text-secondary)">颜色</span>
                  <input type="color" data-rarr="${arrName}" data-ri="${ri}" data-rk="color" value="${esc(
            r.color || "#EF4444"
          )}" />
                  ${
                    isSingle
                      ? `<label style="display:flex;align-items:center;gap:3px;font-size:11.5px;cursor:pointer">
                           <input type="checkbox" data-rbool="${arrName}:${ri}:alertFrame" ${
                          r.alertFrame ? "checked" : ""
                        } /> 外框
                         </label>
                         ${
                           r.alertFrame
                             ? `<input type="color" data-rarr="${arrName}" data-ri="${ri}" data-rk="frameColor" value="${esc(
                                 r.frameColor || r.color || "#EF4444"
                               )}" />`
                             : ``
                         }`
                      : ``
                  }
                  <button class="btn btn-icon btn-danger-outline" data-rdel="${arrName}:${ri}" style="height:21px;min-width:22px;padding:0 5px">✕</button>
                </div>
              </div>

              ${
                !isCust
                  ? `<div style="display:grid;grid-template-columns:1.25fr 76px 88px 1.15fr;gap:6px;align-items:center">
                       <select data-rarr="${arrName}" data-ri="${ri}" data-rk="leftVar">${varSelectOpts(
                      r.leftVar || "val",
                      true
                    )}</select>
                       <select class="mono" data-rarr="${arrName}" data-ri="${ri}" data-rk="op">
                         ${[">", ">=", "<", "<=", "==", "!="]
                           .map(
                             (op) =>
                               `<option value="${op}" ${
                                 r.op === op ? "selected" : ""
                               }>${esc(op)}</option>`
                           )
                           .join("")}
                       </select>
                       <select data-rarr="${arrName}" data-ri="${ri}" data-rk="rightMode" data-rerender-ins="1">
                         <option value="const" ${r.rightMode !== "var" ? "selected" : ""}>数值</option>
                         <option value="var" ${r.rightMode === "var" ? "selected" : ""}>通道</option>
                       </select>
                       ${
                         r.rightMode === "var"
                           ? `<select data-rarr="${arrName}" data-ri="${ri}" data-rk="rightVar">${varSelectOpts(
                               r.rightVar || "CH0",
                               false
                             )}</select>`
                           : `<input type="number" step="any" class="mono" data-rarr-num="${arrName}:${ri}:rightConst" value="${
                               r.rightConst ?? 0
                             }" />`
                       }
                     </div>`
                  : `<div style="display:flex;align-items:center;gap:6px">
                       <input type="text" class="mono" style="flex:1" data-rarr="${arrName}" data-ri="${ri}" data-rk="expr" value="${esc(
                      r.expr || ""
                    )}" placeholder="如 val >= 80 || abs(CH0 - CH1) > 200" />
                       <button class="wcfg-q-btn" data-open-help="${arrName}:${ri}" title="表达式语法">?</button>
                     </div>`
              }
            </div>`;
        })
        .join("");
    }

    function renderInstanceList() {
      const el = document.getElementById("widgetInstList");
      if (!el) return;
      const widgets = ensureWidgetsInitialized();
      el.innerHTML = widgets
        .map((w, idx) => {
          const tMeta = WIDGET_TYPES.find((t) => t.type === w.type);
          const isSel = String(w.id) === String(rt.selectedId);
          return `
            <div class="wcfg-inst-item ${isSel ? "selected" : ""}" data-sel-wid="${esc(w.id)}" style="cursor:grab">
              <div style="display:flex;align-items:center;gap:6px;overflow:hidden">
                <span class="tb-label" style="cursor:grab;font-size:12px;letter-spacing:-1px" title="按住上下拖动排序">⋮⋮</span>
                <input type="checkbox" data-toggle-wid="${esc(w.id)}" ${
            w.enabled !== false ? "checked" : ""
          } />
                <div style="overflow:hidden">
                  <div style="font-weight:600;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${esc(
                    w.title
                  )}</div>
                  <div style="font-size:11px;color:var(--text-muted)">${esc(
                    tMeta ? tMeta.name : w.type
                  )}</div>
                </div>
              </div>
              <div style="display:flex;align-items:center;gap:3px;flex-shrink:0">
                <button class="wcfg-prio-btn" data-move-wid="${esc(w.id)}:-1" ${
            idx === 0 ? "disabled" : ""
          } title="上移">↑</button>
                <button class="wcfg-prio-btn" data-move-wid="${esc(w.id)}:1" ${
            idx === widgets.length - 1 ? "disabled" : ""
          } title="下移">↓</button>
                <button class="wcfg-q-btn" data-wg-help="${esc(w.type)}" title="查看适用变量与显示说明">?</button>
                <button class="btn btn-icon btn-danger-outline" data-del-wid="${esc(
                  w.id
                )}" style="height:21px;min-width:22px;padding:0 5px" title="删除">✕</button>
              </div>
            </div>`;
        })
        .join("");
    }

    function renderInspector() {
      const pane = document.getElementById("widgetInspectorPane");
      if (!pane) return;
      const widgets = ensureWidgetsInitialized();
      const w = widgets.find((x) => String(x.id) === String(rt.selectedId));
      if (!w) {
        pane.innerHTML = `<div class="tb-label" style="padding:26px;text-align:center">请在左侧选择或新增一个控件</div>`;
        return;
      }
      const tMeta = WIDGET_TYPES.find((t) => t.type === w.type) || {
        name: w.type,
        colorArch: "single",
      };

      let html = `
        <div class="wcfg-card">
          <div class="wcfg-card-hdr">
            <div style="display:flex;align-items:center;gap:7px">
              <span>基础配置</span>
              <span class="wcfg-type-chip">${esc(tMeta.name)}</span>
              <button class="wcfg-q-btn" data-wg-help="${esc(
                w.type
              )}" title="查看可绑定的物理量与显示效果">?</button>
            </div>
            <label style="display:flex;align-items:center;gap:5px;font-weight:400;font-size:12px;cursor:pointer">
              <input type="checkbox" data-bind="showHeader" ${
                w.showHeader !== false ? "checked" : ""
              } />
              <span>显示标题栏</span>
            </label>
          </div>
          <div class="wcfg-grid-3">
            <label class="wcfg-label">标题
              <input type="text" data-bind="title" value="${esc(w.title || "")}" />
            </label>
            <label class="wcfg-label">宽度 (%)
              <input type="number" min="30" max="100" class="mono" data-bind-num="widthPct" value="${
                w.widthPct || 100
              }" />
            </label>
            <label class="wcfg-label">高度 (0=自适应)
              <input type="number" min="0" max="540" class="mono" id="wcfgHeightPx" value="${
                w.height || 0
              }" />
            </label>
          </div>
        </div>`;

      html += `<div class="wcfg-card"><div class="wcfg-card-hdr"><span>参数配置</span></div>`;

      if (w.type === "enum_badge") {
        html += `
          <label class="wcfg-label">数据通道 <select data-bind="channel">${chOpts(w.channel)}</select></label>
          <div style="display:flex;flex-direction:column;gap:5px">
            <div class="wcfg-sub-row" style="border-style:dashed">
              <span class="wcfg-locked-pill" title="未匹配任何枚举值时的默认兜底项">未命中</span>
              <input type="text" style="flex:1" data-bind="unmatchedLabel" value="${esc(
                w.unmatchedLabel || "未知状态"
              )}" placeholder="显示文字" />
              <input type="color" data-bind="unmatchedColor" value="${esc(
                w.unmatchedColor || "#8B5CF6"
              )}" />
              <span class="tb-label" style="width:24px;text-align:center">默认</span>
            </div>
            ${(w.enumMap || [])
              .map(
                (e, i) => `
              <div class="wcfg-sub-row">
                <input type="text" class="mono" style="width:74px" data-arr="enumMap" data-i="${i}" data-k="k" value="${esc(
                  e.k
                )}" placeholder="枚举值" />
                <input type="text" style="flex:1" data-arr="enumMap" data-i="${i}" data-k="label" value="${esc(
                  e.label
                )}" placeholder="显示文字" />
                <input type="color" data-arr="enumMap" data-i="${i}" data-k="color" value="${esc(
                  e.color || "#3B82F6"
                )}" />
                <button class="btn btn-icon btn-danger-outline" data-arr-del="enumMap:${i}" style="height:22px;min-width:22px">✕</button>
              </div>`
              )
              .join("")}
          </div>
          <div><button class="btn" id="wcfgAddEnum">+ 新增状态</button></div>`;
      } else if (w.type === "linear_bar") {
        html += `
          <div class="wcfg-grid-3">
            <label class="wcfg-label">数据通道 <select data-bind="channel">${chOpts(w.channel)}</select></label>
            <label class="wcfg-label">单位 <input type="text" class="mono" data-bind="unitOverride" value="${esc(
              w.unitOverride || ""
            )}" placeholder="默认随通道" /></label>
            <label class="wcfg-label">小数位 <input type="number" min="0" max="5" class="mono" data-bind-num="decimals" value="${
              w.decimals ?? 1
            }" /></label>
          </div>
          <div class="wcfg-grid-3">
            <label class="wcfg-label">下限 <input type="number" step="any" class="mono" data-bind-num="min" value="${
              w.min ?? 0
            }" /></label>
            <label class="wcfg-label">上限 <input type="number" step="any" class="mono" data-bind-num="max" value="${
              w.max ?? 100
            }" /></label>
            <label class="wcfg-label">起点
              <select data-bind="originMode" data-rebuild-drawer="1">
                <option value="left" ${w.originMode !== "center_zero" ? "selected" : ""}>左端单向</option>
                <option value="center_zero" ${w.originMode === "center_zero" ? "selected" : ""}>零位双向</option>
              </select>
            </label>
          </div>
          <div style="display:flex;align-items:center;gap:20px">
            <label style="display:flex;align-items:center;gap:5px;cursor:pointer">
              <input type="checkbox" data-bind="showPct" ${w.showPct !== false ? "checked" : ""} /> <span>显示百分比</span>
            </label>
            <label style="display:flex;align-items:center;gap:5px;cursor:pointer">
              <input type="checkbox" data-bind="showValue" ${w.showValue !== false ? "checked" : ""} /> <span>显示数值</span>
            </label>
          </div>`;
      } else if (w.type === "bitmask_led") {
        html += `
          <div style="display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:8px">
            <label class="wcfg-label">模式
              <select data-bind="decodeMode" data-rerender-ins="1" data-rebuild-drawer="1">
                <option value="bitmask" ${w.decodeMode === "bitmask" ? "selected" : ""}>按位掩码</option>
                <option value="expr_leds" ${w.decodeMode === "expr_leds" ? "selected" : ""}>独立表达式</option>
              </select>
            </label>
            <label class="wcfg-label">数据通道 <select data-bind="channel">${chOpts(
              w.channel,
              w.decodeMode === "expr_leds"
            )}</select></label>
            <label class="wcfg-label">样式
              <select data-bind="displayStyle" data-rerender-ins="1" data-rebuild-drawer="1">
                <option value="led_grid" ${w.displayStyle === "led_grid" ? "selected" : ""}>矩阵灯格</option>
                <option value="compact_pill" ${w.displayStyle === "compact_pill" ? "selected" : ""}>文字胶囊</option>
              </select>
            </label>
            <label class="wcfg-label">每行列数
              <select data-bind-num="gridCols" data-rebuild-drawer="1" ${
                w.displayStyle === "compact_pill" ? "disabled" : ""
              }>
                <option value="1" ${Number(w.gridCols) === 1 ? "selected" : ""}>1 列</option>
                <option value="2" ${!w.gridCols || Number(w.gridCols) === 2 ? "selected" : ""}>2 列</option>
                <option value="3" ${Number(w.gridCols) === 3 ? "selected" : ""}>3 列</option>
                <option value="4" ${Number(w.gridCols) === 4 ? "selected" : ""}>4 列</option>
              </select>
            </label>
          </div>
          <div style="display:flex;flex-direction:column;gap:5px">
            ${(w.items || [])
              .map(
                (it, i) => `
              <div class="wcfg-sub-row">
                ${
                  w.decodeMode === "expr_leds"
                    ? `<input type="text" style="width:94px" data-arr="items" data-i="${i}" data-k="label" value="${esc(
                        it.label
                      )}" data-rebuild-drawer="1" />
                       <input type="text" class="mono" style="flex:1" data-arr="items" data-i="${i}" data-k="expr" value="${esc(
                        it.expr || ""
                      )}" placeholder="如 abs(CH3) > 120" />
                       <span class="wcfg-hit-badge mono" data-led-badge="${i}">--</span>
                       <button class="wcfg-q-btn" data-open-help="led:${i}">?</button>`
                    : `<span class="mono tb-label">Bit</span>
                       <input type="text" class="mono" style="width:50px" data-arr="items" data-i="${i}" data-k="key" value="${esc(
                        it.key
                      )}" />
                       <input type="text" style="flex:1" data-arr="items" data-i="${i}" data-k="label" value="${esc(
                        it.label
                      )}" data-rebuild-drawer="1" />`
                }
                <input type="color" data-arr="items" data-i="${i}" data-k="color" value="${esc(
                  it.color || "#EF4444"
                )}" />
                <button class="btn btn-icon btn-danger-outline" data-arr-del="items:${i}" style="height:22px;min-width:22px">✕</button>
              </div>`
              )
              .join("")}
          </div>
          <div><button class="btn" id="wcfgAddLed">+ 新增指示项</button></div>`;
      } else if (w.type === "num_grid") {
        html += `
          <div style="display:flex;align-items:center;justify-content:space-between;gap:10px">
            <div style="display:flex;align-items:center;gap:8px">
              <span class="tb-label">列数</span>
              <select style="width:84px" data-bind-num="cols" data-rebuild-drawer="1">
                <option value="1" ${w.cols === 1 ? "selected" : ""}>1 列</option>
                <option value="2" ${w.cols !== 1 && w.cols !== 3 ? "selected" : ""}>2 列</option>
                <option value="3" ${w.cols === 3 ? "selected" : ""}>3 列</option>
              </select>
            </div>
            <button class="btn btn-primary" id="wcfgAddNumCard" style="height:24px">+ 新增子卡</button>
          </div>
          <div style="display:flex;flex-direction:column;gap:7px">
            ${(w.cards || [])
              .map(
                (c, i) => `
              <div class="wcfg-cond-card" data-subcard-wrap="${i}">
                <div style="display:flex;align-items:center;justify-content:space-between;gap:6px;flex-wrap:wrap">
                  <div style="display:flex;align-items:center;gap:6px;flex-wrap:wrap">
                    <span class="mono tb-label" style="font-weight:700">#${i + 1}</span>
                    <select style="width:154px" data-arr="cards" data-i="${i}" data-k="ch" data-rebuild-drawer="1">${chOpts(
                  c.ch
                )}</select>
                    <select style="width:94px" data-arr="cards" data-i="${i}" data-k="sub">
                      <option value="none" ${c.sub === "none" ? "selected" : ""}>无副标</option>
                      <option value="peak" ${c.sub === "peak" ? "selected" : ""}>峰值 Pk</option>
                      <option value="min" ${c.sub === "min" ? "selected" : ""}>谷值 Min</option>
                      <option value="pp" ${c.sub === "pp" ? "selected" : ""}>峰峰值 P-P</option>
                    </select>
                    <span class="wcfg-hit-badge mono" data-subcard-sum="${i}">常态</span>
                  </div>
                  <div style="display:flex;align-items:center;gap:5px">
                    <button class="btn" data-add-subcard-rule="${i}" style="height:22px;padding:0 7px">+ 条件</button>
                    <button class="btn btn-icon btn-danger-outline" data-arr-del="cards:${i}" style="height:22px;min-width:22px">✕</button>
                  </div>
                </div>
                ${
                  c.rules && c.rules.length > 0
                    ? `<div style="display:flex;flex-direction:column;gap:5px;padding-left:9px;border-left:2px solid var(--border)">
                         ${renderRuleCardsHTML(c.rules, "cardRules_" + i, "subcard")}
                       </div>`
                    : ``
                }
              </div>`
              )
              .join("")}
          </div>`;
      } else if (w.type === "arc_gauge") {
        html += `
          <div class="wcfg-grid-2">
            <label class="wcfg-label">主通道 <select data-bind="channel">${chOpts(w.channel)}</select></label>
            <label class="wcfg-label">参考通道 <select data-bind="refChannel">${chOpts(w.refChannel, true)}</select></label>
          </div>
          <div class="wcfg-grid-3">
            <label class="wcfg-label">下限 <input type="number" class="mono" data-bind-num="min" value="${w.min ?? 0}" /></label>
            <label class="wcfg-label">上限 <input type="number" class="mono" data-bind-num="max" value="${w.max ?? 3600}" /></label>
            <label class="wcfg-label">起点
              <select data-bind="originMode">
                <option value="left" ${w.originMode !== "center_zero" ? "selected" : ""}>左起单向</option>
                <option value="center_zero" ${w.originMode === "center_zero" ? "selected" : ""}>零位双向</option>
              </select>
            </label>
          </div>`;
      } else if (w.type === "three_phase") {
        html += `
          <div class="wcfg-grid-3">
            <label class="wcfg-label">下限 <input type="number" class="mono" data-bind-num="min" value="${w.min ?? 0}" /></label>
            <label class="wcfg-label">上限 <input type="number" class="mono" data-bind-num="max" value="${w.max ?? 100}" /></label>
            <label class="wcfg-label">基线
              <select data-bind="baselineMode" data-rebuild-drawer="1">
                <option value="bottom" ${w.baselineMode !== "center_zero" ? "selected" : ""}>底部</option>
                <option value="center_zero" ${w.baselineMode === "center_zero" ? "selected" : ""}>零位居中</option>
              </select>
            </label>
          </div>
          <div style="display:flex;flex-direction:column;gap:5px">
            ${(w.bars || [])
              .map(
                (b, i) => `
              <div class="wcfg-sub-row">
                <input type="text" class="mono" style="width:68px" data-arr="bars" data-i="${i}" data-k="label" value="${esc(
                  b.label
                )}" data-rebuild-drawer="1" />
                <select style="flex:1" data-arr="bars" data-i="${i}" data-k="ch">${chOpts(b.ch)}</select>
                <input type="color" data-arr="bars" data-i="${i}" data-k="color" value="${esc(
                  b.color || "#3B82F6"
                )}" data-rebuild-drawer="1" />
                <button class="btn btn-icon btn-danger-outline" data-arr-del="bars:${i}" style="height:22px;min-width:22px">✕</button>
              </div>`
              )
              .join("")}
          </div>
          <div><button class="btn" id="wcfgAddBar">+ 新增立柱</button></div>`;
      } else if (w.type === "angle_compass") {
        html += `
          <div class="wcfg-grid-2">
            <label class="wcfg-label">主通道 <select data-bind="channel">${chOpts(w.channel)}</select></label>
            <label class="wcfg-label">对比通道 <select data-bind="compareChannel">${chOpts(w.compareChannel, true)}</select></label>
          </div>
          <div class="wcfg-grid-3">
            <label class="wcfg-label">单位制
              <select data-bind="angleUnit" data-rerender-ins="1">
                <option value="deg360" ${w.angleUnit === "deg360" ? "selected" : ""}>角度 0~360°</option>
                <option value="rad2pi" ${w.angleUnit === "rad2pi" ? "selected" : ""}>弧度 0~2π</option>
                <option value="cpr" ${w.angleUnit === "cpr" ? "selected" : ""}>脉冲数 CPR</option>
              </select>
            </label>
            <label class="wcfg-label">满圈脉冲 <input type="number" class="mono" data-bind-num="encoderCpr" value="${
              w.encoderCpr || 16384
            }" ${w.angleUnit === "cpr" ? "" : "disabled"} /></label>
            <label class="wcfg-label">零位方向
              <select data-bind="zeroDir">
                <option value="top" ${w.zeroDir !== "right" ? "selected" : ""}>正上</option>
                <option value="right" ${w.zeroDir === "right" ? "selected" : ""}>正右</option>
              </select>
            </label>
          </div>`;
      } else if (w.type === "xy_lissajous") {
        html += `
          <div class="wcfg-grid-2">
            <label class="wcfg-label">X 通道 <select data-bind="xChannel" data-need-tail="1">${chOpts(w.xChannel)}</select></label>
            <label class="wcfg-label">Y 通道 <select data-bind="yChannel" data-need-tail="1">${chOpts(w.yChannel)}</select></label>
          </div>
          <div class="wcfg-grid-3">
            <label class="wcfg-label">量程
              <select data-bind="scaleMode" data-rerender-ins="1">
                <option value="fixed" ${w.scaleMode === "fixed" ? "selected" : ""}>固定</option>
                <option value="auto" ${w.scaleMode !== "fixed" ? "selected" : ""}>自动</option>
              </select>
            </label>
            <label class="wcfg-label">半径上限 <input type="number" class="mono" data-bind-num="fixedMax" value="${
              w.fixedMax || 50
            }" ${w.scaleMode !== "fixed" ? "disabled" : ""} /></label>
            <label class="wcfg-label">尾迹点数 <input type="number" class="mono" data-bind-num="tailPoints" value="${
              w.tailPoints || 64
            }" /></label>
          </div>`;
      } else if (w.type === "svpwm_hex") {
        html += `
          <div class="wcfg-grid-2">
            <label class="wcfg-label">输入模式
              <select data-bind="inputSource" data-rerender-ins="1">
                <option value="dq_theta" ${w.inputSource === "dq_theta" ? "selected" : ""}>Vd/Vq + 角度</option>
                <option value="duty_abc" ${w.inputSource === "duty_abc" ? "selected" : ""}>三相占空比</option>
                <option value="ab" ${w.inputSource === "ab" ? "selected" : ""}>Uα / Uβ 直入</option>
              </select>
            </label>
            <label class="wcfg-label">母线通道 <select data-bind="vbusCh">${chOpts(w.vbusCh)}</select></label>
          </div>
          ${
            w.inputSource === "dq_theta"
              ? `<div class="wcfg-grid-3">
                   <label class="wcfg-label">Vd <select data-bind="vdCh">${chOpts(w.vdCh)}</select></label>
                   <label class="wcfg-label">Vq <select data-bind="vqCh">${chOpts(w.vqCh)}</select></label>
                   <label class="wcfg-label">角度 θ <select data-bind="thetaCh">${chOpts(w.thetaCh)}</select></label>
                 </div>`
              : w.inputSource === "duty_abc"
              ? `<div class="wcfg-grid-3">
                   <label class="wcfg-label">A 相 <select data-bind="dutyACh">${chOpts(w.dutyACh)}</select></label>
                   <label class="wcfg-label">B 相 <select data-bind="dutyBCh">${chOpts(w.dutyBCh)}</select></label>
                   <label class="wcfg-label">C 相 <select data-bind="dutyCCh">${chOpts(w.dutyCCh)}</select></label>
                 </div>`
              : `<div class="wcfg-grid-2">
                   <label class="wcfg-label">Uα <select data-bind="vaCh">${chOpts(w.vaCh)}</select></label>
                   <label class="wcfg-label">Uβ <select data-bind="vbCh">${chOpts(w.vbCh)}</select></label>
                 </div>`
          }`;
      } else if (w.type === "id_iq_circle") {
        html += `
          <div class="wcfg-grid-3">
            <label class="wcfg-label">X 轴通道 <select data-bind="idCh">${chOpts(w.idCh)}</select></label>
            <label class="wcfg-label">Y 轴通道 <select data-bind="iqCh">${chOpts(w.iqCh)}</select></label>
            <label class="wcfg-label">限幅圆半径 <input type="number" class="mono" data-bind-num="imaxFixed" value="${
              w.imaxFixed || 120
            }" /></label>
          </div>`;
      } else if (w.type === "isr_stacked") {
        html += `
          <div class="wcfg-grid-3">
            <label class="wcfg-label">总预算 <input type="number" class="mono" data-bind-num="budget" value="${
              w.budget || 3000
            }" /></label>
            <label class="wcfg-label">单位 <input type="text" class="mono" data-bind="budgetUnit" value="${esc(
              w.budgetUnit || "cyc"
            )}" /></label>
            <label class="wcfg-label">总量通道 <select data-bind="totalCh">${chOpts(w.totalCh, true)}</select></label>
          </div>
          <div style="display:flex;flex-direction:column;gap:5px">
            ${(w.stages || [])
              .map(
                (s, i) => `
              <div class="wcfg-sub-row">
                <input type="text" style="width:94px" data-arr="stages" data-i="${i}" data-k="label" value="${esc(
                  s.label
                )}" data-rebuild-drawer="1" />
                <select style="flex:1" data-arr="stages" data-i="${i}" data-k="ch">${chOpts(s.ch)}</select>
                <input type="color" data-arr="stages" data-i="${i}" data-k="color" value="${esc(
                  s.color || "#3B82F6"
                )}" data-rebuild-drawer="1" />
                <button class="btn btn-icon btn-danger-outline" data-arr-del="stages:${i}" style="height:22px;min-width:22px">✕</button>
              </div>`
              )
              .join("")}
          </div>
          <div><button class="btn" id="wcfgAddStage">+ 新增分段</button></div>`;
      } else if (w.type === "ref_fbk_err") {
        html += `
          <div class="wcfg-grid-3">
            <label class="wcfg-label">给定通道 <select data-bind="refCh">${chOpts(w.refCh)}</select></label>
            <label class="wcfg-label">反馈通道 <select data-bind="fbkCh">${chOpts(w.fbkCh)}</select></label>
            <label class="wcfg-label">量程上限 <input type="number" class="mono" data-bind-num="max" value="${
              w.max ?? 3600
            }" /></label>
          </div>`;
      } else if (w.type === "spark_stat") {
        html += `
          <div class="wcfg-grid-3">
            <label class="wcfg-label">数据通道 <select data-bind="channel">${chOpts(w.channel)}</select></label>
            <label class="wcfg-label">窗口点数 <input type="number" class="mono" data-bind-num="windowPts" value="${
              w.windowPts || 60
            }" /></label>
            <label class="wcfg-label">统计值
              <select data-bind="statType">
                <option value="pp" ${w.statType === "pp" ? "selected" : ""}>峰峰值 P-P</option>
                <option value="maxmin" ${w.statType === "maxmin" ? "selected" : ""}>极值 Min~Max</option>
              </select>
            </label>
          </div>`;
      } else if (w.type === "quick_visibility") {
        html += `
          <div style="display:flex;align-items:center;gap:18px">
            <div style="display:flex;align-items:center;gap:7px">
              <span class="tb-label">列数</span>
              <select style="width:84px" data-bind-num="cols" data-rebuild-drawer="1">
                <option value="1" ${w.cols !== 2 ? "selected" : ""}>1 列</option>
                <option value="2" ${w.cols === 2 ? "selected" : ""}>2 列</option>
              </select>
            </div>
            <label style="display:flex;align-items:center;gap:5px;cursor:pointer">
              <input type="checkbox" data-bind="showLiveNum" data-rebuild-drawer="1" ${
                w.showLiveNum !== false ? "checked" : ""
              } /> 显示实时值
            </label>
          </div>`;
      }
      html += `</div>`;

      // CARD 3: 颜色配置 / 外框高亮
      if (tMeta.colorArch === "single") {
        html += `
          <div class="wcfg-card">
            <div class="wcfg-card-hdr">
              <span>颜色配置</span>
              <button class="btn" id="wcfgAddColorRule" style="height:23px;padding:0 8px">+ 新增条件</button>
            </div>
            <div style="display:flex;align-items:center;gap:10px">
              <span class="tb-label">默认颜色</span>
              <select style="width:128px" data-bind="normalColorMode" data-rerender-ins="1">
                <option value="fixed" ${w.normalColorMode !== "channel" ? "selected" : ""}>固定颜色</option>
                <option value="channel" ${w.normalColorMode === "channel" ? "selected" : ""}>跟随通道</option>
              </select>
              ${
                w.normalColorMode !== "channel"
                  ? `<input type="color" data-bind="baseColor" value="${esc(
                      w.baseColor || "#10B981"
                    )}" />`
                  : ``
              }
            </div>
            ${
              w.colorRules && w.colorRules.length > 0
                ? `<div style="display:flex;flex-direction:column;gap:6px">${renderRuleCardsHTML(
                    w.colorRules,
                    "colorRules",
                    "single"
                  )}</div>`
                : ``
            }
          </div>`;
      } else if (tMeta.colorArch === "multi") {
        html += `
          <div class="wcfg-card">
            <div class="wcfg-card-hdr">
              <span>外框高亮</span>
              <button class="btn" id="wcfgAddFrameRule" style="height:23px;padding:0 8px">+ 新增条件</button>
            </div>
            ${
              w.frameRules && w.frameRules.length > 0
                ? `<div style="display:flex;flex-direction:column;gap:6px">${renderRuleCardsHTML(
                    w.frameRules,
                    "frameRules",
                    "frame"
                  )}</div>`
                : `<div class="tb-label">未设置条件</div>`
            }
          </div>`;
      }

      pane.innerHTML = html;
      bindInspectorEvents(w);
      updateInspectorRuleHitBadges();
    }

    function updateInspectorRuleHitBadges() {
      if (hooks.getActivePage() !== "config") return;
      const p = prof();
      if (!p || !Array.isArray(p.drawer_widgets)) return;
      const w = p.drawer_widgets.find((x) => String(x.id) === String(rt.selectedId));
      if (!w) return;

      const checkList = (arr, name, selfVal) => {
        const list = arr || [];
        const evals = list.map((r) => evaluateConditionRule(r, selfVal));
        const winnerIdx = evals.findIndex((res) => res.ok && res.hit);

        list.forEach((r, ri) => {
          const badge = document.querySelector(`[data-hit-badge="${name}:${ri}"]`);
          const card = document.querySelector(`[data-rule-card="${name}:${ri}"]`);
          if (!badge || !card) return;
          const res = evals[ri];
          card.style.setProperty("--hit-col", r.color || "#EF4444");
          card.classList.toggle("hit-now", res.ok && res.hit && ri === winnerIdx);
          badge.classList.remove("on", "overridden");
          badge.style.color = "";

          if (!res.ok) {
            badge.textContent = "⚠ " + (res.err || "语法错");
            badge.style.color = "var(--danger)";
          } else if (res.hit && ri === winnerIdx) {
            badge.textContent = "● 生效";
            badge.classList.add("on");
            badge.style.setProperty("--hit-col", r.color || "#EF4444");
          } else if (res.hit && ri > winnerIdx) {
            badge.textContent = `◐ 被 P${winnerIdx + 1} 覆盖`;
            badge.classList.add("overridden");
          } else {
            badge.textContent = "○ 未触发";
          }
        });
        return winnerIdx >= 0 ? list[winnerIdx] : null;
      };

      const mainSelf = readChannelVal(w.channel || w.fbkCh || "CH0");
      checkList(w.colorRules, "colorRules", mainSelf);
      checkList(w.frameRules, "frameRules", mainSelf);

      if (w.type === "num_grid") {
        (w.cards || []).forEach((c, ci) => {
          const v = readChannelVal(c.ch);
          const winRule = checkList(c.rules, `cardRules_${ci}`, v);
          const wrap = document.querySelector(`[data-subcard-wrap="${ci}"]`);
          const sumTag = document.querySelector(`[data-subcard-sum="${ci}"]`);
          if (wrap) {
            wrap.style.setProperty("--hit-col", winRule ? winRule.color || "#EF4444" : "#3B82F6");
            wrap.classList.toggle("hit-now", !!winRule);
          }
          if (sumTag) {
            if (winRule) {
              sumTag.textContent = "● 已变色";
              sumTag.classList.add("on");
              sumTag.style.setProperty("--hit-col", winRule.color || "#EF4444");
            } else {
              sumTag.textContent =
                c.rules && c.rules.length > 0 ? `${c.rules.length} 条规则` : "常态";
              sumTag.classList.remove("on");
            }
          }
        });
      }

      if (w.type === "bitmask_led" && w.decodeMode === "expr_leds") {
        const code = Math.round(readChannelVal(w.channel || "CH7"));
        (w.items || []).forEach((it, li) => {
          const badge = document.querySelector(`[data-led-badge="${li}"]`);
          if (!badge) return;
          const res = evalCustomStr(it.expr, code);
          badge.classList.remove("on");
          badge.style.color = "";
          if (!res.ok) {
            badge.textContent = "⚠ " + res.err;
            badge.style.color = "var(--danger)";
          } else if (res.hit) {
            badge.textContent = "● 亮";
            badge.classList.add("on");
            badge.style.setProperty("--hit-col", it.color || "#EF4444");
          } else {
            badge.textContent = "○ 灭";
          }
        });
      }
    }

    function bindInspectorEvents(w) {
      const pane = document.getElementById("widgetInspectorPane");
      if (!pane) return;

      pane.querySelectorAll("[data-bind]").forEach((el) => {
        const key = el.dataset.bind;
        const needIns = el.dataset.rerenderIns === "1";
        const needDrawerDOM =
          el.dataset.rebuildDrawer === "1" || key === "title" || key === "showHeader";
        const evName = el.type === "checkbox" || el.tagName === "SELECT" ? "change" : "input";
        el.addEventListener(evName, () => {
          w[key] = el.type === "checkbox" ? el.checked : el.value;
          renderInstanceList();
          if (needIns) renderInspector();
          if (needDrawerDOM) renderDrawerStructure();
          else updateDrawerLive();
          schedulePersist();
        });
      });

      pane.querySelectorAll("[data-bind-num]").forEach((el) => {
        const key = el.dataset.bindNum;
        const needDrawerDOM = el.dataset.rebuildDrawer === "1" || key === "widthPct";
        const evName = el.tagName === "SELECT" ? "change" : "input";
        el.addEventListener(evName, () => {
          const parsed = parseFloat(el.value);
          if (isFinite(parsed)) w[key] = parsed;
          renderInstanceList();
          if (needDrawerDOM) renderDrawerStructure();
          else updateDrawerLive();
          schedulePersist();
        });
      });

      const hInp = document.getElementById("wcfgHeightPx");
      if (hInp) {
        hInp.addEventListener("input", () => {
          const v = parseInt(hInp.value, 10);
          w.height = isFinite(v) && v > 40 ? v : null;
          renderDrawerStructure();
          schedulePersist();
        });
      }

      pane.querySelectorAll("[data-arr]").forEach((el) => {
        const needDrawerDOM = el.dataset.rebuildDrawer === "1";
        const needIns = el.dataset.rerenderIns === "1";
        const evName = el.tagName === "SELECT" ? "change" : "input";
        el.addEventListener(evName, () => {
          w[el.dataset.arr][Number(el.dataset.i)][el.dataset.k] = el.value;
          if (needIns) renderInspector();
          if (needDrawerDOM) renderDrawerStructure();
          else updateDrawerLive();
          updateInspectorRuleHitBadges();
          schedulePersist();
        });
      });

      pane.querySelectorAll("[data-arr-del]").forEach((btn) => {
        btn.addEventListener("click", () => {
          const [aName, idx] = btn.dataset.arrDel.split(":");
          w[aName].splice(Number(idx), 1);
          renderInspector();
          renderDrawerStructure();
          commitPersistNow();
        });
      });

      pane.querySelectorAll("[data-add-subcard-rule]").forEach((btn) => {
        btn.addEventListener("click", () => {
          const ci = Number(btn.dataset.addSubcardRule);
          const list = getRuleArray(w, "cardRules_" + ci, true);
          list.push(
            mkRule({
              mode: "builder",
              leftVar: "val",
              op: ">=",
              rightMode: "const",
              rightConst: 50,
              color: "#EF4444",
            })
          );
          renderInspector();
          updateDrawerLive();
          commitPersistNow();
        });
      });

      pane.querySelectorAll("[data-rprio-sel]").forEach((sel) => {
        sel.addEventListener("change", () => {
          const [aName, riStr] = sel.dataset.rprioSel.split(":");
          const fromIdx = Number(riStr),
            toIdx = Number(sel.value);
          const list = getRuleArray(w, aName, false);
          if (fromIdx !== toIdx && fromIdx >= 0 && toIdx >= 0 && toIdx < list.length) {
            const [moved] = list.splice(fromIdx, 1);
            list.splice(toIdx, 0, moved);
            renderInspector();
            updateDrawerLive();
            commitPersistNow();
          }
        });
      });

      pane.querySelectorAll("[data-rprio-move]").forEach((btn) => {
        btn.addEventListener("click", () => {
          const [aName, riStr, dStr] = btn.dataset.rprioMove.split(":");
          const ri = Number(riStr),
            dst = ri + Number(dStr);
          const list = getRuleArray(w, aName, false);
          if (dst >= 0 && dst < list.length) {
            const [moved] = list.splice(ri, 1);
            list.splice(dst, 0, moved);
            renderInspector();
            updateDrawerLive();
            commitPersistNow();
          }
        });
      });

      pane.querySelectorAll("[data-rmode]").forEach((btn) => {
        btn.addEventListener("click", () => {
          const [aName, ri, mode] = btn.dataset.rmode.split(":");
          const list = getRuleArray(w, aName, false);
          const r = list[Number(ri)];
          if (!r) return;
          if (mode === "custom" && r.mode === "builder") {
            const rhs = r.rightMode === "var" ? r.rightVar : r.rightConst;
            const lhs = r.leftVar === "abs_val" ? "abs(val)" : r.leftVar;
            r.expr = `${lhs} ${r.op} ${rhs}`;
          }
          r.mode = mode;
          renderInspector();
          updateDrawerLive();
          commitPersistNow();
        });
      });

      pane.querySelectorAll("[data-rarr]").forEach((el) => {
        const needIns = el.dataset.rerenderIns === "1";
        const evName = el.tagName === "SELECT" ? "change" : "input";
        el.addEventListener(evName, () => {
          const list = getRuleArray(w, el.dataset.rarr, false);
          const r = list[Number(el.dataset.ri)];
          if (!r) return;
          r[el.dataset.rk] = el.value;
          if (needIns) renderInspector();
          updateDrawerLive();
          updateInspectorRuleHitBadges();
          schedulePersist();
        });
      });

      pane.querySelectorAll("[data-rarr-num]").forEach((el) => {
        el.addEventListener("input", () => {
          const [aName, ri, rk] = el.dataset.rarrNum.split(":");
          const list = getRuleArray(w, aName, false);
          const r = list[Number(ri)];
          if (!r) return;
          const v = parseFloat(el.value);
          r[rk] = isFinite(v) ? v : 0;
          updateDrawerLive();
          updateInspectorRuleHitBadges();
          schedulePersist();
        });
      });

      pane.querySelectorAll("[data-rbool]").forEach((el) => {
        el.addEventListener("change", () => {
          const [aName, ri, rk] = el.dataset.rbool.split(":");
          const list = getRuleArray(w, aName, false);
          const r = list[Number(ri)];
          if (!r) return;
          r[rk] = el.checked;
          renderInspector();
          updateDrawerLive();
          commitPersistNow();
        });
      });

      pane.querySelectorAll("[data-rdel]").forEach((btn) => {
        btn.addEventListener("click", () => {
          const [aName, ri] = btn.dataset.rdel.split(":");
          const list = getRuleArray(w, aName, false);
          list.splice(Number(ri), 1);
          renderInspector();
          updateDrawerLive();
          commitPersistNow();
        });
      });

      pane.querySelectorAll("[data-open-help]").forEach((btn) => {
        btn.addEventListener("click", (e) => {
          e.stopPropagation();
          const row = btn.closest(".wcfg-cond-card, .wcfg-sub-row");
          rt.activeHelpInput = row ? row.querySelector("input.mono") : null;
          openAutoFlipPop(document.getElementById("exprHelpPop"), btn);
        });
      });

      pane.querySelectorAll("[data-wg-help]").forEach((btn) => {
        btn.addEventListener("click", (e) => {
          e.stopPropagation();
          showWidgetGuide(btn.dataset.wgHelp, btn);
        });
      });

      const bindAdd = (id, arrKey, item, rebuildDrawer) => {
        const b = document.getElementById(id);
        if (b) {
          b.addEventListener("click", () => {
            w[arrKey] = w[arrKey] || [];
            w[arrKey].push(item);
            renderInspector();
            if (rebuildDrawer) renderDrawerStructure();
            else updateDrawerLive();
            commitPersistNow();
          });
        }
      };
      const cat = getChannelsCatalog();
      const firstKey = (cat[0] && cat[0].key) || "CH0";
      bindAdd("wcfgAddEnum", "enumMap", { k: "1", label: "新状态", color: "#3B82F6" }, false);
      bindAdd("wcfgAddLed", "items", { key: "6", label: "新指示", color: "#10B981", expr: "CH6 == 4" }, true);
      bindAdd("wcfgAddNumCard", "cards", { ch: firstKey, dec: 2, sub: "none", rules: [] }, true);
      bindAdd("wcfgAddBar", "bars", { label: "D", ch: firstKey, color: "#EC4899" }, true);
      bindAdd("wcfgAddStage", "stages", { label: "新分段", ch: firstKey, color: "#06B6D4" }, true);
      bindAdd(
        "wcfgAddColorRule",
        "colorRules",
        mkRule({ mode: "builder", leftVar: "val", op: ">=", rightMode: "const", rightConst: 50, color: "#EF4444", alertFrame: true, frameColor: "#EF4444" }),
        false
      );
      bindAdd(
        "wcfgAddFrameRule",
        "frameRules",
        mkRule({ mode: "builder", leftVar: w.channel || "CH6", op: "==", rightMode: "const", rightConst: 6, color: "#EF4444" }),
        false
      );
    }

    // ==================== RIGHT DRAWER DOM & 60FPS RENDERER ====================
    function widgetInnerHTML(w) {
      const id = esc(w.id);
      if (w.type === "enum_badge") {
        return `<div id="wb_${id}" style="display:flex;align-items:center;justify-content:space-between;padding:5px 8px;border-radius:4px;border:1px solid var(--border);background:var(--bg-panel)">
          <span style="display:flex;align-items:center;gap:6px;font-weight:600;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">
            <span class="eb-dot" style="width:8px;height:8px;border-radius:50%;flex-shrink:0"></span><span class="eb-txt">--</span>
          </span>
          <span class="mono eb-ch tb-label">${esc(w.channel || "")}</span>
        </div>`;
      }
      if (w.type === "linear_bar") {
        return `<div id="wb_${id}" style="display:flex;flex-direction:column;gap:4px">
          <div class="lb-top-row" style="display:flex;justify-content:space-between;align-items:baseline">
            <span class="mono lb-val" style="font-size:14px;font-weight:700">--</span>
            <span class="mono lb-pct tb-label"></span>
          </div>
          <div style="height:8px;border-radius:4px;background:var(--bg-input);border:1px solid var(--border);position:relative;overflow:hidden">
            ${
              w.originMode === "center_zero"
                ? `<div style="position:absolute;left:50%;top:0;bottom:0;width:1px;background:var(--text-muted)"></div>`
                : ``
            }
            <div class="lb-fill" style="position:absolute;top:0;bottom:0;left:0"></div>
          </div>
        </div>`;
      }
      if (w.type === "bitmask_led") {
        if (w.displayStyle === "compact_pill") {
          return `<div class="mono" id="wb_${id}" style="padding:5px 8px;border-radius:4px;border:1px solid var(--border);background:var(--bg-panel);font-weight:600;white-space:normal;word-break:break-word;line-height:1.38"></div>`;
        }
        return `<div class="dw-led-matrix" id="wb_${id}" style="grid-template-columns:repeat(${
          w.gridCols || 2
        }, minmax(0,1fr))">
          ${(w.items || [])
            .map(
              (it, i) =>
                `<div class="dw-led-cell" data-li="${i}"><span class="dw-led-bulb"></span><span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${esc(
                  it.label
                )}</span></div>`
            )
            .join("")}
        </div>`;
      }
      if (w.type === "num_grid") {
        return `<div class="dw-num-grid" id="wb_${id}" style="grid-template-columns:repeat(${
          w.cols || 2
        }, minmax(0,1fr))">
          ${(w.cards || [])
            .map((c, ci) => {
              const m = chMeta(c.ch);
              return `<div class="dw-num-cell" data-ci="${ci}" style="--c-col:${esc(m.color)}">
                <div style="color:var(--text-secondary);font-size:11px;display:flex;justify-content:space-between;gap:4px">
                  <span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${esc(
                    m.name
                  )}</span>
                  <span class="mono ng-sub" style="font-size:10.5px;color:var(--text-muted)"></span>
                </div>
                <div style="display:flex;justify-content:space-between;align-items:baseline;margin-top:1px">
                  <span class="mono ng-v" style="font-size:14.5px;font-weight:600">--</span>
                  <span style="color:var(--text-muted);font-size:10.5px">${esc(m.unit)}</span>
                </div>
              </div>`;
            })
            .join("")}
        </div>`;
      }
      if (w.type === "three_phase") {
        return `<div id="wb_${id}" style="display:flex;align-items:stretch;justify-content:space-around;height:100%;min-height:86px;gap:6px">
          ${(w.bars || [])
            .map(
              (b, bi) => `
            <div style="flex:1;display:flex;flex-direction:column;align-items:center;gap:3px">
              <span class="mono tp-v-${bi}" style="font-size:11px">0</span>
              <div style="flex:1;width:18px;background:var(--bg-input);border:1px solid var(--border);border-radius:3px;position:relative;overflow:hidden">
                ${
                  w.baselineMode === "center_zero"
                    ? `<div style="position:absolute;left:0;right:0;top:50%;height:1px;background:var(--text-muted)"></div>`
                    : ``
                }
                <div class="tp-b-${bi}" style="position:absolute;left:0;right:0;bottom:0;background:${esc(
                  b.color || "#3B82F6"
                )}"></div>
              </div>
              <span class="mono tb-label">${esc(b.label)}</span>
            </div>`
            )
            .join("")}
        </div>`;
      }
      if (w.type === "isr_stacked") {
        return `<div id="wb_${id}" style="display:flex;flex-direction:column;gap:5px">
          <div style="display:flex;justify-content:space-between;font-size:11.5px">
            <span class="mono isr-tot" style="font-weight:700">--</span>
            <span class="mono isr-pct" style="font-weight:600">--</span>
          </div>
          <div style="height:10px;border-radius:4px;background:var(--bg-input);border:1px solid var(--border);display:flex;overflow:hidden">
            ${(w.stages || [])
              .map(
                (s, si) =>
                  `<div data-si="${si}" style="background:${esc(s.color || "#3B82F6")};height:100%"></div>`
              )
              .join("")}
            <div class="seg-rest" style="background:#F59E0B;height:100%"></div>
          </div>
          <div style="display:flex;flex-wrap:wrap;gap:7px;font-size:11px;color:var(--text-secondary)">
            ${(w.stages || [])
              .map(
                (s, si) =>
                  `<span><i style="display:inline-block;width:7px;height:7px;border-radius:2px;background:${esc(
                    s.color || "#3B82F6"
                  )}"></i> ${esc(s.label)} <b class="mono st-v-${si}">0</b></span>`
              )
              .join("")}
          </div>
        </div>`;
      }
      if (w.type === "ref_fbk_err") {
        return `<div id="wb_${id}" style="display:flex;flex-direction:column;gap:5px">
          <div style="display:flex;justify-content:space-between;font-size:11.5px">
            <span>给定 <b class="mono rf-r">0</b> / 反馈 <b class="mono rf-f">0</b></span>
            <span class="mono rf-e" style="font-weight:700">Δ 0</span>
          </div>
          <div style="height:6px;background:var(--bg-input);border-radius:3px;overflow:hidden"><div class="rf-br" style="height:100%;background:${esc(
            w.refColor || "#3B82F6"
          )}"></div></div>
          <div style="height:6px;background:var(--bg-input);border-radius:3px;overflow:hidden"><div class="rf-bf" style="height:100%"></div></div>
        </div>`;
      }
      if (w.type === "spark_stat") {
        return `<div id="wb_${id}" style="display:flex;flex-direction:column;gap:4px">
          <div style="display:flex;justify-content:space-between"><span class="mono sp-v" style="font-weight:700">--</span><span class="mono sp-st tb-label">--</span></div>
          <canvas class="sp-c" width="240" height="38" style="width:100%;height:38px"></canvas>
        </div>`;
      }
      if (w.type === "quick_visibility") {
        const p = prof();
        const plotMap = Object.fromEntries((p.plots || []).map((pl) => [pl.id, pl.title]));
        const cat = getChannelsCatalog();
        return `<div id="wb_${id}" style="display:grid;grid-template-columns:repeat(${
          w.cols || 1
        }, minmax(0,1fr));gap:2px;max-height:220px;overflow-y:auto">
          ${cat
            .map((c) => {
              const checked = c.visible && c.plot_id !== "none";
              const tag =
                c.plot_id === "none" ? "" : (plotMap[c.plot_id] || c.plot_id).slice(0, 5);
              return `<label class="qch-row" style="justify-content:space-between">
                <span style="display:flex;align-items:center;gap:6px;overflow:hidden">
                  <input type="checkbox" ${
                    c.isMath ? `data-qv-math="${c.rawIdx}"` : `data-qv-ch="${c.rawIdx}"`
                  } ${checked ? "checked" : ""} />
                  <span class="qch-dot" style="background:${esc(c.color)}"></span>
                  <span class="qch-name">${esc(c.isMath ? c.name : `${c.key} ${c.name}`)}</span>
                </span>
                ${
                  w.showLiveNum !== false
                    ? `<span class="mono qv-n" data-ck="${esc(
                        c.key
                      )}" style="font-size:11px;color:var(--text-secondary)">--</span>`
                    : `<span class="qch-plot-tag">${esc(tag)}</span>`
                }
              </label>`;
            })
            .join("")}
        </div>`;
      }
      return `<div style="width:100%;height:100%;min-height:88px;display:flex;align-items:center;justify-content:center"><canvas id="cvs_${id}"></canvas></div>`;
    }

    function renderDrawerStructure() {
      const host = document.getElementById("drawerWidgetsCanvas");
      const p = prof();
      if (!host || !p) return;
      const widgets = ensureWidgetsInitialized();
      const isCfgPage = hooks.getActivePage() === "config";
      const unlocked = !p.layout_locked || isCfgPage;

      host.innerHTML = widgets
        .filter((w) => w.enabled !== false)
        .map((w) => {
          const wPct = Math.max(30, Math.min(100, Number(w.widthPct) || 100));
          const hCss = w.height ? `height:${Number(w.height)}px;` : ``;
          const isSel = isCfgPage && String(w.id) === String(rt.selectedId);
          return `
            <div class="dw-box ${unlocked ? "unlocked" : ""} ${isSel ? "selected-outline" : ""}"
                 id="wcard_${esc(w.id)}" data-wid="${esc(w.id)}"
                 style="width:calc(${wPct}% - 8px);${hCss}${unlocked ? "cursor:grab;" : ""}">
              ${
                w.showHeader !== false
                  ? `<div class="dw-hdr" style="${unlocked ? "cursor:grab" : ""}"><span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${
                      unlocked ? "⋮⋮ " : ""
                    }${esc(w.title)}</span></div>`
                  : ``
              }
              <div class="dw-body">${widgetInnerHTML(w)}</div>
              ${
                unlocked
                  ? `<div class="dw-grip-r" data-rgrip="${esc(w.id)}"></div>
                     <div class="dw-grip-b" data-bgrip="${esc(w.id)}"></div>
                     <div class="dw-grip-br" data-brgrip="${esc(w.id)}"></div>
                     <div class="dw-size-hud mono"></div>`
                  : ``
              }
            </div>`;
        })
        .join("");

      bindDrawerInteractions();
      updateDrawerLive();
    }

    function fitCanvas(cvs) {
      const dpr = window.devicePixelRatio || 1;
      const p = cvs.parentElement;
      const cw = Math.max(80, p ? p.clientWidth - 2 : 140);
      const ch = Math.max(74, p && p.clientHeight > 40 ? p.clientHeight - 2 : 110);
      cvs.style.width = cw + "px";
      cvs.style.height = ch + "px";
      if (cvs.width !== Math.round(cw * dpr) || cvs.height !== Math.round(ch * dpr)) {
        cvs.width = Math.round(cw * dpr);
        cvs.height = Math.round(ch * dpr);
      }
      const ctx = cvs.getContext("2d");
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      return { ctx, cw, ch };
    }

    function toDeg360(raw, unit, cpr) {
      let d = raw;
      if (unit === "rad2pi" || unit === "radpi") d = (raw * 180) / Math.PI;
      else if (unit === "cpr") d = (raw / Math.max(1, cpr || 16384)) * 360;
      return ((d % 360) + 360) % 360;
    }

    function updateDrawerLive() {
      const p = prof();
      if (!p || !Array.isArray(p.drawer_widgets)) return;
      const dr = document.getElementById("rightDrawer");
      if (dr && dr.classList.contains("collapsed")) return;

      const isLight = document.documentElement.getAttribute("data-theme") === "light";
      const txtCol = isLight ? "#18191C" : "#E6E7EB";
      const mutCol = isLight ? "#555A66" : "#989CA6";
      const brdCol = isLight ? "#D5D9E2" : "#2D2F36";

      for (const w of p.drawer_widgets) {
        if (w.enabled === false) continue;
        const card = document.getElementById(`wcard_${w.id}`);
        if (!card) continue;
        const tMeta = WIDGET_TYPES.find((t) => t.type === w.type) || { colorArch: "single" };
        const selfVal = readChannelVal(w.channel || w.fbkCh || "CH0");

        let mainCol = "#3B82F6",
          frameCol = null;
        if (tMeta.colorArch === "single") {
          const ap = resolveSingleWidgetAppearance(w, selfVal);
          mainCol = ap.color;
          frameCol = ap.frameColor;
        } else if (tMeta.colorArch === "multi") {
          frameCol = resolveMultiWidgetFrameColor(w, selfVal);
        }

        if (frameCol) {
          card.style.setProperty("--frame-col", frameCol);
          card.classList.add("custom-frame-active");
        } else {
          card.classList.remove("custom-frame-active");
        }

        if (w.type === "enum_badge") {
          const el = document.getElementById(`wb_${w.id}`);
          if (!el) continue;
          const iv = Math.round(readChannelVal(w.channel));
          const hit = (w.enumMap || []).find(
            (x) => String(x.k).trim() !== "" && Number(x.k) === iv
          );
          el.querySelector(".eb-dot").style.background = hit
            ? hit.color
            : w.unmatchedColor || "#8B5CF6";
          el.querySelector(".eb-txt").textContent = hit
            ? hit.label
            : `${w.unmatchedLabel || "未知状态"} (${iv})`;
          el.querySelector(".eb-ch").textContent = w.channel || "";
        } else if (w.type === "linear_bar") {
          const el = document.getElementById(`wb_${w.id}`);
          if (!el) continue;
          const m = chMeta(w.channel);
          const v = readChannelVal(w.channel);
          const min = Number(w.min ?? 0),
            max = Math.max(min + 1e-6, Number(w.max ?? 100));
          const valEl = el.querySelector(".lb-val"),
            pctEl = el.querySelector(".lb-pct"),
            topRow = el.querySelector(".lb-top-row");

          valEl.style.display = w.showValue === false ? "none" : "";
          pctEl.style.display = w.showPct === false ? "none" : "";
          topRow.style.display = w.showValue === false && w.showPct === false ? "none" : "flex";

          const unitStr = w.unitOverride || m.unit;
          valEl.textContent = `${v.toFixed(w.decimals ?? m.dec)}${unitStr ? " " + unitStr : ""}`;
          valEl.style.color = mainCol;
          const fill = el.querySelector(".lb-fill");
          fill.style.background = mainCol;

          if (w.originMode === "center_zero") {
            const hs = Math.max(Math.abs(min), Math.abs(max), 1),
              r = Math.max(-1, Math.min(1, v / hs));
            pctEl.textContent = `${(r * 100).toFixed(1)}%`;
            fill.style.left = r >= 0 ? "50%" : `${50 + r * 50}%`;
            fill.style.width = `${Math.abs(r) * 50}%`;
          } else {
            const pct = Math.max(0, Math.min(100, ((v - min) / (max - min)) * 100));
            pctEl.textContent = `${pct.toFixed(1)}%`;
            fill.style.left = "0";
            fill.style.width = `${pct}%`;
          }
        } else if (w.type === "bitmask_led") {
          const el = document.getElementById(`wb_${w.id}`);
          if (!el) continue;
          const code = Math.round(readChannelVal(w.channel || "CH7")) & 0xffffffff;
          const names = [];
          (w.items || []).forEach((it, i) => {
            const on =
              w.decodeMode === "expr_leds"
                ? evalCustomStr(it.expr, code).hit
                : (code & (1 << Number(it.key))) !== 0;
            if (on) names.push(it.label);
            const cell = el.querySelector(`[data-li="${i}"]`);
            if (cell) {
              const c = it.color || "#EF4444";
              cell.style.borderColor = on ? c : "";
              cell.style.color = on ? c : "";
              const b = cell.querySelector(".dw-led-bulb");
              b.style.background = on ? c : "";
              b.style.boxShadow = on ? `0 0 6px ${c}` : "";
            }
          });
          if (w.displayStyle === "compact_pill") {
            if (names.length === 0) {
              el.textContent = w.okText || "0x0000 正常";
              el.style.color = "var(--ok)";
              el.style.borderColor = "var(--border)";
            } else {
              const hexTag =
                w.decodeMode === "bitmask"
                  ? `0x${(code & 0xffff).toString(16).toUpperCase().padStart(4, "0")} `
                  : "";
              el.textContent = hexTag + names.join(" | ");
              el.style.color = "var(--danger)";
              el.style.borderColor = "var(--danger)";
            }
          }
        } else if (w.type === "num_grid") {
          const el = document.getElementById(`wb_${w.id}`);
          if (!el) continue;
          (w.cards || []).forEach((c, ci) => {
            const cell = el.querySelector(`[data-ci="${ci}"]`);
            if (!cell) return;
            const m = chMeta(c.ch),
              v = readChannelVal(c.ch),
              dec = c.dec ?? m.dec;
            const vEl = cell.querySelector(".ng-v");
            vEl.textContent = v.toFixed(dec);
            const warnRes = evaluateSubCardWarn(c, v);
            const wCol = warnRes.color || "#EF4444";
            cell.classList.toggle("sub-alert", warnRes.hit);
            if (warnRes.hit) {
              cell.style.setProperty("--sub-alert-col", wCol);
              vEl.style.color = wCol;
            } else {
              vEl.style.color = "";
            }
            const sub = cell.querySelector(".ng-sub");
            if (c.sub === "peak" && rt.peaks[c.ch] !== undefined) {
              sub.textContent = `Pk:${Number(rt.peaks[c.ch]).toFixed(dec)}`;
            } else if (c.sub === "min" && rt.mins[c.ch] !== undefined) {
              sub.textContent = `Min:${Number(rt.mins[c.ch]).toFixed(dec)}`;
            } else if (c.sub === "pp" && Array.isArray(rt.tails[c.ch]) && rt.tails[c.ch].length > 1) {
              const h = rt.tails[c.ch];
              sub.textContent = `PP:${(Math.max(...h) - Math.min(...h)).toFixed(dec)}`;
            } else {
              sub.textContent = "";
            }
          });
        } else if (w.type === "arc_gauge") {
          const cvs = document.getElementById(`cvs_${w.id}`);
          if (!cvs) continue;
          const { ctx, cw, ch } = fitCanvas(cvs);
          ctx.clearRect(0, 0, cw, ch);
          const cx = cw * 0.5,
            cy = ch * 0.57,
            r = Math.max(22, Math.min(cw * 0.38, ch * 0.44));
          const a0 = Math.PI * 0.78,
            aSpan = Math.PI * 1.44;
          const min = Number(w.min ?? 0),
            max = Math.max(min + 1e-6, Number(w.max ?? 3600));
          const v = readChannelVal(w.channel),
            m = chMeta(w.channel);
          ctx.lineWidth = Math.max(6, r * 0.17);
          ctx.lineCap = "round";
          ctx.strokeStyle = brdCol;
          ctx.beginPath();
          ctx.arc(cx, cy, r, a0, a0 + aSpan);
          ctx.stroke();
          ctx.strokeStyle = mainCol;
          ctx.beginPath();
          if (w.originMode === "center_zero") {
            const mid = a0 + aSpan * 0.5,
              hs = Math.max(Math.abs(min), Math.abs(max), 1);
            const bi = Math.max(-1, Math.min(1, v / hs));
            ctx.arc(cx, cy, r, Math.min(mid, mid + bi * aSpan * 0.5), Math.max(mid, mid + bi * aSpan * 0.5));
          } else {
            ctx.arc(cx, cy, r, a0, a0 + aSpan * Math.max(0, Math.min(1, (v - min) / (max - min))));
          }
          ctx.stroke();
          if (w.refChannel) {
            const rv = readChannelVal(w.refChannel);
            const ra = a0 + aSpan * Math.max(0, Math.min(1, (rv - min) / (max - min)));
            ctx.strokeStyle = txtCol;
            ctx.lineWidth = 2.2;
            ctx.beginPath();
            ctx.moveTo(cx + Math.cos(ra) * (r - 7), cy + Math.sin(ra) * (r - 7));
            ctx.lineTo(cx + Math.cos(ra) * (r + 7), cy + Math.sin(ra) * (r + 7));
            ctx.stroke();
          }
          ctx.fillStyle = txtCol;
          ctx.font = `bold ${Math.max(13, Math.round(r * 0.33))}px "JetBrains Mono", monospace`;
          ctx.textAlign = "center";
          ctx.fillText(v.toFixed(w.decimals ?? m.dec), cx, cy + 4);
          ctx.fillStyle = mutCol;
          ctx.font = "11px sans-serif";
          ctx.fillText(w.unitOverride || m.unit, cx, cy + 19);
        } else if (w.type === "three_phase") {
          const el = document.getElementById(`wb_${w.id}`);
          if (!el) continue;
          const min = Number(w.min ?? 0),
            max = Math.max(min + 1e-6, Number(w.max ?? 100));
          (w.bars || []).forEach((b, bi) => {
            const v = readChannelVal(b.ch);
            const vEl = el.querySelector(`.tp-v-${bi}`),
              bEl = el.querySelector(`.tp-b-${bi}`);
            if (vEl) vEl.textContent = v.toFixed(1);
            if (bEl) {
              if (w.baselineMode === "center_zero") {
                const hs = Math.max(Math.abs(min), Math.abs(max), 1),
                  r = Math.max(-1, Math.min(1, v / hs));
                bEl.style.bottom = r >= 0 ? "50%" : `${50 + r * 50}%`;
                bEl.style.height = `${Math.abs(r) * 50}%`;
              } else {
                bEl.style.bottom = "0";
                bEl.style.height = `${Math.max(0, Math.min(100, ((v - min) / (max - min)) * 100))}%`;
              }
            }
          });
        } else if (w.type === "angle_compass") {
          const cvs = document.getElementById(`cvs_${w.id}`);
          if (!cvs) continue;
          const { ctx, cw, ch } = fitCanvas(cvs);
          ctx.clearRect(0, 0, cw, ch);
          const cx = cw * 0.5,
            cy = ch * 0.45,
            r = Math.max(20, Math.min(cw, ch) * 0.35);
          const off = w.zeroDir === "right" ? 0 : -90;
          const deg1 = toDeg360(readChannelVal(w.channel), w.angleUnit, w.encoderCpr);
          ctx.strokeStyle = brdCol;
          ctx.lineWidth = 1.4;
          ctx.beginPath();
          ctx.arc(cx, cy, r, 0, Math.PI * 2);
          ctx.stroke();
          let lbl = `${deg1.toFixed(1)}°`;
          if (w.compareChannel) {
            const deg2 = toDeg360(readChannelVal(w.compareChannel), w.angleUnit, w.encoderCpr);
            const r2 = ((deg2 + off) * Math.PI) / 180;
            ctx.strokeStyle = "#F59E0B";
            ctx.lineWidth = 1.8;
            ctx.beginPath();
            ctx.moveTo(cx, cy);
            ctx.lineTo(cx + Math.cos(r2) * (r - 5), cy + Math.sin(r2) * (r - 5));
            ctx.stroke();
            const diff = ((deg1 - deg2 + 540) % 360) - 180;
            lbl = `θ:${deg1.toFixed(0)}° Δ:${diff >= 0 ? "+" : ""}${diff.toFixed(1)}°`;
          }
          const r1 = ((deg1 + off) * Math.PI) / 180;
          ctx.strokeStyle = mainCol;
          ctx.lineWidth = 2.5;
          ctx.beginPath();
          ctx.moveTo(cx, cy);
          ctx.lineTo(cx + Math.cos(r1) * (r - 3), cy + Math.sin(r1) * (r - 3));
          ctx.stroke();
          ctx.fillStyle = txtCol;
          ctx.font = '11.5px "JetBrains Mono", monospace';
          ctx.textAlign = "center";
          ctx.fillText(lbl, cx, ch - 4);
        } else if (w.type === "xy_lissajous") {
          const cvs = document.getElementById(`cvs_${w.id}`);
          if (!cvs) continue;
          const { ctx, cw, ch } = fitCanvas(cvs);
          ctx.clearRect(0, 0, cw, ch);
          const xs = rt.tails[w.xChannel] || [];
          const ys = rt.tails[w.yChannel] || [];
          const nPts = Math.min(xs.length, ys.length);
          let bound = Math.max(1, Number(w.fixedMax) || 50);
          if (w.scaleMode !== "fixed" && nPts > 0) {
            let mx = 1;
            for (let i = 0; i < nPts; i++) {
              mx = Math.max(mx, Math.abs(xs[i]), Math.abs(ys[i]));
            }
            bound = mx * 1.15;
          }
          const cx = cw * 0.5,
            cy = ch * 0.5,
            R = Math.max(20, Math.min(cw, ch) * 0.42);
          ctx.strokeStyle = brdCol;
          ctx.beginPath();
          ctx.moveTo(cx - R, cy);
          ctx.lineTo(cx + R, cy);
          ctx.moveTo(cx, cy - R);
          ctx.lineTo(cx, cy + R);
          ctx.stroke();
          if (nPts > 1) {
            ctx.strokeStyle = mainCol;
            ctx.lineWidth = 1.8;
            ctx.beginPath();
            for (let i = 0; i < nPts; i++) {
              const px = cx + (xs[i] / bound) * R,
                py = cy - (ys[i] / bound) * R;
              if (i === 0) ctx.moveTo(px, py);
              else ctx.lineTo(px, py);
            }
            ctx.stroke();
            const lx = cx + (xs[nPts - 1] / bound) * R,
              ly = cy - (ys[nPts - 1] / bound) * R;
            ctx.fillStyle = mainCol;
            ctx.beginPath();
            ctx.arc(lx, ly, 3, 0, Math.PI * 2);
            ctx.fill();
          }
        } else if (w.type === "svpwm_hex") {
          const cvs = document.getElementById(`cvs_${w.id}`);
          if (!cvs) continue;
          const { ctx, cw, ch } = fitCanvas(cvs);
          ctx.clearRect(0, 0, cw, ch);
          const vdc = Math.max(6, readChannelVal(w.vbusCh || "CH26") || 72);
          let ua = 0,
            ub = 0;
          if (w.inputSource === "dq_theta") {
            const vd = readChannelVal(w.vdCh),
              vq = readChannelVal(w.vqCh),
              th = (readChannelVal(w.thetaCh) * Math.PI) / 180;
            ua = vd * Math.cos(th) - vq * Math.sin(th);
            ub = vd * Math.sin(th) + vq * Math.cos(th);
          } else if (w.inputSource === "duty_abc") {
            const da = readChannelVal(w.dutyACh) / 100,
              db = readChannelVal(w.dutyBCh) / 100,
              dc = readChannelVal(w.dutyCCh) / 100;
            ua = (2 / 3) * vdc * (da - 0.5 * db - 0.5 * dc);
            ub = (2 / 3) * vdc * (0.866025 * (db - dc));
          } else {
            ua = readChannelVal(w.vaCh);
            ub = readChannelVal(w.vbCh);
          }
          const cx = cw * 0.5,
            cy = ch * 0.47,
            R = Math.max(22, Math.min(cw * 0.42, ch * 0.39));
          ctx.strokeStyle = brdCol;
          ctx.lineWidth = 1.3;
          ctx.beginPath();
          for (let k = 0; k < 6; k++) {
            const a = (k * Math.PI) / 3;
            if (k === 0) ctx.moveTo(cx + Math.cos(a) * R, cy - Math.sin(a) * R);
            else ctx.lineTo(cx + Math.cos(a) * R, cy - Math.sin(a) * R);
          }
          ctx.closePath();
          ctx.stroke();
          ctx.setLineDash([3, 3]);
          ctx.strokeStyle = "#10B981";
          ctx.beginPath();
          ctx.arc(cx, cy, R * 0.866025, 0, Math.PI * 2);
          ctx.stroke();
          ctx.setLineDash([]);
          const vFull = (2 / 3) * vdc,
            vx = cx + (ua / vFull) * R,
            vy = cy - (ub / vFull) * R;
          const mod = Math.hypot(ua, ub) / (vdc / 1.73205);
          const sec = Math.floor((((Math.atan2(ub, ua) * 180) / Math.PI + 360) % 360) / 60) + 1;
          ctx.strokeStyle = mainCol;
          ctx.lineWidth = 2.3;
          ctx.beginPath();
          ctx.moveTo(cx, cy);
          ctx.lineTo(vx, vy);
          ctx.stroke();
          ctx.fillStyle = mainCol;
          ctx.beginPath();
          ctx.arc(vx, vy, 3.5, 0, Math.PI * 2);
          ctx.fill();
          ctx.fillStyle = txtCol;
          ctx.font = '11.5px "JetBrains Mono", monospace';
          ctx.textAlign = "center";
          ctx.fillText(`S${sec} · M=${mod.toFixed(2)}`, cx, ch - 4);
        } else if (w.type === "id_iq_circle") {
          const cvs = document.getElementById(`cvs_${w.id}`);
          if (!cvs) continue;
          const { ctx, cw, ch } = fitCanvas(cvs);
          ctx.clearRect(0, 0, cw, ch);
          const cx = cw * 0.5,
            cy = ch * 0.47,
            R = Math.max(22, Math.min(cw * 0.4, ch * 0.38));
          const imax = Math.max(5, Number(w.imaxFixed) || 120);
          ctx.strokeStyle = brdCol;
          ctx.beginPath();
          ctx.moveTo(cx - R - 5, cy);
          ctx.lineTo(cx + R + 5, cy);
          ctx.moveTo(cx, cy - R - 5);
          ctx.lineTo(cx, cy + R + 5);
          ctx.stroke();
          ctx.setLineDash([4, 3]);
          ctx.strokeStyle = "#F59E0B";
          ctx.beginPath();
          ctx.arc(cx, cy, R, 0, Math.PI * 2);
          ctx.stroke();
          ctx.setLineDash([]);
          const xv = readChannelVal(w.idCh),
            yv = readChannelVal(w.iqCh);
          const px = cx + (xv / imax) * R,
            py = cy - (yv / imax) * R;
          ctx.strokeStyle = mainCol;
          ctx.lineWidth = 2;
          ctx.beginPath();
          ctx.moveTo(cx, cy);
          ctx.lineTo(px, py);
          ctx.stroke();
          ctx.fillStyle = mainCol;
          ctx.beginPath();
          ctx.arc(px, py, 3.8, 0, Math.PI * 2);
          ctx.fill();
          ctx.fillStyle = txtCol;
          ctx.font = '11.5px "JetBrains Mono", monospace';
          ctx.textAlign = "center";
          ctx.fillText(`X:${xv.toFixed(1)} Y:${yv.toFixed(1)}`, cx, ch - 4);
        } else if (w.type === "isr_stacked") {
          const el = document.getElementById(`wb_${w.id}`);
          if (!el) continue;
          const budget = Math.max(10, Number(w.budget) || 3000);
          let sum = 0;
          (w.stages || []).forEach((s, si) => {
            const v = Math.max(0, readChannelVal(s.ch));
            sum += v;
            const seg = el.querySelector(`[data-si="${si}"]`);
            if (seg) seg.style.width = `${(v / budget) * 100}%`;
            const lb = el.querySelector(`.st-v-${si}`);
            if (lb) lb.textContent = v.toFixed(0);
          });
          const tot = w.totalCh ? Math.max(sum, readChannelVal(w.totalCh)) : sum;
          el.querySelector(".seg-rest").style.width = `${(Math.max(0, tot - sum) / budget) * 100}%`;
          const pct = (tot / budget) * 100;
          el.querySelector(".isr-tot").textContent = `${tot.toFixed(0)} / ${budget} ${w.budgetUnit || ""}`;
          el.querySelector(".isr-pct").textContent = `${pct.toFixed(1)}%`;
          el.querySelector(".isr-pct").style.color =
            pct > 80 ? "var(--danger)" : pct > 60 ? "var(--warn)" : "var(--ok)";
        } else if (w.type === "ref_fbk_err") {
          const el = document.getElementById(`wb_${w.id}`);
          if (!el) continue;
          const r = readChannelVal(w.refCh),
            f = readChannelVal(w.fbkCh),
            diff = f - r;
          const span = Math.max(1, (Number(w.max) || 3600) - (Number(w.min) || 0));
          el.querySelector(".rf-r").textContent = r.toFixed(1);
          el.querySelector(".rf-f").textContent = f.toFixed(1);
          const eEl = el.querySelector(".rf-e");
          eEl.textContent = `Δ ${diff >= 0 ? "+" : ""}${diff.toFixed(1)}`;
          eEl.style.color = mainCol;
          el.querySelector(".rf-br").style.width = `${Math.max(0, Math.min(100, (r / span) * 100))}%`;
          const bf = el.querySelector(".rf-bf");
          bf.style.background = mainCol;
          bf.style.width = `${Math.max(0, Math.min(100, (f / span) * 100))}%`;
        } else if (w.type === "spark_stat") {
          const el = document.getElementById(`wb_${w.id}`);
          if (!el) continue;
          const m = chMeta(w.channel),
            arr = rt.tails[w.channel] || [];
          const cur = readChannelVal(w.channel);
          el.querySelector(".sp-v").textContent = `${cur.toFixed(m.dec)} ${m.unit}`;
          if (arr.length > 1) {
            const mn = Math.min(...arr),
              mx = Math.max(...arr);
            el.querySelector(".sp-st").textContent =
              w.statType === "maxmin"
                ? `${mn.toFixed(m.dec)}~${mx.toFixed(m.dec)}`
                : `P-P:${(mx - mn).toFixed(m.dec)}`;
            const cvs = el.querySelector(".sp-c"),
              ctx = cvs.getContext("2d");
            ctx.clearRect(0, 0, cvs.width, cvs.height);
            const span = Math.max(1e-4, mx - mn);
            const lo = mn - span * 0.12,
              hi = mx + span * 0.12;
            ctx.strokeStyle = mainCol;
            ctx.lineWidth = 1.7;
            ctx.beginPath();
            arr.forEach((v, i) => {
              const x = (i / (arr.length - 1)) * cvs.width;
              const y = cvs.height - Math.max(0, Math.min(1, (v - lo) / (hi - lo))) * (cvs.height - 6) - 3;
              if (i === 0) ctx.moveTo(x, y);
              else ctx.lineTo(x, y);
            });
            ctx.stroke();
          }
        } else if (w.type === "quick_visibility") {
          const el = document.getElementById(`wb_${w.id}`);
          if (!el) continue;
          el.querySelectorAll(".qv-n").forEach((sp) => {
            const k = sp.dataset.ck,
              m = chMeta(k);
            sp.textContent = readChannelVal(k).toFixed(m.dec);
          });
        }
      }
    }

    function bindDrawerInteractions() {
      const container = document.getElementById("drawerWidgetsCanvas");
      const p = prof();
      if (!container || !p) return;
      const unlocked = !p.layout_locked || hooks.getActivePage() === "config";

      container.querySelectorAll(".dw-box").forEach((box) => {
        box.addEventListener("click", (e) => {
          if (box._justDragged) return;
          if (e.target.closest("input, select, button, label")) return;
          rt.selectedId = box.dataset.wid;
          if (hooks.getActivePage() === "config") {
            renderInstanceList();
            renderInspector();
            container
              .querySelectorAll(".dw-box")
              .forEach((b) =>
                b.classList.toggle("selected-outline", String(b.dataset.wid) === String(rt.selectedId))
              );
          }
        });

        // Pure pointer drag-to-reorder (100% WebView2 safe, zero HTML5 drag-drop dependency)
        box.addEventListener("mousedown", (e) => {
          if (!unlocked || e.button !== 0 || box.classList.contains("resizing")) return;
          if (
            e.target.closest(
              ".dw-grip-r, .dw-grip-b, .dw-grip-br, input, select, button, label"
            )
          ) {
            return;
          }
          const startX = e.clientX;
          const startY = e.clientY;
          const r0 = box.getBoundingClientRect();
          const offX = startX - r0.left;
          const offY = startY - r0.top;
          let dragging = false;
          let ghost = null;

          const onMove = (ev) => {
            if (!dragging) {
              if (Math.hypot(ev.clientX - startX, ev.clientY - startY) < 5) return;
              dragging = true;
              box.classList.add("dw-drag-placeholder");
              ghost = box.cloneNode(true);
              ghost.classList.remove("dw-drag-placeholder", "selected-outline");
              const srcCvs = box.querySelector("canvas");
              const dstCvs = ghost.querySelector("canvas");
              if (srcCvs && dstCvs && srcCvs.width > 0 && srcCvs.height > 0) {
                dstCvs.width = srcCvs.width;
                dstCvs.height = srcCvs.height;
                dstCvs.getContext("2d").drawImage(srcCvs, 0, 0);
              }
              Object.assign(ghost.style, {
                position: "fixed",
                left: r0.left + "px",
                top: r0.top + "px",
                width: r0.width + "px",
                height: r0.height + "px",
                margin: "0",
                pointerEvents: "none",
                zIndex: "9999",
                opacity: "0.92",
                border: "1.5px solid var(--accent)",
                boxShadow: "0 12px 28px rgba(0, 0, 0, 0.42)",
                cursor: "grabbing",
              });
              document.body.appendChild(ghost);
              document.body.style.cursor = "grabbing";
            }

            ghost.style.left = ev.clientX - offX + "px";
            ghost.style.top = ev.clientY - offY + "px";

            const cr = container.getBoundingClientRect();
            if (ev.clientY < cr.top + 32) container.scrollTop -= 10;
            else if (ev.clientY > cr.bottom - 32) container.scrollTop += 10;

            const siblings = Array.from(container.querySelectorAll(".dw-box"));
            let swapped = false;
            for (const sib of siblings) {
              if (sib === box) continue;
              const r = sib.getBoundingClientRect();
              if (
                ev.clientX >= r.left - 4 &&
                ev.clientX <= r.right + 4 &&
                ev.clientY >= r.top - 4 &&
                ev.clientY <= r.bottom + 4
              ) {
                const listNow = Array.from(container.children);
                const bi = listNow.indexOf(box);
                const si = listNow.indexOf(sib);
                const sameRow = Math.abs(r.top - box.getBoundingClientRect().top) < 24;
                if (bi < si) {
                  const pass = sameRow
                    ? ev.clientX > r.left + r.width * 0.35
                    : ev.clientY > r.top + r.height * 0.35;
                  if (pass) sib.after(box);
                } else {
                  const pass = sameRow
                    ? ev.clientX < r.left + r.width * 0.65
                    : ev.clientY < r.top + r.height * 0.65;
                  if (pass) sib.before(box);
                }
                swapped = true;
                break;
              }
            }
            if (!swapped && siblings.length > 1 && ev.clientX >= cr.left && ev.clientX <= cr.right) {
              const others = siblings.filter((s) => s !== box);
              const firstR = others[0].getBoundingClientRect();
              const lastR = others[others.length - 1].getBoundingClientRect();
              if (ev.clientY < firstR.top) {
                others[0].before(box);
              } else if (ev.clientY > lastR.bottom) {
                others[others.length - 1].after(box);
              }
            }
          };

          const onUp = () => {
            window.removeEventListener("mousemove", onMove);
            window.removeEventListener("mouseup", onUp);
            if (!dragging) return;
            document.body.style.cursor = "";
            if (ghost) ghost.remove();
            box.classList.remove("dw-drag-placeholder");
            box._justDragged = true;
            setTimeout(() => {
              box._justDragged = false;
            }, 0);

            const domIds = Array.from(container.querySelectorAll(".dw-box")).map((b) =>
              String(b.dataset.wid)
            );
            const enabledMap = new Map(
              p.drawer_widgets
                .filter((w) => w.enabled !== false)
                .map((w) => [String(w.id), w])
            );
            let ei = 0;
            p.drawer_widgets = p.drawer_widgets.map((w) => {
              if (w.enabled === false) return w;
              return enabledMap.get(domIds[ei++]) || w;
            });

            renderInstanceList();
            renderDrawerStructure();
            commitPersistNow();
          };

          window.addEventListener("mousemove", onMove);
          window.addEventListener("mouseup", onUp);
        });
      });

      // Quick visibility checkboxes inside `quick_visibility` widget
      container.querySelectorAll("[data-qv-ch], [data-qv-math]").forEach((chk) => {
        chk.addEventListener("change", () => {
          if (chk.dataset.qvCh !== undefined) {
            const ch = p.channels[Number(chk.dataset.qvCh)];
            if (ch) {
              ch.visible = chk.checked;
              if (chk.checked && ch.plot_id === "none" && p.plots && p.plots.length > 0) {
                ch.plot_id = p.plots[0].id;
              }
            }
          } else if (chk.dataset.qvMath !== undefined) {
            const mc = p.math_channels[Number(chk.dataset.qvMath)];
            if (mc) {
              mc.visible = chk.checked;
              if (chk.checked && mc.plot_id === "none" && p.plots && p.plots.length > 0) {
                mc.plot_id = p.plots[0].id;
              }
            }
          }
          commitPersistNow();
        });
      });

      const attachResize = (sel, attr, doW, doH) => {
        container.querySelectorAll(sel).forEach((grip) => {
          const wid = grip.dataset[attr];
          grip.addEventListener("dblclick", (e) => {
            e.stopPropagation();
            const w = p.drawer_widgets.find((x) => String(x.id) === String(wid));
            if (!w) return;
            if (doW && !doH) w.widthPct = (w.widthPct || 100) < 95 ? 100 : 50;
            if (doH && !doW) w.height = null;
            renderInstanceList();
            if (hooks.getActivePage() === "config") renderInspector();
            renderDrawerStructure();
            commitPersistNow();
          });
          grip.addEventListener("mousedown", (e) => {
            e.preventDefault();
            e.stopPropagation();
            const box = grip.closest(".dw-box"),
              hud = box.querySelector(".dw-size-hud"),
              wObj = p.drawer_widgets.find((x) => String(x.id) === String(wid));
            if (!wObj) return;
            box.draggable = false;
            box.classList.add("resizing");
            const contW = Math.max(200, container.clientWidth - 12);
            const sx = e.clientX,
              sy = e.clientY,
              sw = box.getBoundingClientRect().width,
              sh = box.getBoundingClientRect().height;
            const onMove = (ev) => {
              if (doW) {
                let pct = Math.max(
                  30,
                  Math.min(100, Math.round(((sw + ev.clientX - sx) / contW) * 100))
                );
                for (const sp of [33, 50, 67, 100]) if (Math.abs(pct - sp) <= 2) pct = sp;
                wObj.widthPct = pct;
                box.style.width = `calc(${pct}% - 8px)`;
              }
              if (doH) {
                wObj.height = Math.max(52, Math.round(sh + ev.clientY - sy));
                box.style.height = `${wObj.height}px`;
              }
              if (hud) {
                hud.textContent = `${wObj.widthPct || 100}% × ${
                  wObj.height ? wObj.height + "px" : "自适应"
                }`;
              }
              updateDrawerLive();
            };
            const onUp = () => {
              box.classList.remove("resizing");
              box.draggable = !p.layout_locked;
              if (hooks.getActivePage() === "config" && String(wObj.id) === String(rt.selectedId)) {
                renderInspector();
              }
              window.removeEventListener("mousemove", onMove);
              window.removeEventListener("mouseup", onUp);
              commitPersistNow();
            };
            window.addEventListener("mousemove", onMove);
            window.addEventListener("mouseup", onUp);
          });
        });
      };
      attachResize("[data-rgrip]", "rgrip", true, false);
      attachResize("[data-bgrip]", "bgrip", false, true);
      attachResize("[data-brgrip]", "brgrip", true, true);
    }

    // ==================== STATIC EVENT BINDINGS (ONCE ON BOOT) ====================
    function bindGlobalEvents() {
      const exprPop = document.getElementById("exprHelpPop");
      const wgPop = document.getElementById("widgetGuidePop");
      const pickerModal = document.getElementById("widgetPickerModal");
      const pickerGrid = document.getElementById("widgetPickerGrid");

      document.getElementById("btnCloseExprPop").addEventListener("click", () => {
        exprPop.classList.remove("open");
      });
      document.getElementById("btnCloseWgPop").addEventListener("click", () => {
        wgPop.classList.remove("open");
      });
      exprPop.addEventListener("click", (e) => {
        e.stopPropagation();
        const chip = e.target.closest("[data-tpl]");
        if (chip && rt.activeHelpInput) {
          rt.activeHelpInput.value = chip.dataset.tpl;
          rt.activeHelpInput.dispatchEvent(new Event("input", { bubbles: true }));
          exprPop.classList.remove("open");
        }
      });
      wgPop.addEventListener("click", (e) => e.stopPropagation());
      window.addEventListener("click", () => {
        exprPop.classList.remove("open");
        wgPop.classList.remove("open");
      });

      // Instance list clicks & pointer drag-reorder
      const instListEl = document.getElementById("widgetInstList");
      instListEl.addEventListener("mousedown", (e) => {
        if (e.button !== 0) return;
        if (e.target.closest("input, button")) return;
        const item = e.target.closest("[data-sel-wid]");
        const p = prof();
        if (!item || !p || !Array.isArray(p.drawer_widgets)) return;

        const startY = e.clientY;
        const r0 = item.getBoundingClientRect();
        const offY = startY - r0.top;
        let dragging = false;
        let ghost = null;

        const onMove = (ev) => {
          if (!dragging) {
            if (Math.abs(ev.clientY - startY) < 5) return;
            dragging = true;
            item.classList.add("wcfg-drag-placeholder");
            ghost = item.cloneNode(true);
            ghost.classList.remove("wcfg-drag-placeholder");
            Object.assign(ghost.style, {
              position: "fixed",
              left: r0.left + "px",
              top: r0.top + "px",
              width: r0.width + "px",
              margin: "0",
              pointerEvents: "none",
              zIndex: "9999",
              opacity: "0.94",
              border: "1px solid var(--accent)",
              boxShadow: "0 8px 20px rgba(0, 0, 0, 0.35)",
              cursor: "grabbing",
            });
            document.body.appendChild(ghost);
          }
          ghost.style.top = ev.clientY - offY + "px";

          const lr = instListEl.getBoundingClientRect();
          if (ev.clientY < lr.top + 24) instListEl.scrollTop -= 8;
          else if (ev.clientY > lr.bottom - 24) instListEl.scrollTop += 8;

          const items = Array.from(instListEl.querySelectorAll("[data-sel-wid]"));
          for (const sib of items) {
            if (sib === item) continue;
            const r = sib.getBoundingClientRect();
            if (ev.clientY >= r.top && ev.clientY <= r.bottom) {
              const ii = items.indexOf(item);
              const si = items.indexOf(sib);
              if (ii < si && ev.clientY > r.top + r.height * 0.4) sib.after(item);
              else if (ii > si && ev.clientY < r.top + r.height * 0.6) sib.before(item);
              break;
            }
          }
        };

        const onUp = () => {
          window.removeEventListener("mousemove", onMove);
          window.removeEventListener("mouseup", onUp);
          if (!dragging) return;
          if (ghost) ghost.remove();
          item.classList.remove("wcfg-drag-placeholder");
          item._justDragged = true;
          setTimeout(() => {
            item._justDragged = false;
          }, 0);

          const orderedIds = Array.from(instListEl.querySelectorAll("[data-sel-wid]")).map((el) =>
            String(el.dataset.selWid)
          );
          const mapById = new Map(p.drawer_widgets.map((w) => [String(w.id), w]));
          p.drawer_widgets = orderedIds.map((id) => mapById.get(id)).filter(Boolean);
          renderInstanceList();
          renderDrawerStructure();
          commitPersistNow();
        };

        window.addEventListener("mousemove", onMove);
        window.addEventListener("mouseup", onUp);
      });

      instListEl.addEventListener("click", (e) => {
        const p = prof();
        if (!p) return;
        const mv = e.target.closest("[data-move-wid]");
        if (mv) {
          e.stopPropagation();
          const [wid, dStr] = mv.dataset.moveWid.split(":");
          const idx = (p.drawer_widgets || []).findIndex((w) => String(w.id) === String(wid));
          const dst = idx + Number(dStr);
          if (idx >= 0 && dst >= 0 && dst < p.drawer_widgets.length) {
            const [moved] = p.drawer_widgets.splice(idx, 1);
            p.drawer_widgets.splice(dst, 0, moved);
            renderInstanceList();
            renderDrawerStructure();
            commitPersistNow();
          }
          return;
        }
        const wgBtn = e.target.closest("[data-wg-help]");
        if (wgBtn) {
          e.stopPropagation();
          showWidgetGuide(wgBtn.dataset.wgHelp, wgBtn);
          return;
        }
        const del = e.target.closest("[data-del-wid]");
        if (del) {
          e.stopPropagation();
          const wid = del.dataset.delWid;
          p.drawer_widgets = (p.drawer_widgets || []).filter((w) => String(w.id) !== String(wid));
          if (String(rt.selectedId) === String(wid)) {
            rt.selectedId = p.drawer_widgets[0] ? String(p.drawer_widgets[0].id) : null;
          }
          renderInstanceList();
          renderInspector();
          renderDrawerStructure();
          commitPersistNow();
          return;
        }
        if (e.target.dataset.toggleWid !== undefined) {
          e.stopPropagation();
          const w = (p.drawer_widgets || []).find(
            (x) => String(x.id) === String(e.target.dataset.toggleWid)
          );
          if (w) {
            w.enabled = e.target.checked;
            renderDrawerStructure();
            commitPersistNow();
          }
          return;
        }
        const item = e.target.closest("[data-sel-wid]");
        if (item && !item._justDragged) {
          rt.selectedId = item.dataset.selWid;
          renderInstanceList();
          renderInspector();
          renderDrawerStructure();
          const targetCard = document.getElementById(`wcard_${rt.selectedId}`);
          if (targetCard) targetCard.scrollIntoView({ behavior: "smooth", block: "nearest" });
        }
      });

      // Picker modal
      pickerGrid.innerHTML = WIDGET_TYPES.map(
        (t) => `
        <div class="wcfg-picker-item" data-pick-type="${esc(t.type)}">
          <div style="display:flex;align-items:center;justify-content:space-between;gap:6px">
            <strong style="font-size:12.5px">${esc(t.name)}</strong>
            <button class="wcfg-q-btn" data-picker-help="${esc(t.type)}" title="查看适用变量与显示效果">?</button>
          </div>
          <span class="tb-label" style="white-space:normal">${esc(t.brief)}</span>
        </div>`
      ).join("");

      document.getElementById("btnAddDrawerWidget").addEventListener("click", () => {
        pickerModal.classList.add("open");
      });
      document.getElementById("btnCloseWidgetPicker").addEventListener("click", () => {
        pickerModal.classList.remove("open");
      });
      pickerGrid.addEventListener("click", (e) => {
        const hBtn = e.target.closest("[data-picker-help]");
        if (hBtn) {
          e.stopPropagation();
          showWidgetGuide(hBtn.dataset.pickerHelp, hBtn);
          return;
        }
        const card = e.target.closest("[data-pick-type]");
        if (!card) return;
        const p = prof();
        if (!p) return;
        ensureWidgetsInitialized();
        const inst = createDefaultWidgetInstance(card.dataset.pickType);
        p.drawer_widgets.push(inst);
        rt.selectedId = inst.id;
        pickerModal.classList.remove("open");
        renderInstanceList();
        renderInspector();
        renderDrawerStructure();
        const targetCard = document.getElementById(`wcard_${rt.selectedId}`);
        if (targetCard) targetCard.scrollIntoView({ behavior: "smooth", block: "nearest" });
        commitPersistNow();
      });

      // Shared Right-Drawer Width Splitter
      const splitter = document.getElementById("drawerWidthSplitter");
      const drawer = document.getElementById("rightDrawer");
      if (splitter && drawer) {
        splitter.addEventListener("mousedown", (e) => {
          const p = prof();
          if (!p) return;
          e.preventDefault();
          splitter.classList.add("dragging");
          const sx = e.clientX;
          const sw = drawer.getBoundingClientRect().width;
          const onMove = (ev) => {
            const nw = Math.max(240, Math.min(700, Math.round(sw - (ev.clientX - sx))));
            p.right_drawer_width = nw;
            drawer.style.width = nw + "px";
            updateDrawerLive();
            if (hooks.onDrawerResized) hooks.onDrawerResized();
          };
          const onUp = () => {
            splitter.classList.remove("dragging");
            window.removeEventListener("mousemove", onMove);
            window.removeEventListener("mouseup", onUp);
            commitPersistNow();
          };
          window.addEventListener("mousemove", onMove);
          window.addEventListener("mouseup", onUp);
        });
      }
    }

    // ==================== PUBLIC CONTROLLER API ====================
    return {
      init() {
        bindGlobalEvents();
      },
      syncAll() {
        const p = prof();
        if (!p) return;
        const drawer = document.getElementById("rightDrawer");
        if (drawer) {
          drawer.style.width = Math.max(240, Math.min(700, Number(p.right_drawer_width) || 356)) + "px";
        }
        renderInstanceList();
        renderInspector();
        renderDrawerStructure();
      },
      ingestSnapshot(snap) {
        if (!snap) return;
        if (snap.channel_vals && Object.keys(snap.channel_vals).length > 0) {
          rt.vals = snap.channel_vals;
        } else if (Array.isArray(snap.cards)) {
          const next = {};
          for (const c of snap.cards) next[c.key] = c.raw_value;
          rt.vals = next;
        }
        if (snap.channel_peaks) rt.peaks = snap.channel_peaks;
        if (snap.channel_mins) rt.mins = snap.channel_mins;
        if (snap.series_tails) rt.tails = snap.series_tails;
        updateDrawerLive();
        updateInspectorRuleHitBadges();
      },
      refreshLayoutLock() {
        renderDrawerStructure();
      },
    };
  }

  window.FocWidgets = { createWidgetController };
})();
