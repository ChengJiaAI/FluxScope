use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Balanced professional oscilloscope trace palette (high visibility on both #141416 Dark & #FFFFFF Light)
pub const TRACE_PALETTE: &[&str] = &[
    "#0EA5E9", // Sky Cyan
    "#F59E0B", // Oscilloscope Amber
    "#10B981", // Signal Emerald
    "#EF4444", // Coral Red
    "#8B5CF6", // Violet
    "#EC4899", // Rose
    "#14B8A6", // Teal
    "#84CC16", // Lime
    "#F97316", // Orange
    "#6366F1", // Indigo
    "#06B6D4", // Cyan
    "#D946EF", // Magenta
];

pub fn app_data_dir() -> PathBuf {
    // Support portable mode if portable_data folder sits beside executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let portable = parent.join("portable_data");
            if portable.is_dir() {
                return ensure_subdirs(portable);
            }
        }
    }
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    ensure_subdirs(base.join("FluxScope"))
}

fn ensure_subdirs(root: PathBuf) -> PathBuf {
    let _ = fs::create_dir_all(root.join("profiles"));
    let _ = fs::create_dir_all(root.join("slices"));
    let _ = fs::create_dir_all(root.join("screenshots"));
    root
}

pub fn profiles_dir() -> PathBuf {
    app_data_dir().join("profiles")
}

pub fn slices_dir() -> PathBuf {
    app_data_dir().join("slices")
}

pub fn screenshots_dir() -> PathBuf {
    app_data_dir().join("screenshots")
}

pub fn default_profile_path() -> PathBuf {
    profiles_dir().join("default_profile.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumEntry {
    pub label: String,
    #[serde(default = "default_gray_badge")]
    pub color: String,
}

fn default_gray_badge() -> String {
    "#71717A".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    pub index: usize,
    pub name: String,
    pub alias: String,
    #[serde(default)]
    pub unit: String,
    #[serde(default = "default_trace_color")]
    pub color: String,
    #[serde(default = "default_line_width")]
    pub width: f32,
    #[serde(default = "default_display_gain")]
    pub display_gain: f32,
    #[serde(default)]
    pub visible: bool,
    #[serde(default = "default_none_plot")]
    pub plot_id: String,
    #[serde(default)]
    pub show_card: bool,
    #[serde(default = "default_decimals")]
    pub decimals: usize,
    /// "float" | "enum" | "bitmask"
    #[serde(default = "default_decoder_type")]
    pub decoder_type: String,
    #[serde(default)]
    pub enum_map: BTreeMap<String, EnumEntry>,
    #[serde(default)]
    pub bitmask_map: BTreeMap<String, String>,
}

fn default_trace_color() -> String {
    "#0EA5E9".to_string()
}
fn default_line_width() -> f32 {
    1.5
}
fn default_display_gain() -> f32 {
    1.0
}
fn default_none_plot() -> String {
    "none".to_string()
}
fn default_decimals() -> usize {
    2
}
fn default_decoder_type() -> String {
    "float".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MathChannelConfig {
    pub id: String,
    pub name: String,
    pub alias: String,
    pub expression: String,
    #[serde(default)]
    pub unit: String,
    #[serde(default = "default_trace_color")]
    pub color: String,
    #[serde(default = "default_line_width")]
    pub width: f32,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default = "default_plot_1")]
    pub plot_id: String,
    #[serde(default = "default_true")]
    pub show_card: bool,
    #[serde(default = "default_decimals_3")]
    pub decimals: usize,
}

fn default_true() -> bool {
    true
}
fn default_plot_1() -> String {
    "plot_1".to_string()
}
fn default_decimals_3() -> usize {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlotPanelConfig {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub y_label: String,
    #[serde(default = "default_weight")]
    pub height_weight: f32,
}

fn default_weight() -> f32 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XYPlotConfig {
    #[serde(default = "default_vd")]
    pub x_source: String,
    #[serde(default = "default_vq")]
    pub y_source: String,
    #[serde(default = "default_tail")]
    pub tail_points: usize,
    #[serde(default = "default_true")]
    pub aspect_locked: bool,
    #[serde(default)]
    pub enabled: bool,
}

fn default_vd() -> String {
    "Vd".to_string()
}
fn default_vq() -> String {
    "Vq".to_string()
}
fn default_tail() -> usize {
    800
}

impl Default for XYPlotConfig {
    fn default() -> Self {
        Self {
            x_source: "Vd".to_string(),
            y_source: "Vq".to_string(),
            tail_points: 800,
            aspect_locked: true,
            enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    /// "serial" | "udp" | "tcp"
    #[serde(default = "default_iface_serial")]
    pub interface_type: String,
    #[serde(default)]
    pub last_port: String,
    #[serde(default = "default_baud")]
    pub baudrate: u32,
    #[serde(default = "default_data_bits")]
    pub data_bits: u8,
    #[serde(default = "default_stop_bits")]
    pub stop_bits: u8,
    /// "none" | "even" | "odd"
    #[serde(default = "default_none_str")]
    pub parity: String,
    /// "none" | "hardware" | "software"
    #[serde(default = "default_none_str")]
    pub flow_control: String,
    #[serde(default)]
    pub dtr: bool,
    #[serde(default)]
    pub rts: bool,
    /// VOFA+ compatible protocols: "justfloat" | "firewater" | "rawdata"
    #[serde(default = "default_proto_justfloat")]
    pub protocol: String,
    #[serde(default)]
    pub raw_hex_display: bool,
    #[serde(default = "default_udp_addr")]
    pub udp_bind_addr: String,
    #[serde(default = "default_tcp_addr")]
    pub tcp_remote_addr: String,
}

fn default_iface_serial() -> String {
    "serial".to_string()
}
fn default_data_bits() -> u8 {
    8
}
fn default_stop_bits() -> u8 {
    1
}
fn default_none_str() -> String {
    "none".to_string()
}
fn default_proto_justfloat() -> String {
    "justfloat".to_string()
}
fn default_udp_addr() -> String {
    "0.0.0.0:1347".to_string()
}
fn default_tcp_addr() -> String {
    "127.0.0.1:1347".to_string()
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            interface_type: default_iface_serial(),
            last_port: String::new(),
            baudrate: default_baud(),
            data_bits: 8,
            stop_bits: 1,
            parity: default_none_str(),
            flow_control: default_none_str(),
            dtr: false,
            rts: false,
            protocol: default_proto_justfloat(),
            raw_hex_display: false,
            udp_bind_addr: default_udp_addr(),
            tcp_remote_addr: default_tcp_addr(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusDecodeConfig {
    #[serde(default)]
    pub initialized: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub state_enabled: bool,
    pub state_channel: Option<usize>,
    #[serde(default = "default_foc_state_enum")]
    pub state_enum_map: BTreeMap<String, EnumEntry>,
    #[serde(default = "default_true")]
    pub fault_enabled: bool,
    pub fault_channel: Option<usize>,
    /// "bitmask" (16-bit flags Bit 0..15) | "enum" (exact integer code)
    #[serde(default = "default_fault_mode")]
    pub fault_mode: String,
    #[serde(default = "default_foc_fault_bitmask")]
    pub fault_map: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    pub isr_enabled: bool,
    pub isr_channel: Option<usize>,
    #[serde(default = "default_isr_label")]
    pub isr_label: String,
    #[serde(default = "default_isr_full_scale")]
    pub isr_full_scale: f32,
    #[serde(default = "default_isr_unit")]
    pub isr_unit: String,
}

fn default_fault_mode() -> String {
    "bitmask".to_string()
}
fn default_isr_label() -> String {
    "20kHz中断".to_string()
}
fn default_isr_full_scale() -> f32 {
    3000.0
}
fn default_isr_unit() -> String {
    "cyc".to_string()
}

impl Default for StatusDecodeConfig {
    fn default() -> Self {
        Self {
            initialized: false,
            enabled: true,
            state_enabled: true,
            state_channel: Some(6),
            state_enum_map: default_foc_state_enum(),
            fault_enabled: true,
            fault_channel: Some(7),
            fault_mode: default_fault_mode(),
            fault_map: default_foc_fault_bitmask(),
            isr_enabled: true,
            isr_channel: Some(20),
            isr_label: default_isr_label(),
            isr_full_scale: default_isr_full_scale(),
            isr_unit: default_isr_unit(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudioProfile {
    pub profile_name: String,
    /// "dark" | "light"
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_true")]
    pub right_drawer_open: bool,
    #[serde(default = "default_drawer_width")]
    pub right_drawer_width: u32,
    /// "latest" | "hover" | "t1" | "t2"
    #[serde(default = "default_drawer_source")]
    pub drawer_data_source: String,
    /// "normal" | "motor_id"
    #[serde(default = "default_mode")]
    pub operating_mode: String,
    #[serde(default = "default_baud")]
    pub baudrate: u32,
    #[serde(default = "default_sample_hz")]
    pub nominal_sample_hz: f64,
    #[serde(default = "default_replay_speed")]
    pub replay_speed: f64,
    /// "zero" | "t1" | "t2"
    #[serde(default = "default_replay_start")]
    pub replay_start_mode: String,
    #[serde(default = "default_true")]
    pub follow_enabled: bool,
    #[serde(default = "default_follow_anchor")]
    pub follow_anchor_ratio: f64,
    #[serde(default = "default_follow_span")]
    pub follow_span_sec: f64,
    #[serde(default = "default_follow_custom")]
    pub follow_custom_sec: f64,
    #[serde(default = "default_true")]
    pub layout_locked: bool,
    #[serde(default)]
    pub spike_filter_enabled: bool,
    #[serde(default = "default_spike_thresh")]
    pub spike_threshold: f32,
    #[serde(default)]
    pub connection: ConnectionConfig,
    #[serde(default)]
    pub status_decode: StatusDecodeConfig,
    pub channels: Vec<ChannelConfig>,
    pub math_channels: Vec<MathChannelConfig>,
    pub plots: Vec<PlotPanelConfig>,
    #[serde(default)]
    pub xy_config: XYPlotConfig,
    #[serde(default)]
    pub widgets_initialized: bool,
    #[serde(default)]
    pub drawer_widgets: Vec<serde_json::Value>,
}

fn default_theme() -> String {
    "dark".to_string()
}
fn default_drawer_width() -> u32 {
    356
}
fn default_drawer_source() -> String {
    "latest".to_string()
}
fn default_mode() -> String {
    "normal".to_string()
}
fn default_baud() -> u32 {
    460800
}
fn default_sample_hz() -> f64 {
    1000.0
}
fn default_replay_speed() -> f64 {
    1.0
}
fn default_replay_start() -> String {
    "zero".to_string()
}
fn default_follow_anchor() -> f64 {
    1.0
}
fn default_follow_span() -> f64 {
    5.0
}
fn default_follow_custom() -> f64 {
    10.0
}
fn default_spike_thresh() -> f32 {
    1_000_000.0
}

fn parse_ch_idx_key(s: &str) -> Option<usize> {
    s.strip_prefix("CH")
        .or_else(|| s.strip_prefix("ch"))
        .and_then(|r| r.parse::<usize>().ok())
}

impl StudioProfile {
    pub fn load_or_default(custom_path: Option<&Path>) -> Self {
        let p = custom_path
            .map(|x| x.to_path_buf())
            .unwrap_or_else(default_profile_path);
        if p.is_file() {
            if let Ok(txt) = fs::read_to_string(&p) {
                if let Ok(mut prof) = serde_json::from_str::<StudioProfile>(&txt) {
                    if prof.migrate_and_sync() {
                        let _ = prof.save(Some(&p));
                    }
                    return prof;
                }
            }
        }
        let prof = create_default_profile();
        let _ = prof.save(Some(&p));
        prof
    }

    /// Ensures backwards compatibility when upgrading from earlier profile JSONs
    pub fn migrate_and_sync(&mut self) -> bool {
        let mut migrated = false;
        if self.connection.baudrate == 0 {
            self.connection.baudrate = self.baudrate.max(9600);
            migrated = true;
        }
        if self.right_drawer_width < 240 || self.right_drawer_width > 720 {
            self.right_drawer_width = 356;
            migrated = true;
        }
        if !self.status_decode.initialized {
            self.connection.baudrate = self.baudrate.max(9600);
            if let Some(ch) = self.channels.iter().find(|c| c.decoder_type == "enum") {
                self.status_decode.state_channel = Some(ch.index);
                if !ch.enum_map.is_empty() {
                    self.status_decode.state_enum_map = ch.enum_map.clone();
                }
            }
            if let Some(ch) = self.channels.iter().find(|c| c.decoder_type == "bitmask") {
                self.status_decode.fault_channel = Some(ch.index);
                if !ch.bitmask_map.is_empty() {
                    self.status_decode.fault_map = ch.bitmask_map.clone();
                }
            }
            if let Some(ch) = self.channels.iter().find(|c| c.alias == "isr_max_cyc") {
                self.status_decode.isr_channel = Some(ch.index);
            }
            self.status_decode.initialized = true;
            migrated = true;
        }
        if !self.widgets_initialized {
            self.ensure_default_widgets();
            migrated = true;
        }
        self.baudrate = self.connection.baudrate;
        self.sync_channels_with_status_decode();
        migrated
    }

    pub fn resolve_key_for_alias(&self, alias_or_key: &str) -> String {
        if alias_or_key.starts_with("CH") || alias_or_key.starts_with('M') {
            return alias_or_key.to_string();
        }
        if let Some(ch) = self
            .channels
            .iter()
            .find(|c| c.alias == alias_or_key || c.name == alias_or_key)
        {
            return format!("CH{}", ch.index);
        }
        if let Some(mc) = self
            .math_channels
            .iter()
            .find(|m| m.alias == alias_or_key || m.name == alias_or_key || m.id == alias_or_key)
        {
            return mc.id.clone();
        }
        "CH0".to_string()
    }

    /// Migrates legacy `status_decode` + `show_card` + `xy_config` + `quick_visibility` into modular Right-Drawer widgets
    pub fn ensure_default_widgets(&mut self) {
        use serde_json::json;
        let sd = &self.status_decode;
        let st_ch_num = sd.state_channel.unwrap_or(6);
        let st_ch = format!("CH{}", st_ch_num);

        let mut enum_rows: Vec<(i64, serde_json::Value)> = sd
            .state_enum_map
            .iter()
            .map(|(k, v)| {
                let ord = k.parse::<i64>().unwrap_or(9999);
                (
                    ord,
                    json!({ "k": k, "label": v.label, "color": v.color }),
                )
            })
            .collect();
        enum_rows.sort_by_key(|x| x.0);
        let enum_vec: Vec<serde_json::Value> = enum_rows.into_iter().map(|(_, v)| v).collect();

        let isr_ch = format!("CH{}", sd.isr_channel.unwrap_or(20));
        let isr_max = sd.isr_full_scale.max(100.0);
        let flt_ch_num = sd.fault_channel.unwrap_or(7);
        let flt_ch = format!("CH{}", flt_ch_num);

        let mut flt_rows: Vec<(i64, serde_json::Value)> = sd
            .fault_map
            .iter()
            .map(|(k, lbl)| {
                let ord = k.parse::<i64>().unwrap_or(9999);
                (
                    ord,
                    json!({ "key": k, "label": lbl, "color": "#EF4444", "expr": "" }),
                )
            })
            .collect();
        flt_rows.sort_by_key(|x| x.0);
        let flt_vec: Vec<serde_json::Value> = flt_rows.into_iter().map(|(_, v)| v).collect();

        let mut num_cards: Vec<serde_json::Value> = Vec::new();
        for ch in &self.channels {
            if ch.show_card
                && Some(ch.index) != sd.state_channel
                && Some(ch.index) != sd.fault_channel
                && Some(ch.index) != sd.isr_channel
            {
                num_cards.push(json!({
                    "ch": format!("CH{}", ch.index),
                    "dec": ch.decimals,
                    "sub": "none",
                    "rules": []
                }));
            }
        }
        for mc in &self.math_channels {
            if mc.show_card {
                num_cards.push(json!({
                    "ch": mc.id,
                    "dec": mc.decimals,
                    "sub": "none",
                    "rules": []
                }));
            }
        }

        let xy_x = self.resolve_key_for_alias(&self.xy_config.x_source);
        let xy_y = self.resolve_key_for_alias(&self.xy_config.y_source);

        self.drawer_widgets = vec![
            json!({
                "id": "w_enum_1",
                "type": "enum_badge",
                "enabled": sd.enabled && sd.state_enabled,
                "showHeader": true,
                "title": "状态枚举徽章",
                "widthPct": 50,
                "height": null,
                "channel": st_ch,
                "unmatchedLabel": "未知状态",
                "unmatchedColor": "#8B5CF6",
                "enumMap": enum_vec,
                "frameRules": [
                    { "mode": "builder", "leftVar": format!("CH{}", st_ch_num), "op": "==", "rightMode": "const", "rightConst": 6, "rightVar": "CH0", "expr": format!("CH{} == 6", st_ch_num), "color": "#EF4444", "alertFrame": false, "frameColor": "#EF4444" }
                ]
            }),
            json!({
                "id": "w_bar_1",
                "type": "linear_bar",
                "enabled": sd.enabled && sd.isr_enabled,
                "showHeader": true,
                "title": "线性进度条",
                "widthPct": 50,
                "height": null,
                "channel": isr_ch,
                "unitOverride": sd.isr_unit,
                "decimals": 0,
                "min": 0,
                "max": isr_max,
                "originMode": "left",
                "showValue": true,
                "showPct": true,
                "normalColorMode": "fixed",
                "baseColor": "#10B981",
                "colorRules": [
                    { "mode": "builder", "leftVar": "val", "op": ">=", "rightMode": "const", "rightConst": (isr_max * 0.8).round(), "rightVar": "CH0", "expr": "val >= 2400", "color": "#EF4444", "alertFrame": true, "frameColor": "#EF4444" },
                    { "mode": "builder", "leftVar": "val", "op": ">=", "rightMode": "const", "rightConst": (isr_max * 0.6).round(), "rightVar": "CH0", "expr": "val >= 1800", "color": "#F59E0B", "alertFrame": false, "frameColor": "#F59E0B" }
                ]
            }),
            json!({
                "id": "w_led_1",
                "type": "bitmask_led",
                "enabled": sd.enabled && sd.fault_enabled,
                "showHeader": true,
                "title": "位掩码状态灯阵",
                "widthPct": 100,
                "height": null,
                "channel": flt_ch,
                "decodeMode": "bitmask",
                "displayStyle": "compact_pill",
                "gridCols": 2,
                "okText": "0x0000 正常",
                "items": flt_vec,
                "frameRules": [
                    { "mode": "builder", "leftVar": format!("CH{}", flt_ch_num), "op": ">", "rightMode": "const", "rightConst": 0, "rightVar": "CH0", "expr": format!("CH{} > 0", flt_ch_num), "color": "#EF4444", "alertFrame": false, "frameColor": "#EF4444" }
                ]
            }),
            json!({
                "id": "w_num_1",
                "type": "num_grid",
                "enabled": true,
                "showHeader": true,
                "title": "多通道数值网格",
                "widthPct": 100,
                "height": null,
                "cols": 2,
                "cards": num_cards,
                "frameRules": []
            }),
            json!({
                "id": "w_xy_1",
                "type": "xy_lissajous",
                "enabled": self.xy_config.enabled,
                "showHeader": true,
                "title": "X-Y 轨迹图",
                "widthPct": 100,
                "height": 172,
                "xChannel": xy_x,
                "yChannel": xy_y,
                "scaleMode": "auto",
                "fixedMax": 50,
                "tailPoints": self.xy_config.tail_points.clamp(32, 400),
                "normalColorMode": "fixed",
                "baseColor": "#8B5CF6",
                "colorRules": []
            }),
            json!({
                "id": "w_qv_1",
                "type": "quick_visibility",
                "enabled": true,
                "showHeader": true,
                "title": "曲线显隐面板",
                "widthPct": 100,
                "height": null,
                "cols": 1,
                "showLiveNum": true
            }),
        ];
        self.widgets_initialized = true;
    }

    /// Keeps `status_decode` (used for automatic state/fault transition bookmarks & MCP status) synchronized with active widgets
    pub fn sync_channels_with_status_decode(&mut self) {
        self.baudrate = self.connection.baudrate;

        // Mirror active `enum_badge` & `bitmask_led` widgets into `status_decode` so automatic bookmarks stay accurate!
        if self.widgets_initialized {
            if let Some(ew) = self.drawer_widgets.iter().find(|w| {
                w.get("type").and_then(|v| v.as_str()) == Some("enum_badge")
                    && w.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true)
            }) {
                if let Some(ch_s) = ew.get("channel").and_then(|v| v.as_str()) {
                    if let Some(idx) = parse_ch_idx_key(ch_s) {
                        self.status_decode.state_enabled = true;
                        self.status_decode.state_channel = Some(idx);
                    }
                }
                if let Some(arr) = ew.get("enumMap").and_then(|v| v.as_array()) {
                    let mut m = BTreeMap::new();
                    for row in arr {
                        let k = row.get("k").and_then(|v| v.as_str()).unwrap_or("").trim();
                        if !k.is_empty() {
                            let lbl = row
                                .get("label")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let col = row
                                .get("color")
                                .and_then(|v| v.as_str())
                                .unwrap_or("#71717A")
                                .to_string();
                            m.insert(k.to_string(), EnumEntry { label: lbl, color: col });
                        }
                    }
                    if !m.is_empty() {
                        self.status_decode.state_enum_map = m;
                    }
                }
            }

            if let Some(fw) = self.drawer_widgets.iter().find(|w| {
                w.get("type").and_then(|v| v.as_str()) == Some("bitmask_led")
                    && w.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true)
                    && w.get("decodeMode").and_then(|v| v.as_str()).unwrap_or("bitmask") == "bitmask"
            }) {
                if let Some(ch_s) = fw.get("channel").and_then(|v| v.as_str()) {
                    if let Some(idx) = parse_ch_idx_key(ch_s) {
                        self.status_decode.fault_enabled = true;
                        self.status_decode.fault_channel = Some(idx);
                        self.status_decode.fault_mode = "bitmask".to_string();
                    }
                }
                if let Some(arr) = fw.get("items").and_then(|v| v.as_array()) {
                    let mut m = BTreeMap::new();
                    for row in arr {
                        let k = row.get("key").and_then(|v| v.as_str()).unwrap_or("").trim();
                        let lbl = row.get("label").and_then(|v| v.as_str()).unwrap_or("").trim();
                        if !k.is_empty() && !lbl.is_empty() {
                            m.insert(k.to_string(), lbl.to_string());
                        }
                    }
                    if !m.is_empty() {
                        self.status_decode.fault_map = m;
                    }
                }
            }
        }

        let st_idx = if self.status_decode.state_enabled {
            self.status_decode.state_channel
        } else {
            None
        };
        let flt_idx = if self.status_decode.fault_enabled {
            self.status_decode.fault_channel
        } else {
            None
        };
        for ch in &mut self.channels {
            if Some(ch.index) == st_idx {
                ch.decoder_type = "enum".to_string();
                ch.enum_map = self.status_decode.state_enum_map.clone();
            } else if Some(ch.index) == flt_idx {
                ch.decoder_type = "bitmask".to_string();
                ch.bitmask_map = self.status_decode.fault_map.clone();
            } else if ch.decoder_type == "enum" || ch.decoder_type == "bitmask" {
                ch.decoder_type = "float".to_string();
            }
        }
    }

    pub fn save(&self, custom_path: Option<&Path>) -> Result<PathBuf, String> {
        let p = custom_path
            .map(|x| x.to_path_buf())
            .unwrap_or_else(default_profile_path);
        if let Some(parent) = p.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let txt = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&p, txt).map_err(|e| e.to_string())?;
        Ok(p)
    }

    pub fn ensure_channel_capacity(&mut self, count: usize) -> bool {
        let mut changed = false;
        while self.channels.len() < count {
            let idx = self.channels.len();
            self.channels.push(ChannelConfig {
                index: idx,
                name: format!("通道 CH{}", idx),
                alias: format!("ch{}", idx),
                unit: String::new(),
                color: TRACE_PALETTE[idx % TRACE_PALETTE.len()].to_string(),
                width: 1.5,
                display_gain: 1.0,
                visible: false,
                plot_id: "none".to_string(),
                show_card: false,
                decimals: 2,
                decoder_type: "float".to_string(),
                enum_map: BTreeMap::new(),
                bitmask_map: BTreeMap::new(),
            });
            changed = true;
        }
        changed
    }
}

pub fn default_foc_state_enum() -> BTreeMap<String, EnumEntry> {
    let items = [
        ("0", "IDLE 待机", "#71717A"),
        ("1", "CALIB 零偏校准", "#0EA5E9"),
        ("2", "ALIGN 预定位", "#F59E0B"),
        ("3", "OPEN_LOOP 开环强拖", "#FB923C"),
        ("4", "CLOSED_LOOP 闭环FOC", "#10B981"),
        ("5", "BRAKING 主动刹车", "#A855F7"),
        ("6", "FAULT 故障保护", "#EF4444"),
        ("7", "MOTOR_ID 参数辨识", "#3B82F6"),
    ];
    let mut m = BTreeMap::new();
    for (k, lbl, col) in items {
        m.insert(
            k.to_string(),
            EnumEntry {
                label: lbl.to_string(),
                color: col.to_string(),
            },
        );
    }
    m
}

pub fn default_foc_fault_bitmask() -> BTreeMap<String, String> {
    let items = [
        ("0", "软件过流 (OVER_CURRENT)"),
        ("1", "母线过压 (OVER_VOLTAGE)"),
        ("2", "母线欠压 (UNDER_VOLTAGE)"),
        ("3", "过温保护 (OVER_TEMP)"),
        ("4", "超速保护 (OVER_SPEED)"),
        ("5", "堵转/失步 (STALL)"),
        ("6", "硬件CMPSS过流锁存 (HW_OVERCURRENT)"),
        ("7", "缺相故障 (PHASE_LOSS)"),
        ("8", "零偏校准失败 (OFFSET_ERR)"),
        ("9", "通信丢失 (COMM_LOST)"),
        ("10", "MOS管自检异常 (MOSFET_FAIL)"),
    ];
    let mut m = BTreeMap::new();
    for (k, v) in items {
        m.insert(k.to_string(), v.to_string());
    }
    m
}

pub fn create_default_profile() -> StudioProfile {
    let plots = vec![
        PlotPanelConfig {
            id: "plot_1".to_string(),
            title: "转速环与电流环".to_string(),
            y_label: "RPM / A".to_string(),
            height_weight: 1.15,
        },
        PlotPanelConfig {
            id: "plot_2".to_string(),
            title: "电压矢量 · 母线 · 调制比".to_string(),
            y_label: "V / Ratio".to_string(),
            height_weight: 0.85,
        },
    ];

    let specs: &[(&str, &str, &str, bool, &str, bool, &str)] = &[
        ("目标转速", "speed_ref", "RPM", true, "plot_1", true, "float"),
        ("反馈转速", "speed_est", "RPM", true, "plot_1", true, "float"),
        ("Iq给定", "Iq_ref", "A", true, "plot_1", false, "float"),
        ("Iq反馈", "Iq_fbk", "A", true, "plot_1", true, "float"),
        ("速度环PI输出", "speed_pi_iq", "A", false, "none", false, "float"),
        ("三相电流峰值", "Iabc_max", "A", false, "plot_1", true, "float"),
        ("FOC状态机", "foc_state", "", false, "none", true, "enum"),
        ("故障位标志", "fault_code", "hex", false, "none", true, "bitmask"),
        ("Vd输出电压", "Vd", "V", true, "plot_2", false, "float"),
        ("Vq输出电压", "Vq", "V", true, "plot_2", false, "float"),
        ("A相占空比", "duty_a", "", false, "none", false, "float"),
        ("B相占空比", "duty_b", "", false, "none", false, "float"),
        ("C相占空比", "duty_c", "", false, "none", false, "float"),
        ("控制目标转速", "ctrl_rpm", "RPM", false, "none", false, "float"),
        ("观测器电角速度", "flux_omega", "rad/s", false, "none", false, "float"),
        ("串口丢包计数", "tx_drop", "cnt", false, "none", false, "float"),
        ("电流采样耗时", "prof_cur", "cyc", false, "none", false, "float"),
        ("观测器耗时", "prof_obs", "cyc", false, "none", false, "float"),
        ("PLL相位误差", "pll_err", "rad", false, "plot_2", true, "float"),
        ("SVPWM输出耗时", "prof_out", "cyc", false, "none", false, "float"),
        ("20kHz中断峰值", "isr_max_cyc", "cyc", false, "none", true, "float"),
        ("中断超限次数", "isr_overrun", "cnt", false, "none", false, "float"),
        ("FOC总耗时", "prof_run", "cyc", false, "none", false, "float"),
        ("A相电流Ia", "Ia", "A", false, "none", false, "float"),
        ("发送频率", "vofa_hz", "Hz", false, "none", false, "float"),
        ("母线电压原始", "Vbus_raw", "V", false, "none", false, "float"),
        ("母线电压滤波", "Vbus", "V", true, "plot_2", true, "float"),
    ];

    let mut channels = Vec::with_capacity(specs.len());
    for (idx, &(name, alias, unit, vis, pid, card, dec)) in specs.iter().enumerate() {
        channels.push(ChannelConfig {
            index: idx,
            name: name.to_string(),
            alias: alias.to_string(),
            unit: unit.to_string(),
            color: TRACE_PALETTE[idx % TRACE_PALETTE.len()].to_string(),
            width: 1.5,
            display_gain: 1.0,
            visible: vis,
            plot_id: pid.to_string(),
            show_card: card,
            decimals: if unit == "RPM" || unit == "cyc" || unit == "cnt" || unit == "Hz" {
                0
            } else {
                2
            },
            decoder_type: dec.to_string(),
            enum_map: if dec == "enum" {
                default_foc_state_enum()
            } else {
                BTreeMap::new()
            },
            bitmask_map: if dec == "bitmask" {
                default_foc_fault_bitmask()
            } else {
                BTreeMap::new()
            },
        });
    }

    let math_channels = vec![
        MathChannelConfig {
            id: "M0".to_string(),
            name: "电压调制比".to_string(),
            alias: "mod_idx".to_string(),
            expression: "sqrt(Vd^2 + Vq^2) / maximum(Vbus / 1.73205, 1.0)".to_string(),
            unit: "ratio".to_string(),
            color: "#EC4899".to_string(),
            width: 1.6,
            visible: true,
            plot_id: "plot_2".to_string(),
            show_card: true,
            decimals: 3,
        },
        MathChannelConfig {
            id: "M1".to_string(),
            name: "估算电功率".to_string(),
            alias: "p_elec".to_string(),
            expression: "1.5 * (Vd * 0.0 + Vq * Iq_fbk)".to_string(),
            unit: "W".to_string(),
            color: "#14B8A6".to_string(),
            width: 1.5,
            visible: false,
            plot_id: "plot_2".to_string(),
            show_card: true,
            decimals: 1,
        },
    ];

    let mut status_decode = StatusDecodeConfig::default();
    status_decode.initialized = true;

    let mut prof = StudioProfile {
        profile_name: "FOC_27CH_标准预设".to_string(),
        theme: "dark".to_string(),
        right_drawer_open: true,
        right_drawer_width: 356,
        drawer_data_source: "latest".to_string(),
        operating_mode: "normal".to_string(),
        baudrate: 460800,
        nominal_sample_hz: 1000.0,
        replay_speed: 1.0,
        replay_start_mode: "zero".to_string(),
        follow_enabled: true,
        follow_anchor_ratio: 1.0,
        follow_span_sec: 5.0,
        follow_custom_sec: 10.0,
        layout_locked: true,
        spike_filter_enabled: false,
        spike_threshold: 1_000_000.0,
        connection: ConnectionConfig::default(),
        status_decode,
        channels,
        math_channels,
        plots,
        xy_config: XYPlotConfig::default(),
        widgets_initialized: false,
        drawer_widgets: Vec::new(),
    };
    prof.ensure_default_widgets();
    prof
}

/// Binary-persisted waveform slice for fast loading & multi-slice independent X/Y overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceMeta {
    pub slice_id: String,
    pub name: String,
    #[serde(default)]
    pub note: String,
    pub created_at: String,
    pub sample_rate_hz: f64,
    pub sample_count: usize,
    pub channel_keys: Vec<String>,
    pub labels: BTreeMap<String, String>,
    pub enabled: bool,
    pub selected_key: String,
    pub scale_x: f64,
    pub offset_x: f64,
    pub scale_y: f32,
    pub offset_y: f32,
    pub color: String,
    /// "solid" | "dash" | "dot"
    pub line_style: String,
    pub width: f32,
}

#[derive(Debug, Clone)]
pub struct WaveformSlice {
    pub meta: SliceMeta,
    pub t_rel: Vec<f64>,
    pub data: BTreeMap<String, Vec<f32>>,
}

const FSLICE_MAGIC: &[u8; 8] = b"FLXSLCV2";

impl WaveformSlice {
    pub fn save_to_disk(&self) -> Result<PathBuf, String> {
        let dir = slices_dir();
        let path = dir.join(format!("{}.fslice", self.meta.slice_id));
        let meta_json = serde_json::to_vec(&self.meta).map_err(|e| e.to_string())?;
        let mut f = fs::File::create(&path).map_err(|e| e.to_string())?;
        f.write_all(FSLICE_MAGIC).map_err(|e| e.to_string())?;
        let meta_len = meta_json.len() as u32;
        f.write_all(&meta_len.to_le_bytes())
            .map_err(|e| e.to_string())?;
        f.write_all(&meta_json).map_err(|e| e.to_string())?;

        // Write t_rel (f64 LE)
        for &tv in &self.t_rel {
            f.write_all(&tv.to_le_bytes()).map_err(|e| e.to_string())?;
        }
        // Write each channel in meta.channel_keys order (f32 LE)
        let n = self.t_rel.len();
        for key in &self.meta.channel_keys {
            if let Some(vec) = self.data.get(key) {
                for i in 0..n {
                    let v = vec.get(i).copied().unwrap_or(0.0);
                    f.write_all(&v.to_le_bytes()).map_err(|e| e.to_string())?;
                }
            } else {
                let zero = 0.0f32.to_le_bytes();
                for _ in 0..n {
                    f.write_all(&zero).map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(path)
    }

    pub fn load_from_disk(path: &Path) -> Result<Self, String> {
        let mut bytes = Vec::new();
        fs::File::open(path)
            .map_err(|e| e.to_string())?
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() < 12 || &bytes[0..8] != FSLICE_MAGIC {
            return Err("Invalid fslice header".to_string());
        }
        let meta_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        if bytes.len() < 12 + meta_len {
            return Err("Truncated fslice metadata".to_string());
        }
        let meta: SliceMeta =
            serde_json::from_slice(&bytes[12..12 + meta_len]).map_err(|e| e.to_string())?;
        let mut pos = 12 + meta_len;
        let n = meta.sample_count;
        if bytes.len() < pos + n * 8 {
            return Err("Truncated fslice t_rel".to_string());
        }
        let mut t_rel = Vec::with_capacity(n);
        for _ in 0..n {
            let v = f64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
            t_rel.push(v);
            pos += 8;
        }
        let mut data = BTreeMap::new();
        for key in &meta.channel_keys {
            if bytes.len() < pos + n * 4 {
                break;
            }
            let mut col = Vec::with_capacity(n);
            for _ in 0..n {
                let v = f32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap());
                col.push(v);
                pos += 4;
            }
            data.insert(key.clone(), col);
        }
        Ok(Self { meta, t_rel, data })
    }

    pub fn export_csv(&self, out_path: &Path) -> Result<PathBuf, String> {
        if let Some(parent) = out_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let mut f = fs::File::create(out_path).map_err(|e| e.to_string())?;
        // UTF-8 BOM so Excel on Chinese Windows opens columns cleanly
        f.write_all(&[0xEF, 0xBB, 0xBF]).map_err(|e| e.to_string())?;
        let mut header = String::from("time_s");
        for k in &self.meta.channel_keys {
            let lbl = self.meta.labels.get(k).cloned().unwrap_or_else(|| k.clone());
            header.push(',');
            header.push_str(&lbl.replace(',', "_"));
        }
        header.push('\n');
        f.write_all(header.as_bytes()).map_err(|e| e.to_string())?;

        let n = self.t_rel.len();
        let mut line_buf = String::with_capacity(256);
        for i in 0..n {
            line_buf.clear();
            line_buf.push_str(&format!("{:.6}", self.t_rel[i]));
            for k in &self.meta.channel_keys {
                let v = self
                    .data
                    .get(k)
                    .and_then(|c| c.get(i))
                    .copied()
                    .unwrap_or(0.0);
                line_buf.push_str(&format!(",{:.6}", v));
            }
            line_buf.push('\n');
            f.write_all(line_buf.as_bytes())
                .map_err(|e| e.to_string())?;
        }
        Ok(out_path.to_path_buf())
    }
}

pub fn now_timestamp_label() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{}", secs)
}
