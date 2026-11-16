use crate::engine::{decimate_minmax, TelemetryEngine};
use crate::math_eval::compile_expression;
use crate::models::{
    create_default_profile, default_profile_path, profiles_dir, screenshots_dir, slices_dir,
    EnumEntry, MathChannelConfig, PlotPanelConfig, StudioProfile, TRACE_PALETTE,
};
use crate::post_analysis::{
    compute_auto_shift_sec, compute_full_stats, compute_harmonic_thd, compute_overlay_diff,
    compute_step_metrics, compute_xy_envelope, evaluate_digital_filter, find_series,
    load_bundle_typed,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub const MCP_HTTP_ADDR: &str = "127.0.0.1:18580";

pub struct GuiHooks {
    pub notify_ui: Box<dyn Fn(&str) + Send + Sync>,
    pub send_ui_cmd: Box<dyn Fn(Value) + Send + Sync>,
    pub bring_to_front: Box<dyn Fn() + Send + Sync>,
    pub capture_png: Box<dyn Fn(&str) -> Result<PathBuf, String> + Send + Sync>,
}

pub fn resolve_mcp_exe_path() -> PathBuf {
    resolve_gui_exe_path()
}

pub fn resolve_gui_exe_path() -> PathBuf {
    if let Ok(cur) = std::env::current_exe() {
        return cur;
    }
    PathBuf::from(if cfg!(windows) {
        "FluxScope.exe"
    } else {
        "FluxScope"
    })
}

pub fn build_universal_ai_prompt() -> String {
    let app_bin = resolve_gui_exe_path()
        .to_string_lossy()
        .replace('\\', "/");
    format!(
        r#"请安装并连接我电脑上的 FluxScope 电控高速示波与静态分析上位机 MCP 服务（共 16 个读写接口）：

1. 方式一：单文件 Stdio 模式（推荐，支持界面在线实时联动 + 界面关闭时无头离线读写与一键唤醒）
   - command: {app_bin}
   - args: ["--mcp"]
   - JSON 配置（Cursor / Windsurf / Claude Desktop / Antigravity）：
     {{
       "mcpServers": {{
         "fluxscope": {{
           "command": "{app_bin}",
           "args": ["--mcp"]
         }}
       }}
     }}
   - Claude Code 一键添加：
     claude mcp add fluxscope -- "{app_bin}" --mcp

2. 方式二：HTTP 免路径直连（当 FluxScope 桌面窗口打开时自动在线）
   - url: http://127.0.0.1:18580/mcp

连接后，当你修改固件通信通道时请同步更新上位机通道、显示增益、派生公式与右侧栏仪表控件；进行控制环路调参或噪声谐波排查时，可直接调用阶跃整定 PK、加窗 FFT/THD 诊断、离线滤波器设计（生成下位机 C 代码）与画布截图接口。"#
    )
}

fn widget_catalog_reference() -> Value {
    json!({
        "rule_schema": {
            "description": "colorRules / frameRules / cards[].rules 数组按索引从上到下代表优先级 P1 > P2 > P3，命中首条即生效",
            "builder_mode_example": {
                "mode": "builder",
                "leftVar": "val",
                "op": ">=",
                "rightMode": "const",
                "rightConst": 80.0,
                "rightVar": "CH0",
                "color": "#EF4444",
                "alertFrame": true,
                "frameColor": "#EF4444"
            },
            "expr_mode_example": {
                "mode": "expr",
                "expr": "abs(CH0 - CH1) > 200 || (CH7 & 0x04) != 0",
                "color": "#F59E0B",
                "alertFrame": false
            }
        },
        "widget_types": [
            { "type": "enum_badge", "desc": "状态枚举徽章", "fields": ["title", "widthPct(50|100)", "channel", "unmatchedLabel", "unmatchedColor", "enumMap:[{k,label,color}]", "frameRules"] },
            { "type": "bitmask_led", "desc": "位掩码/多条件状态灯阵", "fields": ["title", "channel", "decodeMode('bitmask'|'expr')", "displayStyle('led_grid'|'active_pills')", "gridCols(1..4)", "okText", "items:[{key,label,color,expr}]", "frameRules"] },
            { "type": "linear_bar", "desc": "单向/零位双向线性进度条", "fields": ["title", "channel", "min", "max", "originMode('left'|'zero')", "decimals", "unitOverride", "showValue", "showPct", "normalColorMode('channel'|'fixed')", "baseColor", "colorRules"] },
            { "type": "num_grid", "desc": "多通道数值网格", "fields": ["title", "cols(1..3)", "cards:[{ch,dec,sub('none'|'peak'|'min'|'pp'),rules}]", "frameRules"] },
            { "type": "arc_gauge", "desc": "240°圆弧表盘(含可选参考针)", "fields": ["title", "widthPct", "height", "channel", "refChannel", "min", "max", "originMode('left'|'top_zero')", "decimals", "colorRules"] },
            { "type": "three_phase", "desc": "多路并排立柱图(2~6路)", "fields": ["title", "widthPct", "height", "min", "max", "baselineMode('bottom'|'zero')", "bars:[{label,ch,color}]", "frameRules"] },
            { "type": "angle_compass", "desc": "360°极坐标双针角度罗盘", "fields": ["title", "widthPct", "height", "channel", "compareChannel", "angleUnit('deg360'|'deg180'|'rad2pi'|'radpi'|'enc')", "encoderCpr", "colorRules"] },
            { "type": "xy_lissajous", "desc": "侧栏实时X-Y平面轨迹", "fields": ["title", "widthPct", "height", "xChannel", "yChannel", "scaleMode('auto'|'fixed')", "fixedMax", "tailPoints", "baseColor", "colorRules"] },
            { "type": "spark_stat", "desc": "滚动微趋势统计卡(Sparkline)", "fields": ["title", "channel", "windowPts", "statType('pp'|'minmax')", "colorRules"] },
            { "type": "quick_visibility", "desc": "示波曲线快捷显隐开关面板", "fields": ["title", "cols(1..3)", "showLiveNum"] },
            { "type": "svpwm_hex", "desc": "SVPWM空间矢量六边形与调制比", "fields": ["title", "widthPct", "height", "inputSource('dq_theta'|'ab'|'duty')", "vdCh", "vqCh", "thetaCh", "vaCh", "vbCh", "dutyACh", "dutyBCh", "dutyCCh", "vbusCh", "colorRules"] },
            { "type": "id_iq_circle", "desc": "四象限圆形限幅矢量图(Id-Iq)", "fields": ["title", "widthPct", "height", "idCh", "iqCh", "imaxFixed", "colorRules"] },
            { "type": "isr_stacked", "desc": "多阶段预算占比堆栈条", "fields": ["title", "budget", "budgetUnit", "totalCh", "stages:[{label,ch,color}]", "frameRules"] },
            { "type": "ref_fbk_err", "desc": "给定与反馈双轨跟随误差条", "fields": ["title", "refCh", "fbkCh", "min", "max", "refColor", "baseColor", "colorRules"] }
        ]
    })
}

pub fn mcp_tools_schema() -> Vec<Value> {
    vec![
        json!({
            "name": "foc_get_status",
            "description": "获取 FluxScope 当前运行状态、链路参数、采样率、时基跟随状态、右侧栏取值基准、指定时刻或最新帧的全通道（含硬件CH与公式M通道）精确读数、状态/故障解码、自动跳变书签、终端日志及归档实验切片摘要。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "inspect_time": {
                        "type": "number",
                        "description": "可选：指定读取某一秒时刻的精确全通道快照；省略则读取最新一帧。"
                    },
                    "console_tail": {
                        "type": "integer",
                        "description": "返回的终端最新行数上限（默认 40，最大 500）。"
                    }
                }
            }
        }),
        json!({
            "name": "foc_get_channels_and_plots",
            "description": "读取完整的 FluxScope 当前配置：所有硬件通道(CH0..CHn，含 display_gain 显示增益)、PC端派生公式通道(M0..Mn)、分屏波形窗、右侧栏 14 类通用仪表控件列表、时基跟随参数、主题与坏点过滤配置。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "include_widget_catalog": {
                        "type": "boolean",
                        "description": "设为 true 时额外附带右侧栏 14 类控件字段规范与多优先级条件变色规则模板。"
                    }
                }
            }
        }),
        json!({
            "name": "foc_configure_channels",
            "description": "【写配置】批量新增或修改硬件通道配置（名称、公式简称、单位、示波器独立显示增益 display_gain、颜色、线宽、绑定波形窗、枚举/位掩码解码字典）及兜底采样率。修改固件发送协议后可一键同步。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "channels": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "index": { "type": "integer" },
                                "name": { "type": "string" },
                                "alias": { "type": "string" },
                                "unit": { "type": "string" },
                                "display_gain": {
                                    "type": "number",
                                    "description": "仅作用于示波器曲线绘制高度的乘数增益（默认 1.0），不影响图例数值、侧栏控件、派生公式与导出数据。"
                                },
                                "color": { "type": "string" },
                                "width": { "type": "number" },
                                "visible": { "type": "boolean" },
                                "plot_id": { "type": "string" },
                                "show_card": { "type": "boolean" },
                                "decimals": { "type": "integer" },
                                "decoder_type": { "type": "string", "enum": ["float", "enum", "bitmask"] },
                                "enum_map": { "type": "object" },
                                "bitmask_map": { "type": "object" }
                            },
                            "required": ["index"]
                        }
                    },
                    "truncate_to_count": { "type": "integer" },
                    "nominal_sample_hz": {
                        "type": "number",
                        "description": "可选：导入无时间列外部旧 CSV 时的兜底采样率 (Hz)。"
                    }
                },
                "required": ["channels"]
            }
        }),
        json!({
            "name": "foc_upsert_math_channel",
            "description": "【写配置】新增或更新上位机端实时向量派生公式通道（支持 CH0..CHn 或简称，支持 ^ 与 sqrt, abs, sin, cos, atan2, hypot, maximum, minimum, clip, lpf(x,0.05), movavg(x,20), rms_win(x,50), diff(x,0.001)）。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "alias": { "type": "string" },
                    "expression": { "type": "string" },
                    "unit": { "type": "string" },
                    "color": { "type": "string" },
                    "width": { "type": "number" },
                    "visible": { "type": "boolean" },
                    "plot_id": { "type": "string" },
                    "show_card": { "type": "boolean" },
                    "decimals": { "type": "integer" }
                },
                "required": ["name", "expression"]
            }
        }),
        json!({
            "name": "foc_remove_math_channel",
            "description": "【写配置】删除指定的派生计算通道（按 ID 如 M0 或简称如 mod_idx）。",
            "inputSchema": {
                "type": "object",
                "properties": { "id_or_alias": { "type": "string" } },
                "required": ["id_or_alias"]
            }
        }),
        json!({
            "name": "foc_manage_plots_and_layout",
            "description": "【写配置】增删重命名分屏波形窗与高度权重、管理右侧栏开关/宽度/取值源(latest|hover|t1|t2)及 14 类可拖拽通用仪表控件(增改/删除/重排顺序/重置默认)、配置底部时基跟随(跟随开关、10%~100%最新帧锚点位置、最近Ns窗口)、切换深浅主题或布局锁。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "add_or_update_plots": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "title": { "type": "string" },
                                "y_label": { "type": "string" },
                                "height_weight": { "type": "number" }
                            },
                            "required": ["id", "title"]
                        }
                    },
                    "remove_plot_ids": { "type": "array", "items": { "type": "string" } },
                    "right_drawer_open": { "type": "boolean" },
                    "right_drawer_width": { "type": "integer" },
                    "drawer_data_source": { "type": "string", "enum": ["latest", "hover", "t1", "t2"] },
                    "upsert_widgets": { "type": "array", "items": { "type": "object" } },
                    "remove_widget_ids": { "type": "array", "items": { "type": "string" } },
                    "reorder_widget_ids": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "按给定 ID 顺序重排右侧栏控件上下位置（未列出的控件自动保留在末尾）。"
                    },
                    "replace_widgets": { "type": "array", "items": { "type": "object" } },
                    "reset_default_widgets": { "type": "boolean" },
                    "follow_enabled": { "type": "boolean", "description": "底部时基『跟随最新』开关" },
                    "follow_anchor_ratio": { "type": "number", "description": "最新一帧在示波器水平落点比例 (0.10 ~ 1.00，如 1.0=最右侧100%，0.5=居中50%)" },
                    "follow_span_sec": { "type": "number", "description": "最左侧到最新一帧显示的时间跨度秒数 (如 5.0)" },
                    "follow_custom_sec": { "type": "number", "description": "底部自定义快捷秒数数值" },
                    "status_decode": { "type": "object" },
                    "xy_config": { "type": "object" },
                    "layout_locked": { "type": "boolean" },
                    "theme": { "type": "string", "enum": ["dark", "light"] }
                }
            }
        }),
        json!({
            "name": "foc_query_waveform",
            "description": "读取实时示波缓冲（cursors 游标区间 / latest 最近N秒 / full 全部 / custom 自定义t1~t2）或指定历史切片 (slice_id) 的统计特征与降采样波形序列。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "slice_id": { "type": "string", "description": "可选：若传入归档切片 ID 或名称，则直接查询该实验记录而非实时示波缓冲。" },
                    "window": { "type": "string", "enum": ["cursors", "latest", "full", "custom"] },
                    "latest_sec": { "type": "number" },
                    "t1": { "type": "number" },
                    "t2": { "type": "number" },
                    "channels": { "type": "array", "items": { "type": "string" } },
                    "max_points": { "type": "integer" },
                    "include_samples": { "type": "boolean" }
                }
            }
        }),
        json!({
            "name": "foc_compute_fft",
            "description": "对实时缓冲选区 [t1, t2] 或指定实验切片执行加窗 FFT 频谱与 THD 谐波失真分析（支持 hann / flattop / rect 窗与极对数 Pn 谐波阶次诊断）。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "channel": { "type": "string" },
                    "slice_id": { "type": "string", "description": "可选：指定切片 ID/名称或 '__live_buffer__'；不传且给定 t1/t2 时分析示波选区。" },
                    "t1": { "type": "number" },
                    "t2": { "type": "number" },
                    "window_type": { "type": "string", "enum": ["hann", "flattop", "rect"] },
                    "pole_pairs": { "type": "integer", "description": "电机极对数 Pn（默认 7），用于自动映射 1fe/2fe/5fe/6fe/7fe 电谐波成因。" }
                },
                "required": ["channel"]
            }
        }),
        json!({
            "name": "foc_set_cursors_and_slice",
            "description": "控制示波器双游标 [T1, T2]（移动位置、开关显示、缩放聚焦），可选将该选区附带调参备注切入『分析』页数据集池或导出带精确 time_s 时基列的 CSV。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "enabled": { "type": "boolean", "description": "双游标显示开关（默认 true）" },
                    "t1": { "type": "number" },
                    "t2": { "type": "number" },
                    "zoom_to_cursors": { "type": "boolean" },
                    "create_slice_name": { "type": "string" },
                    "create_slice_note": { "type": "string", "description": "切入分析库时的调参备注（如 'Kp=0.8, Ki=120'）" },
                    "open_in_analysis": { "type": "boolean", "description": "切取后是否自动将界面切换到『分析』页聚焦该记录" },
                    "export_csv_path": { "type": "string" }
                },
                "required": ["t1", "t2"]
            }
        }),
        json!({
            "name": "foc_manage_slices",
            "description": "管理『分析』页左侧实验数据集池 (Runs)：列出全部实验与实时缓冲快照、一键抓取全缓冲入库 (capture_full)、直接导入外部 CSV 入库 (import_csv)、修改名称/调参备注(note)/颜色/偏移缩放、自动对齐、对比统计、导出 CSV 或删除。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["list", "capture_full", "import_csv", "update", "auto_align", "compare_stats", "delete", "export_csv"]
                    },
                    "slice_id": { "type": "string" },
                    "name": { "type": "string", "description": "实验记录名称（用于 capture_full / import_csv / update）" },
                    "note": { "type": "string", "description": "调参备注，如 'Kp=1.0, Ki=150, 死区补偿开'" },
                    "csv_path": { "type": "string", "description": "action='import_csv' 时要导入的外部 CSV 路径" },
                    "enabled": { "type": "boolean" },
                    "selected_key": { "type": "string" },
                    "scale_x": { "type": "number" },
                    "offset_x": { "type": "number" },
                    "scale_y": { "type": "number" },
                    "offset_y": { "type": "number" },
                    "color": { "type": "string" },
                    "line_style": { "type": "string", "enum": ["solid", "dash", "dot"] },
                    "align_channel": { "type": "string" },
                    "align_threshold": { "type": "number" },
                    "export_csv_path": { "type": "string" }
                },
                "required": ["action"]
            }
        }),
        json!({
            "name": "foc_post_analysis",
            "description": "【核心静态分析引擎】对应『分析』页 6 大功能页签：1. step_response_pk（多轮实验阶跃整定 tr/tp/超调σ%/ts/纹波/RMSE 自动算分与最优评选）；2. harmonic_thd（加窗 FFT、THD% 与电谐波阶次成因诊断）；3. filter_sandbox（在实测波形上运行 iir1/biquad_lpf/movavg/notch/zerophase 滤波或批量扫参，评估纹波衰减与群延迟并自动生成下位机 C 代码）；4. overlay_diff（双曲线自动对齐与 Δ 残差 RMSE）；5. xy_envelope（X-Y 矢量圆/轨迹包络与偏心统计）；6. full_stats（全通道统计总表）。支持实时缓冲 '__live_buffer__' 与任意归档切片，无界面离线亦可运行。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "mode": {
                        "type": "string",
                        "enum": ["step_response_pk", "harmonic_thd", "filter_sandbox", "overlay_diff", "xy_envelope", "full_stats"]
                    },
                    "slice_id": {
                        "type": "string",
                        "description": "目标实验 ID 或名称；传 '__live_buffer__' 或省略时默认使用最新焦点记录或当前示波缓冲。"
                    },
                    "slice_ids": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "用于 step_response_pk 或 overlay_diff 指定多条对比实验 ID 列表；省略则自动取所有已勾选实验。"
                    },
                    "channel": { "type": "string", "description": "主分析通道（如 'speed_est' / 'CH1' / 'Ia'）" },
                    "ref_channel": { "type": "string", "description": "step_response_pk 的目标给定通道（如 'speed_ref' / 'CH0'），或 overlay_diff 的第二对比通道" },
                    "settle_band": { "type": "number", "description": "阶跃稳态误差带比例，如 0.02 (±2%) 或 0.05 (±5%)" },
                    "window_type": { "type": "string", "enum": ["hann", "flattop", "rect"] },
                    "pole_pairs": { "type": "integer", "description": "电机极对数 Pn（默认 7）" },
                    "rpm_override": { "type": "number", "description": "可选：手动指定转速 RPM 以精确计算 fm 与 fe" },
                    "filter_type": {
                        "type": "string",
                        "enum": ["iir1", "biquad_lpf", "movavg", "notch", "zerophase"]
                    },
                    "filter_param": {
                        "type": "number",
                        "description": "滤波器截止频率 fc(Hz)、陷波频率 f0(Hz) 或 movavg 窗口点数 N（默认 25）"
                    },
                    "sweep_params": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "filter_sandbox 可选批量扫参列表（如 [10, 25, 50, 100]），一次调用对比多组截止频率的纹波衰减与延迟！"
                    },
                    "align_mode": { "type": "string", "enum": ["rising50", "threshold", "peak", "manual"] },
                    "align_threshold": { "type": "number" },
                    "normalize_100": { "type": "boolean" },
                    "x_channel": { "type": "string", "description": "xy_envelope 的 X 轴通道（如 'Vd' / 'CH8'）" },
                    "y_channel": { "type": "string", "description": "xy_envelope 的 Y 轴通道（如 'Vq' / 'CH9'）" },
                    "sync_to_gui": { "type": "boolean", "description": "若桌面窗口在线，是否同步将界面切到对应分析视图与参数（默认 true）" }
                },
                "required": ["mode"]
            }
        }),
        json!({
            "name": "foc_navigate_and_view",
            "description": "控制 FluxScope 桌面界面导航与视图状态：切换左侧主页面 (scope|conn|lab|config|terminal|about)、缩放或自适应示波器时间范围 [t_min, t_max]、配置时基跟随锚点，或切换『分析』页的 6 大子页签及参数。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "page": { "type": "string", "enum": ["scope", "conn", "lab", "config", "terminal", "about"] },
                    "bring_to_front": { "type": "boolean" },
                    "scope_view": {
                        "type": "object",
                        "properties": {
                            "fit_all": { "type": "boolean" },
                            "t_min": { "type": "number" },
                            "t_max": { "type": "number" },
                            "follow_enabled": { "type": "boolean" },
                            "follow_anchor_ratio": { "type": "number" },
                            "follow_span_sec": { "type": "number" }
                        }
                    },
                    "analysis_view": {
                        "type": "object",
                        "properties": {
                            "tab": { "type": "string", "enum": ["overlay", "step", "fft", "filter", "xy", "stats"] },
                            "focused_slice_id": { "type": "string" },
                            "overlay_scope_mode": { "type": "string", "enum": ["multi_run", "multi_ch"] },
                            "overlay_channel": { "type": "string" },
                            "multi_channels": { "type": "array", "items": { "type": "string" } },
                            "norm_mode": { "type": "string", "enum": ["raw", "norm100"] },
                            "align_mode": { "type": "string", "enum": ["rising50", "threshold", "peak", "manual"] },
                            "align_threshold": { "type": "number" },
                            "show_diff": { "type": "boolean" },
                            "step_fbk": { "type": "string" },
                            "step_ref": { "type": "string" },
                            "settle_band": { "type": "number" },
                            "fft_channel": { "type": "string" },
                            "fft_window": { "type": "string", "enum": ["hann", "flattop", "rect"] },
                            "fft_scale": { "type": "string", "enum": ["linear", "db"] },
                            "pole_pairs": { "type": "integer" },
                            "filter_channel": { "type": "string" },
                            "filter_type": { "type": "string", "enum": ["iir1", "biquad_lpf", "movavg", "notch", "zerophase"] },
                            "filter_cutoff": { "type": "number" },
                            "xy_x": { "type": "string" },
                            "xy_y": { "type": "string" },
                            "xy_equal": { "type": "boolean" }
                        }
                    }
                }
            }
        }),
        json!({
            "name": "foc_stream_and_serial_control",
            "description": "控制通信链路（串口 Serial / UDP / TCP，兼容 VOFA+ JustFloat / FireWater / RawData）、秒级打开 CSV、定速/自定义倍速流回放、暂停/继续、清空波形或清空终端日志(clear_console)、读取终端与书签(read_console)、设置坏点拦截，或向下位机发送指令（如 ID / ID? / C）。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": [
                            "list_ports", "connect_serial", "connect_udp", "connect_tcp", "disconnect",
                            "load_csv", "replay_csv", "restart_replay", "set_replay_speed",
                            "pause", "resume", "clear_buffer", "clear_console", "read_console",
                            "set_mode", "set_spike_filter", "send_command"
                        ]
                    },
                    "port": { "type": "string" },
                    "baudrate": { "type": "integer" },
                    "protocol": { "type": "string", "enum": ["justfloat", "firewater", "rawdata"] },
                    "data_bits": { "type": "integer" },
                    "stop_bits": { "type": "integer" },
                    "parity": { "type": "string", "enum": ["none", "even", "odd"] },
                    "flow_control": { "type": "string", "enum": ["none", "hardware", "software"] },
                    "dtr": { "type": "boolean" },
                    "rts": { "type": "boolean" },
                    "raw_hex_display": { "type": "boolean" },
                    "udp_bind_addr": { "type": "string" },
                    "tcp_remote_addr": { "type": "string" },
                    "nominal_sample_hz": { "type": "number" },
                    "csv_path": { "type": "string" },
                    "adopt_csv_headers": { "type": "boolean" },
                    "replay_speed": { "type": "number" },
                    "start_mode": { "type": "string", "enum": ["zero", "t1", "t2"] },
                    "operating_mode": { "type": "string", "enum": ["normal", "motor_id"] },
                    "spike_filter_enabled": { "type": "boolean" },
                    "spike_threshold": { "type": "number" },
                    "command": { "type": "string" }
                },
                "required": ["action"]
            }
        }),
        json!({
            "name": "foc_save_or_load_profile",
            "description": "保存、加载、列出通道与控件预设档案，或一键恢复默认 27 通道预设。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["list", "save", "load", "reset_default"] },
                    "profile_name": { "type": "string" }
                },
                "required": ["action"]
            }
        }),
        json!({
            "name": "foc_capture_screenshot",
            "description": "对 FluxScope 示波器多窗画布（自动含窗口名与全变量颜色图例）或『分析』页任意功能页签画布截图保存为高清 PNG 并返回文件路径，供多模态 AI 直接看图诊断。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "enum": [
                            "full_window", "scope", "overlay", "analysis_active",
                            "analysis_overlay", "analysis_step", "analysis_fft", "analysis_filter", "analysis_xy"
                        ],
                        "description": "scope/full_window=主示波合成图；analysis_step/fft/filter/xy/overlay=自动渲染对应分析页签并导出高清图。"
                    }
                }
            }
        }),
        json!({
            "name": "foc_launch_studio",
            "description": "启动或唤醒置顶 FluxScope 原生桌面窗口（可选传入 csv_path 启动即加载）。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "csv_path": { "type": "string" }
                }
            }
        }),
    ]
}

fn default_target_slice_id(eng: &TelemetryEngine, req: Option<&str>) -> String {
    if let Some(s) = req {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let st = eng.store.read().unwrap();
    if !st.timestamps.is_empty() {
        return "__live_buffer__".to_string();
    }
    if let Some(last) = st.slices.last() {
        return last.meta.slice_id.clone();
    }
    "__live_buffer__".to_string()
}

pub fn execute_mcp_tool(
    eng: &Arc<TelemetryEngine>,
    hooks: Option<&GuiHooks>,
    name: &str,
    args: &Value,
) -> Result<Value, String> {
    match name {
        "foc_get_status" => {
            let inspect_time = args.get("inspect_time").and_then(|v| v.as_f64());
            let tail_lines = args
                .get("console_tail")
                .and_then(|v| v.as_u64())
                .unwrap_or(40)
                .clamp(1, 500) as usize;

            let vp = eng.query_viewport(None, None, 320, None, None);
            let snap = eng.query_readout_at(inspect_time);
            let prof = eng.profile.read().unwrap().clone();
            let st = eng.store.read().unwrap();

            let mut named_vals = BTreeMap::new();
            for ch in &prof.channels {
                let ck = format!("CH{}", ch.index);
                if let Some(&v) = snap.channel_vals.get(&ck) {
                    let key = if !ch.alias.is_empty() {
                        format!("{} ({})", ch.alias, ck)
                    } else {
                        ck
                    };
                    named_vals.insert(key, (v as f64 * 10000.0).round() / 10000.0);
                }
            }
            for mc in &prof.math_channels {
                if let Some(&v) = snap.channel_vals.get(&mc.id) {
                    let key = if !mc.alias.is_empty() {
                        format!("{} ({})", mc.alias, mc.id)
                    } else {
                        mc.id.clone()
                    };
                    named_vals.insert(key, (v as f64 * 10000.0).round() / 10000.0);
                }
            }

            let recent_bms: Vec<_> = st
                .bookmarks
                .iter()
                .rev()
                .take(30)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let recent_con: Vec<_> = st
                .console_lines
                .iter()
                .rev()
                .take(tail_lines)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let slices_summary: Vec<Value> = st
                .slices
                .iter()
                .map(|s| {
                    json!({
                        "slice_id": s.meta.slice_id,
                        "name": s.meta.name,
                        "note": s.meta.note,
                        "enabled": s.meta.enabled,
                        "samples": s.meta.sample_count,
                        "hz": s.meta.sample_rate_hz,
                    })
                })
                .collect();

            Ok(json!({
                "app": "FluxScope",
                "gui_online": hooks.is_some(),
                "profile_name": prof.profile_name,
                "theme": prof.theme,
                "source": vp.source_label,
                "interface_type": vp.interface_type,
                "protocol": vp.protocol,
                "rx_kbps": vp.rx_kbps,
                "is_streaming": vp.is_streaming,
                "paused": vp.paused,
                "operating_mode": prof.operating_mode,
                "layout_locked": prof.layout_locked,
                "timebase_follow": {
                    "follow_enabled": prof.follow_enabled,
                    "follow_anchor_ratio": prof.follow_anchor_ratio,
                    "follow_span_sec": prof.follow_span_sec,
                    "follow_custom_sec": prof.follow_custom_sec,
                },
                "right_drawer": {
                    "open": prof.right_drawer_open,
                    "width_px": prof.right_drawer_width,
                    "data_source": prof.drawer_data_source,
                    "widget_count": prof.drawer_widgets.len(),
                },
                "spike_filter": {
                    "enabled": prof.spike_filter_enabled,
                    "threshold": prof.spike_threshold,
                    "rejected_frames": vp.spike_rejected,
                },
                "buffered_frames": vp.buffered_frames,
                "sample_rate_hz": vp.sample_rate_hz,
                "nominal_sample_hz": prof.nominal_sample_hz,
                "time_span_s": [vp.full_t_min, vp.full_t_max],
                "cursors": {
                    "enabled": vp.cursor_enabled,
                    "t1": vp.cursor_t1,
                    "t2": vp.cursor_t2,
                    "delta_s": (vp.cursor_t2 - vp.cursor_t1).abs(),
                },
                "readout_sample_time_s": snap.sample_time,
                "decoded_foc_state": snap.state_label,
                "fault_code": snap.fault_code,
                "active_faults": snap.active_faults,
                "isr_cycles": snap.isr_cycles,
                "isr_full_scale": snap.isr_full_scale,
                "channel_values": named_vals,
                "math_errors": vp.math_errors,
                "recent_bookmarks": recent_bms,
                "recent_console_lines": recent_con,
                "saved_slices": slices_summary,
            }))
        }

        "foc_get_channels_and_plots" => {
            let prof = eng.profile.read().unwrap().clone();
            let mut val = serde_json::to_value(prof).map_err(|e| e.to_string())?;
            if args
                .get("include_widget_catalog")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                if let Some(obj) = val.as_object_mut() {
                    obj.insert("widget_catalog_guide".to_string(), widget_catalog_reference());
                }
            }
            Ok(val)
        }

        "foc_configure_channels" => {
            let arr = args
                .get("channels")
                .and_then(|v| v.as_array())
                .ok_or("缺少 channels 数组")?;
            let mut prof = eng.profile.write().unwrap();
            let mut updated = Vec::new();

            if let Some(nhz) = args.get("nominal_sample_hz").and_then(|v| v.as_f64()) {
                if nhz > 0.0 && nhz.is_finite() {
                    prof.nominal_sample_hz = nhz;
                }
            }

            for item in arr {
                let idx = item
                    .get("index")
                    .and_then(|v| v.as_u64())
                    .ok_or("channel 缺少 index")? as usize;
                prof.ensure_channel_capacity(idx + 1);
                if let Some(ch) = prof.channels.iter_mut().find(|c| c.index == idx) {
                    if let Some(s) = item.get("name").and_then(|v| v.as_str()) {
                        ch.name = s.to_string();
                    }
                    if let Some(s) = item.get("alias").and_then(|v| v.as_str()) {
                        ch.alias = s.to_string();
                    }
                    if let Some(s) = item.get("unit").and_then(|v| v.as_str()) {
                        ch.unit = s.to_string();
                    }
                    if let Some(s) = item.get("color").and_then(|v| v.as_str()) {
                        ch.color = s.to_string();
                    }
                    if let Some(w) = item.get("width").and_then(|v| v.as_f64()) {
                        ch.width = w as f32;
                    }
                    if let Some(b) = item.get("visible").and_then(|v| v.as_bool()) {
                        ch.visible = b;
                    }
                    if let Some(s) = item.get("plot_id").and_then(|v| v.as_str()) {
                        ch.plot_id = s.to_string();
                    }
                    if let Some(b) = item.get("show_card").and_then(|v| v.as_bool()) {
                        ch.show_card = b;
                    }
                    if let Some(d) = item.get("decimals").and_then(|v| v.as_u64()) {
                        ch.decimals = d as usize;
                    }
                    if let Some(g) = item.get("display_gain").and_then(|v| v.as_f64()) {
                        if g.is_finite() {
                            ch.display_gain = g as f32;
                        }
                    }
                    if let Some(s) = item.get("decoder_type").and_then(|v| v.as_str()) {
                        ch.decoder_type = s.to_string();
                    }
                    if let Some(em) = item.get("enum_map").and_then(|v| v.as_object()) {
                        let mut m = BTreeMap::new();
                        for (k, ev) in em {
                            if let Some(lbl) = ev.as_str() {
                                m.insert(
                                    k.clone(),
                                    EnumEntry {
                                        label: lbl.to_string(),
                                        color: "#38BDF8".to_string(),
                                    },
                                );
                            } else if let Ok(ent) = serde_json::from_value::<EnumEntry>(ev.clone())
                            {
                                m.insert(k.clone(), ent);
                            }
                        }
                        ch.enum_map = m;
                    }
                    if let Some(bm) = item.get("bitmask_map").and_then(|v| v.as_object()) {
                        let mut m = BTreeMap::new();
                        for (k, bv) in bm {
                            if let Some(s) = bv.as_str() {
                                m.insert(k.clone(), s.to_string());
                            }
                        }
                        ch.bitmask_map = m;
                    }
                    let sync_info = (
                        ch.decoder_type.clone(),
                        ch.enum_map.clone(),
                        ch.bitmask_map.clone(),
                    );
                    if sync_info.0 == "enum" {
                        prof.status_decode.state_channel = Some(idx);
                        if !sync_info.1.is_empty() {
                            prof.status_decode.state_enum_map = sync_info.1;
                        }
                    } else if sync_info.0 == "bitmask" {
                        prof.status_decode.fault_channel = Some(idx);
                        if !sync_info.2.is_empty() {
                            prof.status_decode.fault_map = sync_info.2;
                        }
                    }
                    updated.push(idx);
                }
            }

            if let Some(trunc) = args.get("truncate_to_count").and_then(|v| v.as_u64()) {
                prof.channels.retain(|c| c.index < trunc as usize);
            }
            prof.channels.sort_by_key(|c| c.index);
            prof.sync_channels_with_status_decode();
            prof.save(None)?;
            let total = prof.channels.len();
            drop(prof);
            eng.bump_math_seq();
            if let Some(h) = hooks {
                (h.notify_ui)("profile");
            }
            Ok(json!({ "status": "ok", "updated_indices": updated, "total_channels": total }))
        }

        "foc_upsert_math_channel" => {
            let expr = args
                .get("expression")
                .and_then(|v| v.as_str())
                .ok_or("缺少 expression")?
                .trim()
                .to_string();
            compile_expression(&expr)?;
            let mname = args
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or("缺少 name")?
                .to_string();

            let mut prof = eng.profile.write().unwrap();
            let req_id = args.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());
            let req_alias = args
                .get("alias")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let existing_pos = prof.math_channels.iter().position(|m| {
                req_id.as_deref() == Some(m.id.as_str())
                    || (req_alias.is_some() && req_alias.as_deref() == Some(m.alias.as_str()))
            });

            let saved_mc = if let Some(pos) = existing_pos {
                let mc = &mut prof.math_channels[pos];
                mc.name = mname;
                mc.expression = expr;
                if let Some(a) = req_alias {
                    mc.alias = a;
                }
                if let Some(u) = args.get("unit").and_then(|v| v.as_str()) {
                    mc.unit = u.to_string();
                }
                if let Some(c) = args.get("color").and_then(|v| v.as_str()) {
                    mc.color = c.to_string();
                }
                if let Some(w) = args.get("width").and_then(|v| v.as_f64()) {
                    mc.width = w as f32;
                }
                if let Some(vis) = args.get("visible").and_then(|v| v.as_bool()) {
                    mc.visible = vis;
                }
                if let Some(pid) = args.get("plot_id").and_then(|v| v.as_str()) {
                    mc.plot_id = pid.to_string();
                }
                if let Some(sc) = args.get("show_card").and_then(|v| v.as_bool()) {
                    mc.show_card = sc;
                }
                if let Some(dec) = args.get("decimals").and_then(|v| v.as_u64()) {
                    mc.decimals = dec as usize;
                }
                mc.clone()
            } else {
                let mid = req_id.unwrap_or_else(|| {
                    let mut k = 0;
                    while prof.math_channels.iter().any(|m| m.id == format!("M{}", k)) {
                        k += 1;
                    }
                    format!("M{}", k)
                });
                let default_pid = prof
                    .plots
                    .first()
                    .map(|p| p.id.clone())
                    .unwrap_or_else(|| "plot_1".to_string());
                let mc = MathChannelConfig {
                    id: mid.clone(),
                    name: mname,
                    alias: req_alias.unwrap_or_else(|| mid.to_lowercase()),
                    expression: expr,
                    unit: args
                        .get("unit")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    color: args
                        .get("color")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| {
                            TRACE_PALETTE[(prof.math_channels.len() + 4) % TRACE_PALETTE.len()]
                                .to_string()
                        }),
                    width: args.get("width").and_then(|v| v.as_f64()).unwrap_or(1.6) as f32,
                    visible: args.get("visible").and_then(|v| v.as_bool()).unwrap_or(true),
                    plot_id: args
                        .get("plot_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or(default_pid),
                    show_card: args
                        .get("show_card")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true),
                    decimals: args.get("decimals").and_then(|v| v.as_u64()).unwrap_or(3) as usize,
                };
                prof.math_channels.push(mc.clone());
                mc
            };

            prof.save(None)?;
            drop(prof);
            eng.bump_math_seq();
            if let Some(h) = hooks {
                (h.notify_ui)("profile");
            }
            Ok(json!({ "status": "ok", "math_channel": saved_mc }))
        }

        "foc_remove_math_channel" => {
            let key = args
                .get("id_or_alias")
                .and_then(|v| v.as_str())
                .ok_or("缺少 id_or_alias")?;
            let mut prof = eng.profile.write().unwrap();
            let before = prof.math_channels.len();
            prof.math_channels
                .retain(|m| m.id != key && m.alias != key);
            let removed = before - prof.math_channels.len();
            prof.save(None)?;
            drop(prof);
            eng.bump_math_seq();
            if let Some(h) = hooks {
                (h.notify_ui)("profile");
            }
            Ok(json!({ "status": "ok", "removed_count": removed }))
        }

        "foc_manage_plots_and_layout" => {
            let mut prof = eng.profile.write().unwrap();
            if let Some(arr) = args.get("add_or_update_plots").and_then(|v| v.as_array()) {
                for p in arr {
                    let pid = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let title = p.get("title").and_then(|v| v.as_str()).unwrap_or("波形窗");
                    if pid.is_empty() {
                        continue;
                    }
                    if let Some(ex) = prof.plots.iter_mut().find(|x| x.id == pid) {
                        ex.title = title.to_string();
                        if let Some(yl) = p.get("y_label").and_then(|v| v.as_str()) {
                            ex.y_label = yl.to_string();
                        }
                        if let Some(hw) = p.get("height_weight").and_then(|v| v.as_f64()) {
                            ex.height_weight = (hw as f32).clamp(0.2, 10.0);
                        }
                    } else {
                        prof.plots.push(PlotPanelConfig {
                            id: pid.to_string(),
                            title: title.to_string(),
                            y_label: p
                                .get("y_label")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            height_weight: p
                                .get("height_weight")
                                .and_then(|v| v.as_f64())
                                .map(|w| (w as f32).clamp(0.2, 10.0))
                                .unwrap_or(1.0),
                        });
                    }
                }
            }
            if let Some(rem) = args.get("remove_plot_ids").and_then(|v| v.as_array()) {
                let rem_set: Vec<String> = rem
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                prof.plots.retain(|p| !rem_set.contains(&p.id));
                if prof.plots.is_empty() {
                    prof.plots.push(PlotPanelConfig {
                        id: "plot_1".to_string(),
                        title: "主波形窗".to_string(),
                        y_label: String::new(),
                        height_weight: 1.0,
                    });
                }
                for ch in &mut prof.channels {
                    if rem_set.contains(&ch.plot_id) {
                        ch.plot_id = "none".to_string();
                    }
                }
                for mc in &mut prof.math_channels {
                    if rem_set.contains(&mc.plot_id) {
                        mc.plot_id = "none".to_string();
                    }
                }
            }
            if let Some(xy) = args.get("xy_config").and_then(|v| v.as_object()) {
                if let Some(xs) = xy.get("x_source").and_then(|v| v.as_str()) {
                    prof.xy_config.x_source = xs.to_string();
                }
                if let Some(ys) = xy.get("y_source").and_then(|v| v.as_str()) {
                    prof.xy_config.y_source = ys.to_string();
                }
                if let Some(tp) = xy.get("tail_points").and_then(|v| v.as_u64()) {
                    prof.xy_config.tail_points = tp as usize;
                }
                if let Some(al) = xy.get("aspect_locked").and_then(|v| v.as_bool()) {
                    prof.xy_config.aspect_locked = al;
                }
                if let Some(en) = xy.get("enabled").and_then(|v| v.as_bool()) {
                    prof.xy_config.enabled = en;
                }
            }
            if let Some(sd) = args.get("status_decode").and_then(|v| v.as_object()) {
                if let Some(en) = sd.get("enabled").and_then(|v| v.as_bool()) {
                    prof.status_decode.enabled = en;
                }
                if let Some(en) = sd.get("state_enabled").and_then(|v| v.as_bool()) {
                    prof.status_decode.state_enabled = en;
                }
                if sd.contains_key("state_channel") {
                    prof.status_decode.state_channel =
                        sd.get("state_channel").and_then(|v| v.as_u64()).map(|x| x as usize);
                }
                if let Some(en) = sd.get("fault_enabled").and_then(|v| v.as_bool()) {
                    prof.status_decode.fault_enabled = en;
                }
                if sd.contains_key("fault_channel") {
                    prof.status_decode.fault_channel =
                        sd.get("fault_channel").and_then(|v| v.as_u64()).map(|x| x as usize);
                }
                if let Some(fm) = sd.get("fault_mode").and_then(|v| v.as_str()) {
                    prof.status_decode.fault_mode = fm.to_string();
                }
                if let Some(en) = sd.get("isr_enabled").and_then(|v| v.as_bool()) {
                    prof.status_decode.isr_enabled = en;
                }
                if sd.contains_key("isr_channel") {
                    prof.status_decode.isr_channel =
                        sd.get("isr_channel").and_then(|v| v.as_u64()).map(|x| x as usize);
                }
                if let Some(lbl) = sd.get("isr_label").and_then(|v| v.as_str()) {
                    prof.status_decode.isr_label = lbl.to_string();
                }
                if let Some(fs) = sd.get("isr_full_scale").and_then(|v| v.as_f64()) {
                    prof.status_decode.isr_full_scale = (fs as f32).max(1.0);
                }
                if let Some(u) = sd.get("isr_unit").and_then(|v| v.as_str()) {
                    prof.status_decode.isr_unit = u.to_string();
                }
                prof.sync_channels_with_status_decode();
            }
            if let Some(op) = args.get("right_drawer_open").and_then(|v| v.as_bool()) {
                prof.right_drawer_open = op;
            }
            if let Some(dw) = args.get("right_drawer_width").and_then(|v| v.as_u64()) {
                prof.right_drawer_width = (dw as u32).clamp(240, 720);
            }
            if let Some(ds) = args.get("drawer_data_source").and_then(|v| v.as_str()) {
                if matches!(ds, "latest" | "hover" | "t1" | "t2") {
                    prof.drawer_data_source = ds.to_string();
                }
            }
            if args
                .get("reset_default_widgets")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                prof.ensure_default_widgets();
            }
            if let Some(rep) = args.get("replace_widgets").and_then(|v| v.as_array()) {
                prof.drawer_widgets = rep.clone();
                prof.widgets_initialized = true;
            }
            if let Some(ups) = args.get("upsert_widgets").and_then(|v| v.as_array()) {
                prof.widgets_initialized = true;
                for item in ups {
                    let wid = item
                        .get("id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("w_mcp_{}", prof.drawer_widgets.len() + 1));
                    if let Some(ex) = prof
                        .drawer_widgets
                        .iter_mut()
                        .find(|x| x.get("id").and_then(|v| v.as_str()) == Some(wid.as_str()))
                    {
                        if let (Some(ex_obj), Some(in_obj)) = (ex.as_object_mut(), item.as_object())
                        {
                            for (k, v) in in_obj {
                                ex_obj.insert(k.clone(), v.clone());
                            }
                        }
                    } else {
                        let mut cloned = item.clone();
                        if let Some(obj) = cloned.as_object_mut() {
                            obj.insert("id".to_string(), json!(wid));
                        }
                        prof.drawer_widgets.push(cloned);
                    }
                }
            }
            if let Some(rem) = args.get("remove_widget_ids").and_then(|v| v.as_array()) {
                let ids: Vec<String> = rem
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                prof.drawer_widgets.retain(|w| {
                    w.get("id")
                        .and_then(|v| v.as_str())
                        .map(|id| !ids.contains(&id.to_string()))
                        .unwrap_or(true)
                });
            }
            if let Some(order_arr) = args.get("reorder_widget_ids").and_then(|v| v.as_array()) {
                let order: Vec<String> = order_arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                let mut reordered = Vec::with_capacity(prof.drawer_widgets.len());
                for oid in &order {
                    if let Some(pos) = prof
                        .drawer_widgets
                        .iter()
                        .position(|w| w.get("id").and_then(|v| v.as_str()) == Some(oid.as_str()))
                    {
                        reordered.push(prof.drawer_widgets.remove(pos));
                    }
                }
                reordered.append(&mut prof.drawer_widgets);
                prof.drawer_widgets = reordered;
            }
            if let Some(fe) = args.get("follow_enabled").and_then(|v| v.as_bool()) {
                prof.follow_enabled = fe;
            }
            if let Some(fa) = args.get("follow_anchor_ratio").and_then(|v| v.as_f64()) {
                if fa.is_finite() {
                    prof.follow_anchor_ratio = fa.clamp(0.10, 1.00);
                }
            }
            if let Some(fs) = args.get("follow_span_sec").and_then(|v| v.as_f64()) {
                if fs.is_finite() && fs > 0.0 {
                    prof.follow_span_sec = fs.clamp(0.0005, 36000.0);
                }
            }
            if let Some(fc) = args.get("follow_custom_sec").and_then(|v| v.as_f64()) {
                if fc.is_finite() && fc > 0.0 {
                    prof.follow_custom_sec = fc.clamp(0.01, 36000.0);
                }
            }
            if let Some(lk) = args.get("layout_locked").and_then(|v| v.as_bool()) {
                prof.layout_locked = lk;
            }
            if let Some(th) = args.get("theme").and_then(|v| v.as_str()) {
                if th == "dark" || th == "light" {
                    prof.theme = th.to_string();
                }
            }
            prof.sync_channels_with_status_decode();
            prof.save(None)?;
            let out = json!({
                "status": "ok",
                "plots": prof.plots,
                "right_drawer_open": prof.right_drawer_open,
                "drawer_data_source": prof.drawer_data_source,
                "widget_count": prof.drawer_widgets.len(),
                "follow_enabled": prof.follow_enabled,
                "follow_anchor_ratio": prof.follow_anchor_ratio,
                "follow_span_sec": prof.follow_span_sec,
                "layout_locked": prof.layout_locked,
                "theme": prof.theme,
            });
            drop(prof);
            if let Some(h) = hooks {
                (h.notify_ui)("profile");
            }
            Ok(out)
        }

        "foc_query_waveform" => {
            if let Some(sid) = args
                .get("slice_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty() && *s != "__live_buffer__")
            {
                let max_pts = args
                    .get("max_points")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(512)
                    .clamp(32, 8192) as usize;
                let bundle = load_bundle_typed(eng, sid, max_pts)?;
                let mut out = compute_full_stats(&bundle);
                if args
                    .get("include_samples")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true)
                {
                    let req_ch: Option<Vec<String>> = args
                        .get("channels")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                                .collect()
                        });
                    let mut series = BTreeMap::new();
                    for ch in &bundle.channels {
                        if let Some(ref filter_list) = req_ch {
                            if !filter_list
                                .iter()
                                .any(|q| q == &ch.key || ch.name.contains(q))
                            {
                                continue;
                            }
                        }
                        series.insert(
                            ch.key.clone(),
                            json!({ "name": ch.name, "unit": ch.unit, "y": ch.data }),
                        );
                    }
                    out["t"] = json!(bundle.t);
                    out["series"] = json!(series);
                }
                return Ok(out);
            }

            let win_mode = args
                .get("window")
                .and_then(|v| v.as_str())
                .unwrap_or("cursors");
            let (t1, t2) = {
                let st = eng.store.read().unwrap();
                if st.timestamps.is_empty() {
                    return Err("当前没有加载任何波形数据".to_string());
                }
                let f0 = st.timestamps[0];
                let f1 = *st.timestamps.last().unwrap();
                match win_mode {
                    "cursors" => (st.cursor_t1.min(st.cursor_t2), st.cursor_t1.max(st.cursor_t2)),
                    "latest" => {
                        let dur = args.get("latest_sec").and_then(|v| v.as_f64()).unwrap_or(2.0);
                        ((f1 - dur).max(f0), f1)
                    }
                    "custom" => (
                        args.get("t1").and_then(|v| v.as_f64()).unwrap_or(f0),
                        args.get("t2").and_then(|v| v.as_f64()).unwrap_or(f1),
                    ),
                    _ => (f0, f1),
                }
            };

            let req_ch: Option<Vec<String>> = args
                .get("channels")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect()
                });

            let mut res = eng.compute_window_stats(Some(t1), Some(t2), req_ch.as_deref())?;

            if args
                .get("include_samples")
                .and_then(|v| v.as_bool())
                .unwrap_or(true)
            {
                let max_pts = args
                    .get("max_points")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(200)
                    .clamp(16, 4000) as usize;
                let vp = eng.query_viewport(Some(t1), Some(t2), max_pts / 2, None, None);
                let mut series = BTreeMap::new();
                for tr in vp.traces {
                    series.insert(
                        tr.key,
                        json!({ "label": tr.label, "display_gain": tr.gain, "t": tr.t, "y": tr.y }),
                    );
                }
                if let Some(obj) = res.as_object_mut() {
                    obj.insert("decimated_traces".to_string(), json!(series));
                }
            }
            Ok(res)
        }

        "foc_compute_fft" => {
            let ch = args
                .get("channel")
                .and_then(|v| v.as_str())
                .ok_or("缺少 channel")?;
            let win_type = args
                .get("window_type")
                .and_then(|v| v.as_str())
                .unwrap_or("hann");
            let poles = args
                .get("pole_pairs")
                .and_then(|v| v.as_u64())
                .unwrap_or(7) as u32;

            if let Some(sid) = args
                .get("slice_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
            {
                let bundle = load_bundle_typed(eng, sid, 16384)?;
                let ser = find_series(eng, &bundle, ch)
                    .ok_or_else(|| format!("未找到通道 '{}'", ch))?;
                let rpm_est = bundle
                    .channels
                    .iter()
                    .find(|c| {
                        c.key == "speed_est"
                            || c.unit.eq_ignore_ascii_case("rpm")
                            || c.name.contains("转速")
                    })
                    .and_then(|sc| {
                        if sc.data.is_empty() {
                            None
                        } else {
                            let s0 = sc.data.len() / 2;
                            let avg: f64 = sc.data[s0..]
                                .iter()
                                .map(|&v| (v as f64).abs())
                                .sum::<f64>()
                                / (sc.data.len() - s0).max(1) as f64;
                            Some(avg)
                        }
                    });
                return Ok(compute_harmonic_thd(
                    &ser.data,
                    bundle.hz,
                    win_type,
                    poles,
                    rpm_est,
                    true,
                ));
            }

            // Live buffer [t1, t2] or cursor window
            let t1 = args.get("t1").and_then(|v| v.as_f64());
            let t2 = args.get("t2").and_then(|v| v.as_f64());
            if win_type != "hann" || args.get("pole_pairs").is_some() {
                let (lo, hi) = {
                    let st = eng.store.read().unwrap();
                    if st.timestamps.is_empty() {
                        return Err("当前缓冲区为空".to_string());
                    }
                    let a = t1.unwrap_or(st.cursor_t1);
                    let b = t2.unwrap_or(st.cursor_t2);
                    (a.min(b), a.max(b))
                };
                let temp_sl = eng.extract_slice(lo, hi, "_fft_tmp", false)?;
                let col = temp_sl
                    .data
                    .get(ch)
                    .or_else(|| {
                        let prof = eng.profile.read().unwrap();
                        prof.channels
                            .iter()
                            .find(|c| format!("CH{}", c.index) == ch || c.name == ch)
                            .and_then(|c| temp_sl.data.get(&c.alias))
                    })
                    .ok_or_else(|| format!("未找到通道 '{}'", ch))?;
                let dur = temp_sl.t_rel.last().copied().unwrap_or(0.0)
                    - temp_sl.t_rel.first().copied().unwrap_or(0.0);
                let hz = if dur > 1e-6 && temp_sl.t_rel.len() >= 2 {
                    (temp_sl.t_rel.len() - 1) as f64 / dur
                } else {
                    temp_sl.meta.sample_rate_hz.max(1.0)
                };
                let rpm_est = temp_sl.data.get("speed_est").and_then(|sc| {
                    if sc.is_empty() {
                        None
                    } else {
                        let avg: f64 =
                            sc.iter().map(|&v| (v as f64).abs()).sum::<f64>() / sc.len() as f64;
                        Some(avg)
                    }
                });
                return Ok(compute_harmonic_thd(col, hz, win_type, poles, rpm_est, true));
            }

            let mut fft_res = eng.compute_fft(ch, t1, t2)?;
            if fft_res.freqs.len() > 240 {
                let (df, da) = decimate_minmax(&fft_res.freqs, &fft_res.amps, 120);
                fft_res.freqs = df;
                fft_res.amps = da;
            }
            serde_json::to_value(fft_res).map_err(|e| e.to_string())
        }

        "foc_set_cursors_and_slice" => {
            let enabled = args.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
            let t1 = args.get("t1").and_then(|v| v.as_f64()).ok_or("缺少 t1")?;
            let t2 = args.get("t2").and_then(|v| v.as_f64()).ok_or("缺少 t2")?;
            eng.set_cursors(Some(enabled), Some(t1), Some(t2));
            let mut out = json!({
                "status": "ok",
                "enabled": enabled,
                "t1": t1.min(t2),
                "t2": t1.max(t2),
                "zoom_to_cursors": args.get("zoom_to_cursors").and_then(|v| v.as_bool()).unwrap_or(false),
            });

            let mut created_sid: Option<String> = None;
            if let Some(sname) = args.get("create_slice_name").and_then(|v| v.as_str()) {
                if !sname.trim().is_empty() {
                    let sl = eng.extract_slice(t1, t2, sname, true)?;
                    let sid = sl.meta.slice_id.clone();
                    if let Some(note) = args.get("create_slice_note").and_then(|v| v.as_str()) {
                        if !note.is_empty() {
                            let mut st = eng.store.write().unwrap();
                            if let Some(saved) =
                                st.slices.iter_mut().find(|x| x.meta.slice_id == sid)
                            {
                                saved.meta.note = note.to_string();
                                let _ = saved.save_to_disk();
                            }
                        }
                    }
                    out["created_slice_id"] = json!(sid);
                    out["created_slice_name"] = json!(sl.meta.name);
                    created_sid = Some(sid);
                }
            }
            if let Some(exp_p) = args.get("export_csv_path").and_then(|v| v.as_str()) {
                if !exp_p.trim().is_empty() {
                    let sl = eng.extract_slice(t1, t2, "_export", false)?;
                    let saved = sl.export_csv(Path::new(exp_p))?;
                    out["exported_csv"] = json!(saved.to_string_lossy());
                }
            }
            if let Some(h) = hooks {
                if created_sid.is_some() {
                    (h.notify_ui)("slices");
                }
                if args
                    .get("open_in_analysis")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    (h.send_ui_cmd)(json!({
                        "page": "lab",
                        "analysis_view": { "focused_slice_id": created_sid }
                    }));
                } else if args
                    .get("zoom_to_cursors")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    (h.notify_ui)("zoom_cursors");
                } else {
                    (h.notify_ui)("cursors");
                }
            }
            Ok(out)
        }

        "foc_manage_slices" => {
            let act = args
                .get("action")
                .and_then(|v| v.as_str())
                .ok_or("缺少 action")?;
            match act {
                "list" => Ok(eng.get_analysis_overview()),
                "capture_full" => {
                    let sname = args
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("全缓冲快照");
                    let mut meta = eng.capture_full_buffer_slice(sname)?;
                    let note_opt = args.get("note").and_then(|v| v.as_str());
                    let col_opt = args.get("color").and_then(|v| v.as_str());
                    if note_opt.is_some() || col_opt.is_some() {
                        let mut st = eng.store.write().unwrap();
                        if let Some(sl) = st
                            .slices
                            .iter_mut()
                            .find(|s| s.meta.slice_id == meta.slice_id)
                        {
                            if let Some(nt) = note_opt {
                                sl.meta.note = nt.to_string();
                            }
                            if let Some(c) = col_opt {
                                sl.meta.color = c.to_string();
                            }
                            let _ = sl.save_to_disk();
                            meta = sl.meta.clone();
                        }
                    }
                    if let Some(h) = hooks {
                        (h.notify_ui)("slices");
                    }
                    Ok(json!({ "status": "ok", "slice": meta }))
                }
                "import_csv" => {
                    let p = args
                        .get("csv_path")
                        .and_then(|v| v.as_str())
                        .ok_or("缺少 csv_path")?;
                    let mut meta = eng.import_external_csv_as_slice(Path::new(p))?;
                    let name_opt = args.get("name").and_then(|v| v.as_str());
                    let note_opt = args.get("note").and_then(|v| v.as_str());
                    let col_opt = args.get("color").and_then(|v| v.as_str());
                    if name_opt.is_some() || note_opt.is_some() || col_opt.is_some() {
                        let mut st = eng.store.write().unwrap();
                        if let Some(sl) = st
                            .slices
                            .iter_mut()
                            .find(|s| s.meta.slice_id == meta.slice_id)
                        {
                            if let Some(nm) = name_opt.filter(|s| !s.trim().is_empty()) {
                                sl.meta.name = nm.trim().to_string();
                            }
                            if let Some(nt) = note_opt {
                                sl.meta.note = nt.to_string();
                            }
                            if let Some(c) = col_opt {
                                sl.meta.color = c.to_string();
                            }
                            let _ = sl.save_to_disk();
                            meta = sl.meta.clone();
                        }
                    }
                    if let Some(h) = hooks {
                        (h.notify_ui)("slices");
                    }
                    Ok(json!({ "status": "ok", "slice": meta }))
                }
                "update" => {
                    let sid = args
                        .get("slice_id")
                        .and_then(|v| v.as_str())
                        .ok_or("缺少 slice_id")?;
                    let prof = eng.profile.read().unwrap().clone();
                    let mut st = eng.store.write().unwrap();
                    let sl = st
                        .slices
                        .iter_mut()
                        .find(|s| s.meta.slice_id == sid || s.meta.name == sid)
                        .ok_or_else(|| format!("未找到切片 '{}'", sid))?;

                    if let Some(nm) = args.get("name").and_then(|v| v.as_str()) {
                        if !nm.trim().is_empty() {
                            sl.meta.name = nm.trim().to_string();
                        }
                    }
                    if let Some(nt) = args.get("note").and_then(|v| v.as_str()) {
                        sl.meta.note = nt.to_string();
                    }
                    if let Some(b) = args.get("enabled").and_then(|v| v.as_bool()) {
                        sl.meta.enabled = b;
                    }
                    if let Some(sk) = args.get("selected_key").and_then(|v| v.as_str()) {
                        let mut resolved = sk.to_string();
                        if !sl.data.contains_key(&resolved) {
                            for ch in &prof.channels {
                                if (sk == format!("CH{}", ch.index) || sk == ch.name)
                                    && sl.data.contains_key(&ch.alias)
                                {
                                    resolved = ch.alias.clone();
                                    break;
                                }
                            }
                        }
                        sl.meta.selected_key = resolved;
                    }
                    if let Some(v) = args.get("scale_x").and_then(|v| v.as_f64()) {
                        sl.meta.scale_x = v;
                    }
                    if let Some(v) = args.get("offset_x").and_then(|v| v.as_f64()) {
                        sl.meta.offset_x = v;
                    }
                    if let Some(v) = args.get("scale_y").and_then(|v| v.as_f64()) {
                        sl.meta.scale_y = v as f32;
                    }
                    if let Some(v) = args.get("offset_y").and_then(|v| v.as_f64()) {
                        sl.meta.offset_y = v as f32;
                    }
                    if let Some(c) = args.get("color").and_then(|v| v.as_str()) {
                        sl.meta.color = c.to_string();
                    }
                    if let Some(ls) = args.get("line_style").and_then(|v| v.as_str()) {
                        sl.meta.line_style = ls.to_string();
                    }
                    sl.save_to_disk()?;
                    let out_meta = sl.meta.clone();
                    drop(st);
                    if let Some(h) = hooks {
                        (h.notify_ui)("slices");
                    }
                    Ok(json!({ "status": "ok", "slice": out_meta }))
                }
                "auto_align" => {
                    let ch = args.get("align_channel").and_then(|v| v.as_str());
                    let th = args
                        .get("align_threshold")
                        .and_then(|v| v.as_f64())
                        .map(|x| x as f32);
                    let aligned = eng.auto_align_slices(ch, th);
                    if let Some(h) = hooks {
                        (h.notify_ui)("slices");
                    }
                    Ok(json!({ "status": "ok", "aligned_offsets": aligned }))
                }
                "compare_stats" => {
                    let ch_override = args.get("align_channel").and_then(|v| v.as_str());
                    let st = eng.store.read().unwrap();
                    let mut report = Vec::new();
                    for sl in &st.slices {
                        if !sl.meta.enabled {
                            continue;
                        }
                        let k = ch_override.unwrap_or(&sl.meta.selected_key);
                        if let Some(col) = sl.data.get(k) {
                            if col.is_empty() {
                                continue;
                            }
                            let n = col.len() as f64;
                            let mut sum = 0.0f64;
                            let mut sq = 0.0f64;
                            let mut min_v = f64::INFINITY;
                            let mut max_v = f64::NEG_INFINITY;
                            for &v in col {
                                let sv = (v * sl.meta.scale_y + sl.meta.offset_y) as f64;
                                sum += sv;
                                sq += sv * sv;
                                min_v = min_v.min(sv);
                                max_v = max_v.max(sv);
                            }
                            report.push(json!({
                                "slice_id": sl.meta.slice_id,
                                "name": sl.meta.name,
                                "note": sl.meta.note,
                                "channel": k,
                                "mean": (sum / n * 10000.0).round() / 10000.0,
                                "rms": ((sq / n).sqrt() * 10000.0).round() / 10000.0,
                                "min": (min_v * 10000.0).round() / 10000.0,
                                "max": (max_v * 10000.0).round() / 10000.0,
                                "p2p": ((max_v - min_v) * 10000.0).round() / 10000.0,
                            }));
                        }
                    }
                    Ok(json!({ "comparison": report }))
                }
                "delete" => {
                    let sid = args
                        .get("slice_id")
                        .and_then(|v| v.as_str())
                        .ok_or("缺少 slice_id")?;
                    let mut st = eng.store.write().unwrap();
                    let mut removed_ids = Vec::new();
                    st.slices.retain(|s| {
                        if s.meta.slice_id == sid || s.meta.name == sid {
                            removed_ids.push(s.meta.slice_id.clone());
                            false
                        } else {
                            true
                        }
                    });
                    for id in &removed_ids {
                        let _ = fs::remove_file(slices_dir().join(format!("{}.fslice", id)));
                    }
                    drop(st);
                    if let Some(h) = hooks {
                        (h.notify_ui)("slices");
                    }
                    Ok(json!({ "status": "ok", "deleted": removed_ids.len() }))
                }
                "export_csv" => {
                    let sid = args
                        .get("slice_id")
                        .and_then(|v| v.as_str())
                        .ok_or("缺少 slice_id")?;
                    let p = args
                        .get("export_csv_path")
                        .and_then(|v| v.as_str())
                        .ok_or("缺少 export_csv_path")?;
                    let st = eng.store.read().unwrap();
                    let sl = st
                        .slices
                        .iter()
                        .find(|s| s.meta.slice_id == sid || s.meta.name == sid)
                        .ok_or_else(|| format!("未找到切片 '{}'", sid))?;
                    let saved = sl.export_csv(Path::new(p))?;
                    Ok(json!({ "status": "ok", "exported_csv": saved.to_string_lossy() }))
                }
                _ => Err(format!("未知的切片操作 '{}'", act)),
            }
        }

        "foc_post_analysis" => {
            let mode = args
                .get("mode")
                .and_then(|v| v.as_str())
                .ok_or("缺少 mode")?;
            let sync_gui = args
                .get("sync_to_gui")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            match mode {
                "step_response_pk" => {
                    let fbk_q = args
                        .get("channel")
                        .and_then(|v| v.as_str())
                        .unwrap_or("speed_est");
                    let ref_q = args
                        .get("ref_channel")
                        .and_then(|v| v.as_str())
                        .unwrap_or("speed_ref");
                    let band = args
                        .get("settle_band")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.02);

                    let ids: Vec<String> = if let Some(arr) =
                        args.get("slice_ids").and_then(|v| v.as_array())
                    {
                        arr.iter()
                            .filter_map(|x| x.as_str().map(|s| s.to_string()))
                            .collect()
                    } else if let Some(single) = args.get("slice_id").and_then(|v| v.as_str()) {
                        vec![single.to_string()]
                    } else {
                        let st = eng.store.read().unwrap();
                        let mut v: Vec<String> = st
                            .slices
                            .iter()
                            .filter(|s| s.meta.enabled)
                            .map(|s| s.meta.slice_id.clone())
                            .collect();
                        if v.is_empty() && !st.timestamps.is_empty() {
                            v.push("__live_buffer__".to_string());
                        }
                        v
                    };

                    if ids.is_empty() {
                        return Err("暂无可分析的实验数据；请先加载 CSV / 采集波形或传入 slice_id".to_string());
                    }

                    let mut rows = Vec::new();
                    let mut resolved_fbk_key = String::new();
                    let mut resolved_ref_key = String::new();
                    for id in &ids {
                        if let Ok(b) = load_bundle_typed(eng, id, 16384) {
                            let fbk_s = find_series(eng, &b, fbk_q);
                            let ref_s = find_series(eng, &b, ref_q);
                            if let Some(fs) = fbk_s {
                                if resolved_fbk_key.is_empty() {
                                    resolved_fbk_key = fs.key.clone();
                                }
                                let ref_slice = ref_s.and_then(|rs| {
                                    if resolved_ref_key.is_empty() {
                                        resolved_ref_key = rs.key.clone();
                                    }
                                    if rs.key != fs.key {
                                        Some(rs.data.as_slice())
                                    } else {
                                        None
                                    }
                                });
                                let m = compute_step_metrics(&b.t, &fs.data, ref_slice, band);
                                rows.push(json!({
                                    "slice_id": b.slice_id,
                                    "name": b.name,
                                    "note": b.note,
                                    "channel": fs.name,
                                    "unit": fs.unit,
                                    "metrics": m,
                                }));
                            }
                        }
                    }

                    // Determine winners across valid runs
                    let mut best_rise: Option<(String, f64)> = None;
                    let mut best_ov: Option<(String, f64)> = None;
                    let mut best_ts: Option<(String, f64)> = None;
                    let mut best_rmse: Option<(String, f64)> = None;
                    for r in &rows {
                        let m = &r["metrics"];
                        if m.get("valid").and_then(|v| v.as_bool()) == Some(true) {
                            let nm = r["name"].as_str().unwrap_or("").to_string();
                            if let Some(v) = m.get("rise_time_ms").and_then(|x| x.as_f64()) {
                                if best_rise.as_ref().map(|b| v < b.1).unwrap_or(true) {
                                    best_rise = Some((nm.clone(), v));
                                }
                            }
                            if let Some(v) = m.get("overshoot_pct").and_then(|x| x.as_f64()) {
                                if best_ov.as_ref().map(|b| v < b.1).unwrap_or(true) {
                                    best_ov = Some((nm.clone(), v));
                                }
                            }
                            if let Some(v) = m.get("settling_time_ms").and_then(|x| x.as_f64()) {
                                if best_ts.as_ref().map(|b| v < b.1).unwrap_or(true) {
                                    best_ts = Some((nm.clone(), v));
                                }
                            }
                            if let Some(v) = m.get("tracking_rmse").and_then(|x| x.as_f64()) {
                                if best_rmse.as_ref().map(|b| v < b.1).unwrap_or(true) {
                                    best_rmse = Some((nm.clone(), v));
                                }
                            }
                        }
                    }

                    if sync_gui {
                        if let Some(h) = hooks {
                            (h.send_ui_cmd)(json!({
                                "page": "lab",
                                "analysis_view": {
                                    "tab": "step",
                                    "focused_slice_id": ids.first(),
                                    "step_fbk": resolved_fbk_key,
                                    "step_ref": resolved_ref_key,
                                    "settle_band": band,
                                }
                            }));
                        }
                    }

                    Ok(json!({
                        "mode": "step_response_pk",
                        "runs": rows,
                        "best_by_metric": {
                            "fastest_rise_time": best_rise.map(|(n, v)| json!({"run": n, "rise_time_ms": v})),
                            "lowest_overshoot": best_ov.map(|(n, v)| json!({"run": n, "overshoot_pct": v})),
                            "fastest_settling": best_ts.map(|(n, v)| json!({"run": n, "settling_time_ms": v})),
                            "lowest_rmse": best_rmse.map(|(n, v)| json!({"run": n, "tracking_rmse": v})),
                        }
                    }))
                }

                "harmonic_thd" => {
                    let sid = default_target_slice_id(eng, args.get("slice_id").and_then(|v| v.as_str()));
                    let ch_q = args.get("channel").and_then(|v| v.as_str()).unwrap_or("Ia");
                    let win = args
                        .get("window_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("hann");
                    let poles = args
                        .get("pole_pairs")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(7) as u32;

                    let b = load_bundle_typed(eng, &sid, 16384)?;
                    let ser = find_series(eng, &b, ch_q)
                        .ok_or_else(|| format!("未找到通道 '{}'", ch_q))?;
                    let rpm = args.get("rpm_override").and_then(|v| v.as_f64()).or_else(|| {
                        b.channels
                            .iter()
                            .find(|c| {
                                c.key == "speed_est"
                                    || c.unit.eq_ignore_ascii_case("rpm")
                                    || c.name.contains("转速")
                            })
                            .and_then(|sc| {
                                if sc.data.is_empty() {
                                    None
                                } else {
                                    let s0 = sc.data.len() / 2;
                                    let avg: f64 = sc.data[s0..]
                                        .iter()
                                        .map(|&v| (v as f64).abs())
                                        .sum::<f64>()
                                        / (sc.data.len() - s0).max(1) as f64;
                                    Some(avg)
                                }
                            })
                    });

                    if sync_gui {
                        if let Some(h) = hooks {
                            (h.send_ui_cmd)(json!({
                                "page": "lab",
                                "analysis_view": {
                                    "tab": "fft",
                                    "focused_slice_id": b.slice_id,
                                    "fft_channel": ser.key,
                                    "fft_window": win,
                                    "pole_pairs": poles,
                                }
                            }));
                        }
                    }

                    let mut res = compute_harmonic_thd(&ser.data, b.hz, win, poles, rpm, true);
                    res["slice_id"] = json!(b.slice_id);
                    res["run_name"] = json!(b.name);
                    res["channel"] = json!(ser.name);
                    res["unit"] = json!(ser.unit);
                    Ok(res)
                }

                "filter_sandbox" => {
                    let sid = default_target_slice_id(eng, args.get("slice_id").and_then(|v| v.as_str()));
                    let ch_q = args
                        .get("channel")
                        .and_then(|v| v.as_str())
                        .unwrap_or("speed_est");
                    let ftype = args
                        .get("filter_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("iir1");
                    let param = args
                        .get("filter_param")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(25.0);

                    let b = load_bundle_typed(eng, &sid, 16384)?;
                    let ser = find_series(eng, &b, ch_q)
                        .ok_or_else(|| format!("未找到通道 '{}'", ch_q))?;

                    if sync_gui {
                        if let Some(h) = hooks {
                            (h.send_ui_cmd)(json!({
                                "page": "lab",
                                "analysis_view": {
                                    "tab": "filter",
                                    "focused_slice_id": b.slice_id,
                                    "filter_channel": ser.key,
                                    "filter_type": ftype,
                                    "filter_cutoff": param,
                                }
                            }));
                        }
                    }

                    if let Some(sweep) = args.get("sweep_params").and_then(|v| v.as_array()) {
                        let list: Vec<Value> = sweep
                            .iter()
                            .filter_map(|x| x.as_f64())
                            .map(|p| evaluate_digital_filter(&ser.data, b.hz, ftype, p))
                            .collect();
                        return Ok(json!({
                            "mode": "filter_sandbox_sweep",
                            "slice_id": b.slice_id,
                            "run_name": b.name,
                            "channel": ser.name,
                            "unit": ser.unit,
                            "filter_type": ftype,
                            "sweep_results": list,
                        }));
                    }

                    let mut res = evaluate_digital_filter(&ser.data, b.hz, ftype, param);
                    res["slice_id"] = json!(b.slice_id);
                    res["run_name"] = json!(b.name);
                    res["channel"] = json!(ser.name);
                    res["unit"] = json!(ser.unit);
                    Ok(res)
                }

                "overlay_diff" => {
                    let ch_q = args
                        .get("channel")
                        .and_then(|v| v.as_str())
                        .unwrap_or("speed_est");
                    let align_mode = args
                        .get("align_mode")
                        .and_then(|v| v.as_str())
                        .unwrap_or("rising50");
                    let thresh = args
                        .get("align_threshold")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(500.0);
                    let norm_100 = args
                        .get("normalize_100")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    // Either compare 2 channels in 1 run (if ref_channel provided & only 1 slice) OR 2 runs on same channel
                    let ids: Vec<String> = args
                        .get("slice_ids")
                        .and_then(|v| v.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_else(|| {
                            let st = eng.store.read().unwrap();
                            st.slices
                                .iter()
                                .filter(|s| s.meta.enabled)
                                .take(2)
                                .map(|s| s.meta.slice_id.clone())
                                .collect()
                        });

                    if ids.len() >= 2 {
                        let b_a = load_bundle_typed(eng, &ids[0], 16384)?;
                        let b_b = load_bundle_typed(eng, &ids[1], 16384)?;
                        let s_a = find_series(eng, &b_a, ch_q)
                            .ok_or_else(|| format!("{} 中未找到通道 '{}'", b_a.name, ch_q))?;
                        let s_b = find_series(eng, &b_b, ch_q)
                            .ok_or_else(|| format!("{} 中未找到通道 '{}'", b_b.name, ch_q))?;
                        let sh_a =
                            compute_auto_shift_sec(&b_a.t, &s_a.data, b_a.offset_x, align_mode, thresh);
                        let sh_b =
                            compute_auto_shift_sec(&b_b.t, &s_b.data, b_b.offset_x, align_mode, thresh);
                        let mut diff = compute_overlay_diff(
                            &b_a.t, &s_a.data, sh_a, &b_b.t, &s_b.data, sh_b, norm_100,
                        );
                        diff["curve_a"] = json!({ "slice_id": b_a.slice_id, "name": b_a.name, "note": b_a.note, "channel": s_a.name });
                        diff["curve_b"] = json!({ "slice_id": b_b.slice_id, "name": b_b.name, "note": b_b.note, "channel": s_b.name });

                        if sync_gui {
                            if let Some(h) = hooks {
                                (h.send_ui_cmd)(json!({
                                    "page": "lab",
                                    "analysis_view": {
                                        "tab": "overlay",
                                        "overlay_scope_mode": "multi_run",
                                        "overlay_channel": s_a.key,
                                        "norm_mode": if norm_100 { "norm100" } else { "raw" },
                                        "align_mode": align_mode,
                                        "align_threshold": thresh,
                                        "show_diff": true,
                                    }
                                }));
                            }
                        }
                        Ok(diff)
                    } else {
                        let sid = default_target_slice_id(eng, args.get("slice_id").and_then(|v| v.as_str()));
                        let ref_q = args
                            .get("ref_channel")
                            .and_then(|v| v.as_str())
                            .unwrap_or("speed_ref");
                        let b = load_bundle_typed(eng, &sid, 16384)?;
                        let s_a = find_series(eng, &b, ch_q)
                            .ok_or_else(|| format!("未找到通道 '{}'", ch_q))?;
                        let s_b = find_series(eng, &b, ref_q)
                            .ok_or_else(|| format!("未找到对比通道 '{}'", ref_q))?;
                        let mut diff = compute_overlay_diff(
                            &b.t, &s_a.data, 0.0, &b.t, &s_b.data, 0.0, norm_100,
                        );
                        diff["run_name"] = json!(b.name);
                        diff["curve_a"] = json!(s_a.name);
                        diff["curve_b"] = json!(s_b.name);
                        Ok(diff)
                    }
                }

                "xy_envelope" => {
                    let sid = default_target_slice_id(eng, args.get("slice_id").and_then(|v| v.as_str()));
                    let x_q = args.get("x_channel").and_then(|v| v.as_str()).unwrap_or("Vd");
                    let y_q = args.get("y_channel").and_then(|v| v.as_str()).unwrap_or("Vq");
                    let b = load_bundle_typed(eng, &sid, 16384)?;
                    let sx = find_series(eng, &b, x_q)
                        .ok_or_else(|| format!("未找到 X 通道 '{}'", x_q))?;
                    let sy = find_series(eng, &b, y_q)
                        .ok_or_else(|| format!("未找到 Y 通道 '{}'", y_q))?;

                    if sync_gui {
                        if let Some(h) = hooks {
                            (h.send_ui_cmd)(json!({
                                "page": "lab",
                                "analysis_view": {
                                    "tab": "xy",
                                    "focused_slice_id": b.slice_id,
                                    "xy_x": sx.key,
                                    "xy_y": sy.key,
                                }
                            }));
                        }
                    }

                    let mut res = compute_xy_envelope(&sx.data, &sy.data, &sx.name, &sy.name);
                    res["slice_id"] = json!(b.slice_id);
                    res["run_name"] = json!(b.name);
                    Ok(res)
                }

                "full_stats" => {
                    let sid = default_target_slice_id(eng, args.get("slice_id").and_then(|v| v.as_str()));
                    let b = load_bundle_typed(eng, &sid, 16384)?;
                    if sync_gui {
                        if let Some(h) = hooks {
                            (h.send_ui_cmd)(json!({
                                "page": "lab",
                                "analysis_view": {
                                    "tab": "stats",
                                    "focused_slice_id": b.slice_id,
                                }
                            }));
                        }
                    }
                    Ok(compute_full_stats(&b))
                }

                _ => Err(format!("未知的分析模式 '{}'", mode)),
            }
        }

        "foc_navigate_and_view" => {
            // Persist any follow preferences in profile too so offline & restart persist cleanly
            if let Some(sv) = args.get("scope_view").and_then(|v| v.as_object()) {
                let mut prof = eng.profile.write().unwrap();
                let mut changed = false;
                if let Some(fe) = sv.get("follow_enabled").and_then(|v| v.as_bool()) {
                    prof.follow_enabled = fe;
                    changed = true;
                }
                if let Some(fa) = sv.get("follow_anchor_ratio").and_then(|v| v.as_f64()) {
                    if fa.is_finite() {
                        prof.follow_anchor_ratio = fa.clamp(0.10, 1.00);
                        changed = true;
                    }
                }
                if let Some(fs) = sv.get("follow_span_sec").and_then(|v| v.as_f64()) {
                    if fs.is_finite() && fs > 0.0 {
                        prof.follow_span_sec = fs.clamp(0.0005, 36000.0);
                        changed = true;
                    }
                }
                if changed {
                    let _ = prof.save(None);
                }
            }

            if let Some(h) = hooks {
                if args
                    .get("bring_to_front")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    (h.bring_to_front)();
                }
                (h.send_ui_cmd)(args.clone());
                Ok(json!({ "status": "ok", "gui_synced": true }))
            } else {
                Ok(json!({
                    "status": "ok",
                    "gui_synced": false,
                    "note": "桌面窗口当前未打开，持久化配置已写入，打开窗口或调用 foc_launch_studio 后生效"
                }))
            }
        }

        "foc_stream_and_serial_control" => {
            let act = args
                .get("action")
                .and_then(|v| v.as_str())
                .ok_or("缺少 action")?;
            {
                let mut prof = eng.profile.write().unwrap();
                let mut touched = false;
                if let Some(pr) = args.get("protocol").and_then(|v| v.as_str()) {
                    prof.connection.protocol = pr.to_lowercase();
                    touched = true;
                }
                if let Some(db) = args.get("data_bits").and_then(|v| v.as_u64()) {
                    prof.connection.data_bits = db as u8;
                    touched = true;
                }
                if let Some(sb) = args.get("stop_bits").and_then(|v| v.as_u64()) {
                    prof.connection.stop_bits = sb as u8;
                    touched = true;
                }
                if let Some(pa) = args.get("parity").and_then(|v| v.as_str()) {
                    prof.connection.parity = pa.to_lowercase();
                    touched = true;
                }
                if let Some(fc) = args.get("flow_control").and_then(|v| v.as_str()) {
                    prof.connection.flow_control = fc.to_lowercase();
                    touched = true;
                }
                if let Some(dtr) = args.get("dtr").and_then(|v| v.as_bool()) {
                    prof.connection.dtr = dtr;
                    touched = true;
                }
                if let Some(rts) = args.get("rts").and_then(|v| v.as_bool()) {
                    prof.connection.rts = rts;
                    touched = true;
                }
                if let Some(rh) = args.get("raw_hex_display").and_then(|v| v.as_bool()) {
                    prof.connection.raw_hex_display = rh;
                    touched = true;
                }
                if let Some(nhz) = args.get("nominal_sample_hz").and_then(|v| v.as_f64()) {
                    if nhz > 0.0 && nhz.is_finite() {
                        prof.nominal_sample_hz = nhz;
                        touched = true;
                    }
                }
                if touched {
                    let _ = prof.save(None);
                }
            }

            match act {
                "list_ports" => Ok(json!({ "ports": TelemetryEngine::list_serial_ports() })),
                "connect_serial" => {
                    let port = args
                        .get("port")
                        .and_then(|v| v.as_str())
                        .ok_or("缺少 port")?;
                    let baud = args
                        .get("baudrate")
                        .and_then(|v| v.as_u64())
                        .unwrap_or_else(|| eng.profile.read().unwrap().connection.baudrate as u64)
                        as u32;
                    let lbl = eng.start_serial(port, baud)?;
                    if let Some(h) = hooks {
                        (h.notify_ui)("stream");
                    }
                    Ok(json!({ "status": "connected", "source": lbl }))
                }
                "connect_udp" => {
                    let addr = args
                        .get("udp_bind_addr")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| {
                            eng.profile.read().unwrap().connection.udp_bind_addr.clone()
                        });
                    let lbl = eng.start_udp(&addr)?;
                    if let Some(h) = hooks {
                        (h.notify_ui)("stream");
                    }
                    Ok(json!({ "status": "connected", "source": lbl }))
                }
                "connect_tcp" => {
                    let addr = args
                        .get("tcp_remote_addr")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| {
                            eng.profile
                                .read()
                                .unwrap()
                                .connection
                                .tcp_remote_addr
                                .clone()
                        });
                    let lbl = eng.start_tcp(&addr)?;
                    if let Some(h) = hooks {
                        (h.notify_ui)("stream");
                    }
                    Ok(json!({ "status": "connected", "source": lbl }))
                }
                "disconnect" => {
                    eng.stop_streams();
                    if let Some(h) = hooks {
                        (h.notify_ui)("stream");
                    }
                    Ok(json!({ "status": "disconnected" }))
                }
                "load_csv" => {
                    let p = args
                        .get("csv_path")
                        .and_then(|v| v.as_str())
                        .ok_or("缺少 csv_path")?;
                    let adopt = args
                        .get("adopt_csv_headers")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let info = eng.load_csv_instant(Path::new(p), None, adopt)?;
                    if let Some(h) = hooks {
                        (h.notify_ui)("fit_all");
                    }
                    Ok(json!(info))
                }
                "replay_csv" | "restart_replay" => {
                    let opt_p = args
                        .get("csv_path")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.trim().is_empty())
                        .map(PathBuf::from);
                    if act == "replay_csv" && opt_p.is_none() {
                        return Err("缺少 csv_path".to_string());
                    }
                    let spd = args
                        .get("replay_speed")
                        .and_then(|v| v.as_f64())
                        .unwrap_or_else(|| eng.profile.read().unwrap().replay_speed);
                    let smode = args.get("start_mode").and_then(|v| v.as_str());
                    let res = eng.start_csv_replay_ex(opt_p.as_deref(), spd, smode)?;
                    if let Some(h) = hooks {
                        (h.notify_ui)("stream");
                    }
                    Ok(res)
                }
                "set_replay_speed" => {
                    let spd = args
                        .get("replay_speed")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(1.0);
                    let applied = eng.set_replay_speed(spd);
                    if let Some(h) = hooks {
                        (h.notify_ui)("stream");
                    }
                    Ok(json!({ "status": "ok", "replay_speed": applied }))
                }
                "pause" => {
                    eng.paused.store(true, Ordering::Relaxed);
                    if let Some(h) = hooks {
                        (h.notify_ui)("stream");
                    }
                    Ok(json!({ "status": "paused" }))
                }
                "resume" => {
                    eng.paused.store(false, Ordering::Relaxed);
                    if let Some(h) = hooks {
                        (h.notify_ui)("stream");
                    }
                    Ok(json!({ "status": "resumed" }))
                }
                "clear_buffer" => {
                    eng.store.write().unwrap().clear_waveforms();
                    eng.bump_data_seq();
                    if let Some(h) = hooks {
                        (h.notify_ui)("stream");
                    }
                    Ok(json!({ "status": "cleared" }))
                }
                "clear_console" => {
                    eng.store.write().unwrap().console_lines.clear();
                    eng.bump_data_seq();
                    if let Some(h) = hooks {
                        (h.notify_ui)("stream");
                    }
                    Ok(json!({ "status": "console_cleared" }))
                }
                "read_console" => {
                    let st = eng.store.read().unwrap();
                    Ok(json!({
                        "bookmarks": st.bookmarks,
                        "console_lines": st.console_lines,
                    }))
                }
                "set_mode" => {
                    let m = args
                        .get("operating_mode")
                        .and_then(|v| v.as_str())
                        .unwrap_or("normal");
                    {
                        let mut prof = eng.profile.write().unwrap();
                        prof.operating_mode = if m == "motor_id" {
                            "motor_id".to_string()
                        } else {
                            "normal".to_string()
                        };
                        prof.save(None)?;
                    }
                    if let Some(h) = hooks {
                        (h.notify_ui)("profile");
                    }
                    Ok(json!({ "status": "ok", "operating_mode": m }))
                }
                "set_spike_filter" => {
                    let mut prof = eng.profile.write().unwrap();
                    if let Some(en) = args.get("spike_filter_enabled").and_then(|v| v.as_bool()) {
                        prof.spike_filter_enabled = en;
                    }
                    if let Some(th) = args.get("spike_threshold").and_then(|v| v.as_f64()) {
                        prof.spike_threshold = th as f32;
                    }
                    prof.save(None)?;
                    let en = prof.spike_filter_enabled;
                    let th = prof.spike_threshold;
                    drop(prof);
                    if let Some(h) = hooks {
                        (h.notify_ui)("profile");
                    }
                    Ok(json!({ "status": "ok", "spike_filter_enabled": en, "spike_threshold": th }))
                }
                "send_command" => {
                    let cmd = args
                        .get("command")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    match eng.send_serial_command(cmd) {
                        Ok(()) => Ok(json!({ "status": "sent", "command": cmd })),
                        Err(e) => Ok(json!({ "status": "serial_not_connected", "detail": e })),
                    }
                }
                _ => Err(format!("未知操作 '{}'", act)),
            }
        }

        "foc_save_or_load_profile" => {
            let act = args
                .get("action")
                .and_then(|v| v.as_str())
                .ok_or("缺少 action")?;
            match act {
                "list" => {
                    let mut names = Vec::new();
                    if let Ok(rd) = fs::read_dir(profiles_dir()) {
                        for e in rd.flatten() {
                            let p = e.path();
                            if p.extension().and_then(|s| s.to_str()) == Some("json") {
                                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                                    names.push(stem.to_string());
                                }
                            }
                        }
                    }
                    names.sort();
                    Ok(json!({ "profiles": names, "current": eng.profile.read().unwrap().profile_name }))
                }
                "save" => {
                    let mut prof = eng.profile.write().unwrap();
                    let pname = args
                        .get("profile_name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| prof.profile_name.clone());
                    prof.profile_name = pname.clone();
                    let custom = profiles_dir().join(format!("{}.json", pname));
                    prof.save(Some(&custom))?;
                    prof.save(None)?;
                    Ok(json!({ "status": "saved", "path": custom.to_string_lossy() }))
                }
                "load" => {
                    let pname = args
                        .get("profile_name")
                        .and_then(|v| v.as_str())
                        .ok_or("缺少 profile_name")?;
                    let custom = profiles_dir().join(format!("{}.json", pname));
                    if !custom.is_file() {
                        return Err(format!("预设文件不存在: {:?}", custom));
                    }
                    let loaded = StudioProfile::load_or_default(Some(&custom));
                    loaded.save(None)?;
                    *eng.profile.write().unwrap() = loaded;
                    eng.bump_math_seq();
                    if let Some(h) = hooks {
                        (h.notify_ui)("profile");
                    }
                    Ok(json!({ "status": "loaded", "profile_name": pname }))
                }
                "reset_default" => {
                    let def = create_default_profile();
                    def.save(Some(&default_profile_path()))?;
                    *eng.profile.write().unwrap() = def;
                    eng.bump_math_seq();
                    if let Some(h) = hooks {
                        (h.notify_ui)("profile");
                    }
                    Ok(json!({ "status": "reset_to_default" }))
                }
                _ => Err(format!("未知预设操作 '{}'", act)),
            }
        }

        "foc_capture_screenshot" => {
            let target = args
                .get("target")
                .and_then(|v| v.as_str())
                .unwrap_or("full_window");
            if let Some(h) = hooks {
                let p = (h.capture_png)(target)?;
                Ok(json!({
                    "status": "ok",
                    "target": target,
                    "screenshot_path": p.to_string_lossy(),
                }))
            } else {
                Err("当前图形界面未打开；请先打开 FluxScope 或调用 foc_launch_studio".to_string())
            }
        }

        "foc_launch_studio" => {
            if let Some(h) = hooks {
                (h.bring_to_front)();
                if let Some(csv) = args.get("csv_path").and_then(|v| v.as_str()) {
                    if !csv.trim().is_empty() {
                        let info = eng.load_csv_instant(Path::new(csv), None, false)?;
                        (h.notify_ui)("fit_all");
                        return Ok(json!({ "status": "focused_and_loaded", "csv": info }));
                    }
                }
                Ok(json!({ "status": "already_running_and_focused" }))
            } else {
                let gui_exe = resolve_gui_exe_path();
                let mut cmd = std::process::Command::new(&gui_exe);
                if let Some(csv) = args.get("csv_path").and_then(|v| v.as_str()) {
                    if !csv.trim().is_empty() {
                        cmd.arg("--csv").arg(csv);
                    }
                }
                let child = cmd
                    .spawn()
                    .map_err(|e| format!("启动 {:?} 失败: {}", gui_exe, e))?;
                Ok(json!({
                    "status": "launched",
                    "pid": child.id(),
                    "exe": gui_exe.to_string_lossy(),
                }))
            }
        }

        _ => Err(format!("未知的 MCP 工具 '{}'", name)),
    }
}

/// Starts embedded HTTP JSON-RPC 2.0 MCP server on http://127.0.0.1:18580/mcp
pub fn start_embedded_http_mcp(eng: Arc<TelemetryEngine>, hooks: Arc<GuiHooks>) -> bool {
    let server = match tiny_http::Server::http(MCP_HTTP_ADDR) {
        Ok(s) => s,
        Err(_) => return false,
    };

    thread::spawn(move || {
        for mut req in server.incoming_requests() {
            let url = req.url().to_string();
            let method = req.method().as_str().to_uppercase();

            if method == "OPTIONS" {
                let resp = tiny_http::Response::from_string("")
                    .with_status_code(204)
                    .with_header(header("Access-Control-Allow-Origin", "*"))
                    .with_header(header("Access-Control-Allow-Methods", "GET, POST, OPTIONS"))
                    .with_header(header("Access-Control-Allow-Headers", "Content-Type"));
                let _ = req.respond(resp);
                continue;
            }

            if method == "GET" && url.starts_with("/health") {
                let body = json!({
                    "ok": true,
                    "app": "FluxScope Native",
                    "version": "2.0.0",
                    "tools_count": mcp_tools_schema().len(),
                    "screenshots_dir": screenshots_dir().to_string_lossy(),
                })
                .to_string();
                let _ = req.respond(json_response(200, body));
                continue;
            }

            let mut body_str = String::new();
            let _ = req.as_reader().read_to_string(&mut body_str);

            if method == "POST" && url.starts_with("/api/call") {
                let parsed: Value = serde_json::from_str(&body_str).unwrap_or_else(|_| json!({}));
                let tname = parsed.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let targs = parsed.get("arguments").cloned().unwrap_or_else(|| json!({}));
                match execute_mcp_tool(&eng, Some(hooks.as_ref()), tname, &targs) {
                    Ok(res) => {
                        let _ = req.respond(json_response(
                            200,
                            json!({ "ok": true, "result": res }).to_string(),
                        ));
                    }
                    Err(err) => {
                        let _ = req.respond(json_response(
                            200,
                            json!({ "ok": false, "error": err }).to_string(),
                        ));
                    }
                }
                continue;
            }

            if method == "POST" && (url == "/mcp" || url == "/mcp/" || url == "/") {
                let msg: Value = match serde_json::from_str(&body_str) {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = req.respond(json_response(
                            400,
                            json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":e.to_string()}}).to_string(),
                        ));
                        continue;
                    }
                };
                if let Some(reply) = handle_jsonrpc_message(&eng, Some(hooks.as_ref()), &msg) {
                    let _ = req.respond(json_response(200, reply.to_string()));
                } else {
                    let _ = req.respond(tiny_http::Response::empty(202));
                }
                continue;
            }

            let _ = req.respond(json_response(404, json!({"error":"not_found"}).to_string()));
        }
    });

    true
}

pub fn handle_jsonrpc_message(
    eng: &Arc<TelemetryEngine>,
    hooks: Option<&GuiHooks>,
    msg: &Value,
) -> Option<Value> {
    let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");
    let req_id = msg.get("id").cloned();

    if req_id.is_none() || method.starts_with("notifications/") {
        return None;
    }
    let id = req_id.unwrap_or(Value::Null);

    match method {
        "initialize" => {
            let pv = msg
                .get("params")
                .and_then(|p| p.get("protocolVersion"))
                .and_then(|v| v.as_str())
                .unwrap_or("2024-11-05");
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": pv,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "fluxscope", "version": "2.0.0" }
                }
            }))
        }
        "ping" => Some(json!({ "jsonrpc": "2.0", "id": id, "result": {} })),
        "resources/list" => {
            Some(json!({ "jsonrpc": "2.0", "id": id, "result": { "resources": [] } }))
        }
        "prompts/list" => Some(json!({ "jsonrpc": "2.0", "id": id, "result": { "prompts": [] } })),
        "tools/list" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "tools": mcp_tools_schema() }
        })),
        "tools/call" => {
            let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));
            let tname = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let targs = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match execute_mcp_tool(eng, hooks, tname, &targs) {
                Ok(res) => {
                    let txt =
                        serde_json::to_string_pretty(&res).unwrap_or_else(|_| "{}".to_string());
                    Some(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{ "type": "text", "text": txt }],
                            "isError": false
                        }
                    }))
                }
                Err(err) => Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [{ "type": "text", "text": format!("调用失败: {}", err) }],
                        "isError": true
                    }
                })),
            }
        }
        _ => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": format!("Method not found: {}", method) }
        })),
    }
}

fn header(k: &str, v: &str) -> tiny_http::Header {
    tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap()
}

fn json_response(code: u16, body: String) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    tiny_http::Response::from_string(body)
        .with_status_code(code)
        .with_header(header("Content-Type", "application/json; charset=utf-8"))
        .with_header(header("Access-Control-Allow-Origin", "*"))
}

fn gui_http_post(path: &str, body: &str, timeout_ms: u64) -> Option<Value> {
    let addr: SocketAddr = MCP_HTTP_ADDR.parse().ok()?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(250)).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(timeout_ms)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(1000)));

    let req = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        path,
        MCP_HTTP_ADDR,
        body.len(),
        body
    );
    stream.write_all(req.as_bytes()).ok()?;
    stream.flush().ok()?;

    let mut resp_bytes = Vec::new();
    stream.read_to_end(&mut resp_bytes).ok()?;
    let resp_str = String::from_utf8_lossy(&resp_bytes);
    let (_, json_body) = resp_str.split_once("\r\n\r\n")?;
    serde_json::from_str(json_body.trim()).ok()
}

fn gui_is_online() -> bool {
    let Ok(addr) = MCP_HTTP_ADDR.parse::<SocketAddr>() else {
        return false;
    };
    TcpStream::connect_timeout(&addr, Duration::from_millis(180)).is_ok()
}

fn handle_offline_stdio_call(eng: &Arc<TelemetryEngine>, msg: &Value) -> Option<Value> {
    let disk_prof = StudioProfile::load_or_default(None);
    *eng.profile.write().unwrap() = disk_prof;
    eng.store.write().unwrap().reload_slices_from_disk();

    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));
    let tname = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let targs = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

    match execute_mcp_tool(eng, None, tname, &targs) {
        Ok(res) => {
            if tname == "foc_launch_studio" {
                for _ in 0..40 {
                    if gui_is_online() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(150));
                }
            }
            let txt = serde_json::to_string_pretty(&res).unwrap_or_else(|_| "{}".to_string());
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{ "type": "text", "text": txt }],
                    "isError": false
                }
            }))
        }
        Err(err) => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{ "type": "text", "text": format!("调用失败: {}", err) }],
                "isError": true
            }
        })),
    }
}

/// Built-in single-executable Stdio MCP server (`FluxScope.exe --mcp`)
pub fn run_stdio_mcp_bridge() {
    let profile = StudioProfile::load_or_default(None);
    let headless_eng = TelemetryEngine::new(profile);

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    for line_res in stdin.lock().lines() {
        let Ok(line) = line_res else {
            break;
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let req_id = msg.get("id").cloned();
        if req_id.is_none() || method.starts_with("notifications/") {
            continue;
        }
        let id = req_id.unwrap_or(Value::Null);

        let reply = if method == "tools/call" && gui_is_online() {
            let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));
            let payload = json!({
                "name": params.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                "arguments": params.get("arguments").cloned().unwrap_or_else(|| json!({})),
            })
            .to_string();

            if let Some(api_res) = gui_http_post("/api/call", &payload, 15000) {
                if api_res.get("ok").and_then(|v| v.as_bool()) == Some(true) {
                    let txt = serde_json::to_string_pretty(&api_res["result"])
                        .unwrap_or_else(|_| "{}".to_string());
                    Some(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{ "type": "text", "text": txt }],
                            "isError": false
                        }
                    }))
                } else {
                    let err = api_res
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown error");
                    Some(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{ "type": "text", "text": format!("调用失败: {}", err) }],
                            "isError": true
                        }
                    }))
                }
            } else {
                handle_offline_stdio_call(&headless_eng, &msg)
            }
        } else if method == "tools/call" {
            handle_offline_stdio_call(&headless_eng, &msg)
        } else if method == "tools/list" {
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "tools": mcp_tools_schema() }
            }))
        } else {
            handle_jsonrpc_message(&headless_eng, None, &msg)
        };

        if let Some(resp_obj) = reply {
            if let Ok(out_line) = serde_json::to_string(&resp_obj) {
                let _ = writeln!(stdout, "{}", out_line);
                let _ = stdout.flush();
            }
        }
    }
}
