use crate::math_eval::{compile_expression, eval_vectorized};
use crate::models::{
    now_timestamp_label, slices_dir, StudioProfile, WaveformSlice, SliceMeta, TRACE_PALETTE,
};
use rustfft::{num_complex::Complex, FftPlanner};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

const JUSTFLOAT_TAIL: [u8; 4] = [0x00, 0x00, 0x80, 0x7F];
const DEFAULT_MAX_FRAMES: usize = 500_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarkEvent {
    pub time_s: f64,
    pub kind: String, // "state" | "fault" | "user"
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecimatedTrace {
    pub key: String,
    pub plot_id: String,
    pub label: String,
    pub color: String,
    pub width: f32,
    #[serde(default = "default_trace_gain")]
    pub gain: f32,
    pub t: Vec<f64>,
    pub y: Vec<f32>,
}

fn default_trace_gain() -> f32 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardReadout {
    pub key: String,
    pub name: String,
    pub unit: String,
    pub color: String,
    pub value_str: String,
    pub raw_value: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadoutSnapshot {
    pub has_data: bool,
    pub sample_time: f64,
    pub status_decode_enabled: bool,
    pub state_enabled: bool,
    pub state_label: String,
    pub state_color: String,
    pub fault_enabled: bool,
    pub fault_code: u16,
    pub active_faults: Vec<String>,
    pub isr_enabled: bool,
    pub isr_label: String,
    pub isr_full_scale: f32,
    pub isr_unit: String,
    pub isr_cycles: i32,
    pub cards: Vec<CardReadout>,
    #[serde(default)]
    pub channel_vals: BTreeMap<String, f32>,
    #[serde(default)]
    pub channel_peaks: BTreeMap<String, f32>,
    #[serde(default)]
    pub channel_mins: BTreeMap<String, f32>,
    #[serde(default)]
    pub series_tails: BTreeMap<String, Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewportPayload {
    pub data_seq: u64,
    pub source_label: String,
    pub protocol: String,
    pub interface_type: String,
    pub is_streaming: bool,
    pub paused: bool,
    pub buffered_frames: usize,
    pub sample_rate_hz: f64,
    pub rx_kbps: f64,
    pub full_t_min: f64,
    pub full_t_max: f64,
    pub view_t_min: f64,
    pub view_t_max: f64,
    pub cursor_enabled: bool,
    pub cursor_t1: f64,
    pub cursor_t2: f64,
    pub sample_time: f64,
    pub spike_rejected: u64,
    pub status_decode_enabled: bool,
    pub state_enabled: bool,
    pub state_label: String,
    pub state_color: String,
    pub fault_enabled: bool,
    pub fault_code: u16,
    pub active_faults: Vec<String>,
    pub isr_enabled: bool,
    pub isr_label: String,
    pub isr_full_scale: f32,
    pub isr_unit: String,
    pub isr_cycles: i32,
    pub cards: Vec<CardReadout>,
    #[serde(default)]
    pub channel_vals: BTreeMap<String, f32>,
    #[serde(default)]
    pub channel_peaks: BTreeMap<String, f32>,
    #[serde(default)]
    pub channel_mins: BTreeMap<String, f32>,
    #[serde(default)]
    pub series_tails: BTreeMap<String, Vec<f32>>,
    pub traces: Vec<DecimatedTrace>,
    pub xy_x: Vec<f32>,
    pub xy_y: Vec<f32>,
    pub math_errors: BTreeMap<String, String>,
    pub bookmark_count: usize,
    pub console_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelWindowStat {
    pub key: String,
    pub label: String,
    pub mean: f64,
    pub rms: f64,
    pub std_dev: f64,
    pub min: f64,
    pub max: f64,
    pub p2p: f64,
    pub overshoot_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FftPeak {
    pub freq_hz: f64,
    pub amplitude: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FftResult {
    pub channel: String,
    pub label: String,
    pub sample_rate_hz: f64,
    pub n_samples: usize,
    pub dc_component: f64,
    pub peaks: Vec<FftPeak>,
    pub freqs: Vec<f64>,
    pub amps: Vec<f32>,
}

/// Hybrid JustFloat binary + interleaved ASCII text demultiplexer
pub struct HybridJustFloatParser {
    buf: Vec<u8>,
    ascii_staging: Vec<u8>,
    inferred_ch_count: Option<usize>,
}

impl HybridJustFloatParser {
    pub fn new(hint_channels: Option<usize>) -> Self {
        Self {
            buf: Vec::with_capacity(16384),
            ascii_staging: Vec::with_capacity(512),
            inferred_ch_count: hint_channels.filter(|&c| c > 0 && c <= 64),
        }
    }

    pub fn feed(&mut self, chunk: &[u8]) -> (Vec<Vec<f32>>, Vec<String>) {
        if chunk.is_empty() {
            return (Vec::new(), Vec::new());
        }
        self.buf.extend_from_slice(chunk);
        if self.buf.len() > 262_144 {
            let drain = self.buf.len() - 65_536;
            self.buf.drain(0..drain);
        }

        let mut frames = Vec::new();
        let mut lines = Vec::new();

        loop {
            let tail_pos = self
                .buf
                .windows(4)
                .position(|w| w == JUSTFLOAT_TAIL);
            let Some(tpos) = tail_pos else {
                // If no binary tail and buffer has newline, flush ASCII lines safely
                if self.inferred_ch_count.is_none() || self.buf.len() > 512 {
                    if let Some(nl) = self.buf.iter().rposition(|&b| b == b'\n') {
                        if tpos_unlikely(&self.buf[..=nl]) {
                            let txt_bytes: Vec<u8> = self.buf.drain(0..=nl).collect();
                            Self::extract_ascii(&txt_bytes, &mut lines);
                        }
                    }
                }
                break;
            };

            let candidate_n = if let Some(expected) = self.inferred_ch_count {
                let need = expected * 4;
                if tpos >= need {
                    if tpos > need {
                        self.ascii_staging.extend_from_slice(&self.buf[..tpos - need]);
                    }
                    Some(expected)
                } else if tpos >= 4 && tpos % 4 == 0 && (tpos / 4) <= 64 {
                    let n = tpos / 4;
                    self.inferred_ch_count = Some(n);
                    Some(n)
                } else {
                    // False tail inside payload or partial startup frame
                    self.ascii_staging.extend_from_slice(&self.buf[..tpos]);
                    self.buf.drain(0..tpos + 4);
                    continue;
                }
            } else if tpos >= 4 && tpos <= 256 && tpos % 4 == 0 {
                let n = tpos / 4;
                self.inferred_ch_count = Some(n);
                Some(n)
            } else if tpos > 256 {
                // Likely ASCII text preceded the first frame; try finding last '\n' before tpos
                let mut chosen = None;
                for n_try in (1..=64).rev() {
                    let start = tpos.saturating_sub(n_try * 4);
                    if start > 0 && (self.buf[start - 1] == b'\n' || self.buf[start - 1] == b'\r') {
                        self.ascii_staging.extend_from_slice(&self.buf[..start]);
                        self.inferred_ch_count = Some(n_try);
                        chosen = Some(n_try);
                        break;
                    }
                }
                if chosen.is_none() {
                    self.buf.drain(0..tpos + 4);
                    continue;
                }
                chosen
            } else {
                self.buf.drain(0..tpos + 4);
                continue;
            };

            if let Some(n_ch) = candidate_n {
                let fstart = tpos - n_ch * 4;
                let mut vals = Vec::with_capacity(n_ch);
                for c in 0..n_ch {
                    let off = fstart + c * 4;
                    let b: [u8; 4] = self.buf[off..off + 4].try_into().unwrap();
                    vals.push(f32::from_le_bytes(b));
                }
                frames.push(vals);
                self.buf.drain(0..tpos + 4);
            }
        }

        if !self.ascii_staging.is_empty() {
            let staged = std::mem::take(&mut self.ascii_staging);
            Self::extract_ascii(&staged, &mut lines);
        }

        (frames, lines)
    }

    fn extract_ascii(raw: &[u8], out: &mut Vec<String>) {
        // Keep printable ASCII + UTF-8 multibyte bytes
        let filtered: Vec<u8> = raw
            .iter()
            .copied()
            .filter(|&b| b == b'\n' || b == b'\r' || b == b'\t' || (32..=126).contains(&b) || b >= 128)
            .collect();
        if filtered.is_empty() {
            return;
        }
        let s = String::from_utf8_lossy(&filtered);
        for ln in s.lines() {
            let t = ln.trim();
            if t.len() >= 2 {
                out.push(t.to_string());
            }
        }
    }
}

fn tpos_unlikely(slice: &[u8]) -> bool {
    slice
        .iter()
        .all(|&b| b == b'\n' || b == b'\r' || b == b'\t' || (32..=126).contains(&b))
}

/// VOFA+ FireWater protocol parser (`printf("a, b, c\n")` or `"any_prefix: a, b, c\r\n"`)
pub struct FireWaterParser {
    buf: Vec<u8>,
}

impl FireWaterParser {
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(8192),
        }
    }

    pub fn feed(&mut self, chunk: &[u8]) -> (Vec<Vec<f32>>, Vec<String>) {
        if chunk.is_empty() {
            return (Vec::new(), Vec::new());
        }
        self.buf.extend_from_slice(chunk);
        if self.buf.len() > 131_072 {
            let drain = self.buf.len() - 32_768;
            self.buf.drain(0..drain);
        }

        let mut frames = Vec::new();
        let mut lines = Vec::new();

        while let Some(nl) = self.buf.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = self.buf.drain(0..=nl).collect();
            let raw_str = String::from_utf8_lossy(&line_bytes);
            let trimmed = raw_str.trim();
            if trimmed.is_empty() {
                continue;
            }

            // VOFA+ FireWater allows optional `prefix: v0, v1, v2`
            let payload_part = if let Some((_, rhs)) = trimmed.rsplit_once(':') {
                let r = rhs.trim();
                if r.bytes().any(|b| b.is_ascii_digit()) {
                    r
                } else {
                    trimmed
                }
            } else {
                trimmed
            };

            let parts: Vec<&str> = payload_part.split(',').map(|s| s.trim()).collect();
            let parsed_floats: Option<Vec<f32>> = if parts.len() >= 2 {
                let mut v = Vec::with_capacity(parts.len());
                let mut all_ok = true;
                for p in &parts {
                    if p.is_empty() {
                        all_ok = false;
                        break;
                    }
                    match p.parse::<f32>() {
                        Ok(fv) if fv.is_finite() => v.push(fv),
                        _ => {
                            all_ok = false;
                            break;
                        }
                    }
                }
                if all_ok { Some(v) } else { None }
            } else if parts.len() == 1 && !parts[0].is_empty() {
                match parts[0].parse::<f32>() {
                    Ok(fv) if fv.is_finite() => Some(vec![fv]),
                    _ => None,
                }
            } else {
                None
            };

            if let Some(vals) = parsed_floats {
                frames.push(vals);
            } else {
                lines.push(trimmed.to_string());
            }
        }

        (frames, lines)
    }
}

/// VOFA+ RawData mode (pure serial/socket assistant without float frame extraction)
pub struct RawDataParser {
    pub hex_display: bool,
    buf: Vec<u8>,
}

impl RawDataParser {
    pub fn new(hex_display: bool) -> Self {
        Self {
            hex_display,
            buf: Vec::with_capacity(4096),
        }
    }

    pub fn feed(&mut self, chunk: &[u8]) -> (Vec<Vec<f32>>, Vec<String>) {
        if chunk.is_empty() {
            return (Vec::new(), Vec::new());
        }
        let mut lines = Vec::new();
        if self.hex_display {
            for row in chunk.chunks(16) {
                let hex_s: Vec<String> = row.iter().map(|b| format!("{:02X}", b)).collect();
                let ascii_s: String = row
                    .iter()
                    .map(|&b| if (32..=126).contains(&b) { b as char } else { '.' })
                    .collect();
                lines.push(format!("[HEX {:02}B] {:<47} | {}", row.len(), hex_s.join(" "), ascii_s));
            }
        } else {
            self.buf.extend_from_slice(chunk);
            while let Some(nl) = self.buf.iter().position(|&b| b == b'\n') {
                let line_b: Vec<u8> = self.buf.drain(0..=nl).collect();
                let s = String::from_utf8_lossy(&line_b).trim_end_matches(&['\r', '\n'][..]).to_string();
                if !s.is_empty() {
                    lines.push(s);
                }
            }
            if self.buf.len() >= 256 {
                let s = String::from_utf8_lossy(&self.buf).to_string();
                self.buf.clear();
                if !s.trim().is_empty() {
                    lines.push(s);
                }
            }
        }
        (Vec::new(), lines)
    }
}

pub enum StreamProtocolParser {
    JustFloat(HybridJustFloatParser),
    FireWater(FireWaterParser),
    RawData(RawDataParser),
}

impl StreamProtocolParser {
    pub fn from_config(protocol: &str, hint_ch: Option<usize>, raw_hex: bool) -> Self {
        match protocol.to_lowercase().as_str() {
            "firewater" => Self::FireWater(FireWaterParser::new()),
            "rawdata" => Self::RawData(RawDataParser::new(raw_hex)),
            _ => Self::JustFloat(HybridJustFloatParser::new(hint_ch)),
        }
    }

    pub fn feed(&mut self, chunk: &[u8]) -> (Vec<Vec<f32>>, Vec<String>) {
        match self {
            Self::JustFloat(p) => p.feed(chunk),
            Self::FireWater(p) => p.feed(chunk),
            Self::RawData(p) => p.feed(chunk),
        }
    }
}

enum ActiveTx {
    Serial(Box<dyn serialport::SerialPort>),
    Tcp(std::net::TcpStream),
    Udp(std::net::UdpSocket, Arc<Mutex<Option<std::net::SocketAddr>>>),
}

pub struct InnerStore {
    pub timestamps: Vec<f64>,
    pub columns: Vec<Vec<f32>>,
    pub col_peaks: Vec<f32>,
    pub col_mins: Vec<f32>,
    pub max_frames: usize,
    pub total_received: u64,
    pub spike_rejected: u64,
    pub measured_hz: f64,
    pub rx_kbps: f64,
    pub source_label: String,
    pub cursor_enabled: bool,
    pub cursor_t1: f64,
    pub cursor_t2: f64,
    pub bookmarks: Vec<BookmarkEvent>,
    pub console_lines: Vec<String>,
    pub slices: Vec<WaveformSlice>,
    // Cached evaluated math expressions (O(delta_N) incremental during streaming/replay!)
    pub active_epoch: u64,
    pub cached_math_seq: (u64, u64),
    pub cached_math_mseq: u64,
    pub cached_math_len: usize,
    pub cached_math: HashMap<String, Vec<f32>>,
    pub cached_math_errors: BTreeMap<String, String>,
    prev_state: Option<i32>,
    prev_fault: Option<u16>,
    fps_counter_frames: u64,
    rx_counter_bytes: u64,
    fps_last_tick: Instant,
    stream_t0: Option<Instant>,
}

impl InnerStore {
    fn new(nominal_hz: f64) -> Self {
        let mut s = Self {
            timestamps: Vec::with_capacity(65536),
            columns: Vec::new(),
            col_peaks: Vec::new(),
            col_mins: Vec::new(),
            max_frames: DEFAULT_MAX_FRAMES,
            total_received: 0,
            spike_rejected: 0,
            measured_hz: nominal_hz,
            rx_kbps: 0.0,
            source_label: "未连接".to_string(),
            cursor_enabled: true,
            cursor_t1: 0.0,
            cursor_t2: 1.0,
            bookmarks: Vec::new(),
            console_lines: Vec::new(),
            slices: Vec::new(),
            active_epoch: 0,
            cached_math_seq: (u64::MAX, u64::MAX),
            cached_math_mseq: u64::MAX,
            cached_math_len: 0,
            cached_math: HashMap::new(),
            cached_math_errors: BTreeMap::new(),
            prev_state: None,
            prev_fault: None,
            fps_counter_frames: 0,
            rx_counter_bytes: 0,
            fps_last_tick: Instant::now(),
            stream_t0: None,
        };
        s.reload_slices_from_disk();
        s
    }

    pub fn reload_slices_from_disk(&mut self) {
        self.slices.clear();
        let dir = slices_dir();
        if let Ok(rd) = fs::read_dir(dir) {
            let mut paths: Vec<PathBuf> = rd
                .filter_map(|e| e.ok().map(|x| x.path()))
                .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("fslice"))
                .collect();
            paths.sort();
            for p in paths {
                if let Ok(sl) = WaveformSlice::load_from_disk(&p) {
                    self.slices.push(sl);
                }
            }
        }
    }

    pub fn clear_waveforms(&mut self) {
        self.timestamps.clear();
        for c in &mut self.columns {
            c.clear();
        }
        self.col_peaks.clear();
        self.col_mins.clear();
        self.total_received = 0;
        self.spike_rejected = 0;
        self.rx_kbps = 0.0;
        self.rx_counter_bytes = 0;
        self.bookmarks.clear();
        self.cached_math.clear();
        self.cached_math_seq = (u64::MAX, u64::MAX);
        self.cached_math_mseq = u64::MAX;
        self.cached_math_len = 0;
        self.prev_state = None;
        self.prev_fault = None;
        self.stream_t0 = None;
    }

    fn ensure_cols(&mut self, n_cols: usize) {
        let cur_len = self.timestamps.len();
        while self.columns.len() < n_cols {
            self.columns.push(vec![0.0f32; cur_len]);
        }
        while self.col_peaks.len() < n_cols {
            self.col_peaks.push(f32::NAN);
        }
        while self.col_mins.len() < n_cols {
            self.col_mins.push(f32::INFINITY);
        }
    }

    fn enforce_ring_cap(&mut self) {
        if self.timestamps.len() > self.max_frames + 8192 {
            let drop_n = self.timestamps.len() - self.max_frames;
            self.timestamps.drain(0..drop_n);
            for col in &mut self.columns {
                if col.len() >= drop_n {
                    col.drain(0..drop_n);
                }
            }
            for vec in self.cached_math.values_mut() {
                if vec.len() >= drop_n {
                    vec.drain(0..drop_n);
                } else {
                    vec.clear();
                }
            }
            self.cached_math_len = self.cached_math_len.saturating_sub(drop_n);
        }
    }
}

#[derive(Clone)]
struct CachedReplayCsv {
    path: PathBuf,
    fname: String,
    ts_norm: Vec<f64>,
    rows: Vec<Vec<f32>>,
    base_hz: f64,
}

struct HighResTimerGuard;
impl HighResTimerGuard {
    fn acquire() -> Self {
        #[cfg(target_os = "windows")]
        unsafe {
            #[link(name = "winmm")]
            extern "system" {
                fn timeBeginPeriod(uPeriod: u32) -> u32;
            }
            timeBeginPeriod(1);
        }
        Self
    }
}
impl Drop for HighResTimerGuard {
    fn drop(&mut self) {
        #[cfg(target_os = "windows")]
        unsafe {
            #[link(name = "winmm")]
            extern "system" {
                fn timeEndPeriod(uPeriod: u32) -> u32;
            }
            timeEndPeriod(1);
        }
    }
}

pub struct TelemetryEngine {
    pub profile: RwLock<StudioProfile>,
    pub store: RwLock<InnerStore>,
    pub data_seq: AtomicU64,
    pub math_seq: AtomicU64,
    pub stream_epoch: AtomicU64,
    pub paused: AtomicBool,
    pub is_streaming: AtomicBool,
    pub replay_speed_bits: AtomicU64,
    pub replay_playhead_bits: AtomicU64,
    replay_meta: Mutex<Option<(String, f64)>>,
    replay_cache: Mutex<Option<CachedReplayCsv>>,
    stop_flag: Mutex<Option<Arc<AtomicBool>>>,
    worker_handle: Mutex<Option<thread::JoinHandle<()>>>,
    active_tx: Mutex<Option<ActiveTx>>,
}

impl TelemetryEngine {
    pub fn new(profile: StudioProfile) -> Arc<Self> {
        let hz = profile.nominal_sample_hz;
        let spd = if profile.replay_speed.is_finite() && profile.replay_speed > 0.0 {
            profile.replay_speed.clamp(0.01, 100.0)
        } else {
            1.0
        };
        Arc::new(Self {
            profile: RwLock::new(profile),
            store: RwLock::new(InnerStore::new(hz)),
            data_seq: AtomicU64::new(1),
            math_seq: AtomicU64::new(1),
            stream_epoch: AtomicU64::new(1),
            paused: AtomicBool::new(false),
            is_streaming: AtomicBool::new(false),
            replay_speed_bits: AtomicU64::new(spd.to_bits()),
            replay_playhead_bits: AtomicU64::new(f64::NAN.to_bits()),
            replay_meta: Mutex::new(None),
            replay_cache: Mutex::new(None),
            stop_flag: Mutex::new(None),
            worker_handle: Mutex::new(None),
            active_tx: Mutex::new(None),
        })
    }

    pub fn bump_math_seq(&self) {
        self.math_seq.fetch_add(1, Ordering::Relaxed);
    }

    pub fn bump_data_seq(&self) {
        self.data_seq.fetch_add(1, Ordering::Relaxed);
    }

    pub fn list_serial_ports() -> Vec<BTreeMap<String, String>> {
        let mut out = Vec::new();
        if let Ok(ports) = serialport::available_ports() {
            for p in ports {
                let mut m = BTreeMap::new();
                let desc = match &p.port_type {
                    serialport::SerialPortType::UsbPort(info) => {
                        info.product.clone().unwrap_or_else(|| "USB Serial".to_string())
                    }
                    serialport::SerialPortType::BluetoothPort => "Bluetooth Serial".to_string(),
                    serialport::SerialPortType::PciPort => "PCI Serial".to_string(),
                    serialport::SerialPortType::Unknown => "Standard COM".to_string(),
                };
                m.insert("port".to_string(), p.port_name);
                m.insert("description".to_string(), desc);
                out.push(m);
            }
        }
        out
    }

    pub fn stop_streams(&self) {
        let ep = self.stream_epoch.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut g) = self.stop_flag.lock() {
            if let Some(flag) = g.take() {
                flag.store(true, Ordering::SeqCst);
            }
        }
        if let Ok(mut tx) = self.active_tx.lock() {
            *tx = None;
        }
        if let Ok(mut wh) = self.worker_handle.lock() {
            if let Some(handle) = wh.take() {
                if handle.thread().id() != thread::current().id() {
                    let _ = handle.join();
                }
            }
        }
        if let Ok(mut rm) = self.replay_meta.lock() {
            *rm = None;
        }
        self.replay_playhead_bits
            .store(f64::NAN.to_bits(), Ordering::SeqCst);
        self.is_streaming.store(false, Ordering::SeqCst);
        if let Ok(mut st) = self.store.write() {
            st.active_epoch = ep;
            st.rx_kbps = 0.0;
        }
        self.bump_data_seq();
    }

    fn begin_stream_session(&self, label: &str, measured_hz: Option<f64>) -> (u64, Arc<AtomicBool>) {
        let my_ep = self.stream_epoch.fetch_add(1, Ordering::SeqCst) + 1;
        {
            let mut st = self.store.write().unwrap();
            st.clear_waveforms();
            st.active_epoch = my_ep;
            st.source_label = label.to_string();
            if let Some(hz) = measured_hz {
                st.measured_hz = hz;
            }
        }
        let stop = Arc::new(AtomicBool::new(false));
        *self.stop_flag.lock().unwrap() = Some(stop.clone());
        self.paused.store(false, Ordering::SeqCst);
        self.is_streaming.store(true, Ordering::SeqCst);
        self.bump_data_seq();
        (my_ep, stop)
    }

    fn record_raw_rx_bytes(&self, n_bytes: usize) {
        if n_bytes == 0 {
            return;
        }
        let now = Instant::now();
        let mut st = self.store.write().unwrap();
        st.rx_counter_bytes += n_bytes as u64;
        let dt = now.duration_since(st.fps_last_tick).as_secs_f64();
        if dt >= 0.5 {
            st.measured_hz = (st.fps_counter_frames as f64 / dt).round();
            st.rx_kbps = ((st.rx_counter_bytes as f64 / 1024.0) / dt * 10.0).round() / 10.0;
            st.fps_counter_frames = 0;
            st.rx_counter_bytes = 0;
            st.fps_last_tick = now;
        }
    }

    pub fn start_serial(self: &Arc<Self>, port_name: &str, baudrate: u32) -> Result<String, String> {
        self.stop_streams();
        let (conn_cfg, hint_ch) = {
            let mut prof = self.profile.write().unwrap();
            prof.baudrate = baudrate;
            prof.connection.baudrate = baudrate;
            prof.connection.last_port = port_name.to_string();
            prof.connection.interface_type = "serial".to_string();
            let _ = prof.save(None);
            (prof.connection.clone(), Some(prof.channels.len()))
        };

        let data_bits = match conn_cfg.data_bits {
            5 => serialport::DataBits::Five,
            6 => serialport::DataBits::Six,
            7 => serialport::DataBits::Seven,
            _ => serialport::DataBits::Eight,
        };
        let stop_bits = match conn_cfg.stop_bits {
            2 => serialport::StopBits::Two,
            _ => serialport::StopBits::One,
        };
        let parity = match conn_cfg.parity.as_str() {
            "even" => serialport::Parity::Even,
            "odd" => serialport::Parity::Odd,
            _ => serialport::Parity::None,
        };
        let flow = match conn_cfg.flow_control.as_str() {
            "hardware" => serialport::FlowControl::Hardware,
            "software" => serialport::FlowControl::Software,
            _ => serialport::FlowControl::None,
        };

        let mut port = serialport::new(port_name, baudrate)
            .data_bits(data_bits)
            .stop_bits(stop_bits)
            .parity(parity)
            .flow_control(flow)
            .timeout(Duration::from_millis(15))
            .open()
            .map_err(|e| format!("无法打开串口 {} @ {}: {}", port_name, baudrate, e))?;

        let _ = port.write_data_terminal_ready(conn_cfg.dtr);
        let _ = port.write_request_to_send(conn_cfg.rts);

        let tx_clone = port.try_clone().ok();
        if let Ok(mut tx_g) = self.active_tx.lock() {
            *tx_g = tx_clone.map(ActiveTx::Serial);
        }

        let proto_tag = match conn_cfg.protocol.as_str() {
            "firewater" => "FireWater",
            "rawdata" => "RawData",
            _ => "JustFloat",
        };
        let label = format!("串口实时: {} @ {} ({})", port_name, baudrate, proto_tag);
        let (my_ep, stop) = self.begin_stream_session(&label, None);

        let eng = Arc::clone(self);
        let handle = thread::spawn(move || {
            let mut reader = port;
            let mut parser = StreamProtocolParser::from_config(
                &conn_cfg.protocol,
                hint_ch,
                conn_cfg.raw_hex_display,
            );
            let mut buf = [0u8; 8192];
            while !stop.load(Ordering::Relaxed) {
                match reader.read(&mut buf) {
                    Ok(n) if n > 0 => {
                        eng.record_raw_rx_bytes(n);
                        let (frames, ascii) = parser.feed(&buf[..n]);
                        if !frames.is_empty() && !eng.paused.load(Ordering::Relaxed) {
                            eng.ingest_frames_internal(&frames, None, None, Some(my_ep));
                        }
                        if !ascii.is_empty() {
                            eng.append_console_lines(&ascii);
                        }
                    }
                    Ok(_) => {}
                    Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(e) => {
                        eng.append_console_lines(&[format!("[系统] 串口断开: {}", e)]);
                        break;
                    }
                }
            }
            if eng.stream_epoch.load(Ordering::SeqCst) == my_ep {
                eng.is_streaming.store(false, Ordering::SeqCst);
                eng.bump_data_seq();
            }
        });
        *self.worker_handle.lock().unwrap() = Some(handle);

        Ok(label)
    }

    pub fn start_udp(self: &Arc<Self>, bind_addr: &str) -> Result<String, String> {
        self.stop_streams();
        let (conn_cfg, hint_ch) = {
            let mut prof = self.profile.write().unwrap();
            prof.connection.interface_type = "udp".to_string();
            prof.connection.udp_bind_addr = bind_addr.to_string();
            let _ = prof.save(None);
            (prof.connection.clone(), Some(prof.channels.len()))
        };

        let sock = std::net::UdpSocket::bind(bind_addr)
            .map_err(|e| format!("无法绑定 UDP 地址 {}: {}", bind_addr, e))?;
        let _ = sock.set_read_timeout(Some(Duration::from_millis(25)));
        let peer_holder = Arc::new(Mutex::new(None));
        if let Ok(tx_sock) = sock.try_clone() {
            if let Ok(mut tx_g) = self.active_tx.lock() {
                *tx_g = Some(ActiveTx::Udp(tx_sock, Arc::clone(&peer_holder)));
            }
        }

        let proto_tag = match conn_cfg.protocol.as_str() {
            "firewater" => "FireWater",
            "rawdata" => "RawData",
            _ => "JustFloat",
        };
        let label = format!("UDP监听: {} ({})", bind_addr, proto_tag);
        let (my_ep, stop) = self.begin_stream_session(&label, None);

        let eng = Arc::clone(self);
        let handle = thread::spawn(move || {
            let mut parser = StreamProtocolParser::from_config(
                &conn_cfg.protocol,
                hint_ch,
                conn_cfg.raw_hex_display,
            );
            let mut buf = [0u8; 8192];
            while !stop.load(Ordering::Relaxed) {
                match sock.recv_from(&mut buf) {
                    Ok((n, peer)) if n > 0 => {
                        if let Ok(mut p) = peer_holder.lock() {
                            *p = Some(peer);
                        }
                        eng.record_raw_rx_bytes(n);
                        let (frames, ascii) = parser.feed(&buf[..n]);
                        if !frames.is_empty() && !eng.paused.load(Ordering::Relaxed) {
                            eng.ingest_frames_internal(&frames, None, None, Some(my_ep));
                        }
                        if !ascii.is_empty() {
                            eng.append_console_lines(&ascii);
                        }
                    }
                    Ok(_) => {}
                    Err(ref e)
                        if e.kind() == std::io::ErrorKind::TimedOut
                            || e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => {
                        eng.append_console_lines(&[format!("[系统] UDP 监听结束: {}", e)]);
                        break;
                    }
                }
            }
            if eng.stream_epoch.load(Ordering::SeqCst) == my_ep {
                eng.is_streaming.store(false, Ordering::SeqCst);
                eng.bump_data_seq();
            }
        });
        *self.worker_handle.lock().unwrap() = Some(handle);

        Ok(label)
    }

    pub fn start_tcp(self: &Arc<Self>, remote_addr: &str) -> Result<String, String> {
        self.stop_streams();
        let (conn_cfg, hint_ch) = {
            let mut prof = self.profile.write().unwrap();
            prof.connection.interface_type = "tcp".to_string();
            prof.connection.tcp_remote_addr = remote_addr.to_string();
            let _ = prof.save(None);
            (prof.connection.clone(), Some(prof.channels.len()))
        };

        let addr: std::net::SocketAddr = remote_addr
            .parse()
            .map_err(|e| format!("无效的 TCP 地址 '{}': {}", remote_addr, e))?;
        let mut stream = std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(3))
            .map_err(|e| format!("无法连接 TCP 服务端 {}: {}", remote_addr, e))?;
        let _ = stream.set_read_timeout(Some(Duration::from_millis(25)));
        let _ = stream.set_nodelay(true);
        if let Ok(tx_s) = stream.try_clone() {
            if let Ok(mut tx_g) = self.active_tx.lock() {
                *tx_g = Some(ActiveTx::Tcp(tx_s));
            }
        }

        let proto_tag = match conn_cfg.protocol.as_str() {
            "firewater" => "FireWater",
            "rawdata" => "RawData",
            _ => "JustFloat",
        };
        let label = format!("TCP连接: {} ({})", remote_addr, proto_tag);
        let (my_ep, stop) = self.begin_stream_session(&label, None);

        let eng = Arc::clone(self);
        let handle = thread::spawn(move || {
            let mut parser = StreamProtocolParser::from_config(
                &conn_cfg.protocol,
                hint_ch,
                conn_cfg.raw_hex_display,
            );
            let mut buf = [0u8; 8192];
            while !stop.load(Ordering::Relaxed) {
                match stream.read(&mut buf) {
                    Ok(0) => {
                        eng.append_console_lines(&["[系统] 对端已关闭 TCP 连接".to_string()]);
                        break;
                    }
                    Ok(n) => {
                        eng.record_raw_rx_bytes(n);
                        let (frames, ascii) = parser.feed(&buf[..n]);
                        if !frames.is_empty() && !eng.paused.load(Ordering::Relaxed) {
                            eng.ingest_frames_internal(&frames, None, None, Some(my_ep));
                        }
                        if !ascii.is_empty() {
                            eng.append_console_lines(&ascii);
                        }
                    }
                    Err(ref e)
                        if e.kind() == std::io::ErrorKind::TimedOut
                            || e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => {
                        eng.append_console_lines(&[format!("[系统] TCP 连接断开: {}", e)]);
                        break;
                    }
                }
            }
            if eng.stream_epoch.load(Ordering::SeqCst) == my_ep {
                eng.is_streaming.store(false, Ordering::SeqCst);
                eng.bump_data_seq();
            }
        });
        *self.worker_handle.lock().unwrap() = Some(handle);

        Ok(label)
    }

    pub fn send_serial_command(&self, cmd: &str) -> Result<(), String> {
        let mut g = self.active_tx.lock().map_err(|e| e.to_string())?;
        match *g {
            Some(ActiveTx::Serial(ref mut port)) => {
                port.write_all(cmd.as_bytes())
                    .and_then(|_| port.flush())
                    .map_err(|e| format!("串口发送失败: {}", e))?;
            }
            Some(ActiveTx::Tcp(ref mut stream)) => {
                stream
                    .write_all(cmd.as_bytes())
                    .and_then(|_| stream.flush())
                    .map_err(|e| format!("TCP 发送失败: {}", e))?;
            }
            Some(ActiveTx::Udp(ref sock, ref peer_mtx)) => {
                let peer = peer_mtx
                    .lock()
                    .ok()
                    .and_then(|p| *p)
                    .ok_or("尚未收到任何 UDP 远端数据包，暂未知晓下位机目标地址")?;
                sock.send_to(cmd.as_bytes(), peer)
                    .map_err(|e| format!("UDP 发送失败: {}", e))?;
            }
            None => return Err("当前未建立通信连接".to_string()),
        }
        drop(g);
        self.append_console_lines(&[format!("[TX -> MCU] {}", cmd.trim_end())]);
        Ok(())
    }

    pub fn append_console_lines(&self, lines: &[String]) {
        if lines.is_empty() {
            return;
        }
        let mut st = self.store.write().unwrap();
        for l in lines {
            st.console_lines.push(l.clone());
        }
        if st.console_lines.len() > 2000 {
            let drain = st.console_lines.len() - 2000;
            st.console_lines.drain(0..drain);
        }
        drop(st);
        self.bump_data_seq();
    }

    pub fn ingest_live_frames(&self, frames: &[Vec<f32>], fixed_dt: Option<f64>) {
        self.ingest_frames_internal(frames, fixed_dt, None, None);
    }

    fn ingest_frames_internal(
        &self,
        frames: &[Vec<f32>],
        fixed_dt: Option<f64>,
        explicit_ts: Option<&[f64]>,
        expected_epoch: Option<u64>,
    ) {
        if frames.is_empty() {
            return;
        }
        let (filt_on, thresh, sd_cfg) = {
            let mut prof = self.profile.write().unwrap();
            let max_c = frames.iter().map(|r| r.len()).max().unwrap_or(0);
            if prof.ensure_channel_capacity(max_c) {
                let _ = prof.save(None);
            }
            (
                prof.spike_filter_enabled,
                prof.spike_threshold,
                prof.status_decode.clone(),
            )
        };

        let now = Instant::now();
        let mut st = self.store.write().unwrap();
        if let Some(ep) = expected_epoch {
            if st.active_epoch != ep {
                return;
            }
        }
        let t0 = *st.stream_t0.get_or_insert(now);
        let elapsed_now = now.duration_since(t0).as_secs_f64();
        let batch_prev_t = st.timestamps.last().copied();

        let n_in = frames.len();
        for (idx, row) in frames.iter().enumerate() {
            if filt_on && row.iter().any(|&v| !v.is_finite() || v.abs() >= thresh) {
                st.spike_rejected += 1;
                continue;
            }
            st.ensure_cols(row.len());
            let ts = if let Some(&exp_t) = explicit_ts.and_then(|s| s.get(idx)) {
                exp_t
            } else if let Some(dt) = fixed_dt {
                st.timestamps.last().copied().map(|last| last + dt).unwrap_or(0.0)
            } else if let Some(base_t) = batch_prev_t {
                let span = (elapsed_now - base_t).max(0.00002 * n_in as f64);
                base_t + span * ((idx + 1) as f64 / n_in as f64)
            } else if n_in > 1 {
                elapsed_now * (idx as f64 / (n_in - 1).max(1) as f64)
            } else {
                elapsed_now
            };

            if let Some(&prev_t) = st.timestamps.last() {
                if ts < prev_t {
                    continue;
                }
            }

            st.timestamps.push(ts);
            let ncols = st.columns.len();
            for c_i in 0..ncols {
                let v = row.get(c_i).copied().unwrap_or(0.0);
                let vf = if v.is_finite() { v } else { 0.0 };
                st.columns[c_i].push(vf);
                if c_i < st.col_peaks.len() {
                    if !st.col_peaks[c_i].is_finite() || vf.abs() > st.col_peaks[c_i].abs() {
                        st.col_peaks[c_i] = vf;
                    }
                }
                if c_i < st.col_mins.len() && vf < st.col_mins[c_i] {
                    st.col_mins[c_i] = vf;
                }
            }
            st.total_received += 1;
            st.fps_counter_frames += 1;

            if sd_cfg.enabled {
                if sd_cfg.state_enabled {
                    if let Some(ec_idx) = sd_cfg.state_channel {
                        if let Some(&fv) = row.get(ec_idx) {
                            if fv.is_finite() {
                                let sv = fv.round() as i32;
                                if let Some(prev) = st.prev_state {
                                    if prev != sv {
                                        let p_lbl = sd_cfg
                                            .state_enum_map
                                            .get(&prev.to_string())
                                            .map(|e| e.label.clone())
                                            .unwrap_or_else(|| prev.to_string());
                                        let n_lbl = sd_cfg
                                            .state_enum_map
                                            .get(&sv.to_string())
                                            .map(|e| e.label.clone())
                                            .unwrap_or_else(|| sv.to_string());
                                        st.bookmarks.push(BookmarkEvent {
                                            time_s: ts,
                                            kind: "state".to_string(),
                                            text: format!("状态跳变: {} -> {}", p_lbl, n_lbl),
                                        });
                                    }
                                }
                                st.prev_state = Some(sv);
                            }
                        }
                    }
                }
                if sd_cfg.fault_enabled {
                    if let Some(mc_idx) = sd_cfg.fault_channel {
                        if let Some(&fv) = row.get(mc_idx) {
                            if fv.is_finite() {
                                let mv = (fv.round() as i64 & 0xFFFF) as u16;
                                let prev = st.prev_fault.unwrap_or(0);
                                if sd_cfg.fault_mode == "enum" {
                                    if mv != 0 && mv != prev {
                                        let name = sd_cfg
                                            .fault_map
                                            .get(&mv.to_string())
                                            .cloned()
                                            .unwrap_or_else(|| format!("故障码 {}", mv));
                                        st.bookmarks.push(BookmarkEvent {
                                            time_s: ts,
                                            kind: "fault".to_string(),
                                            text: format!("故障触发 ({}): {}", mv, name),
                                        });
                                    }
                                } else {
                                    let newly = mv & !prev;
                                    if newly != 0 {
                                        let names: Vec<String> = (0..16)
                                            .filter(|b| (newly & (1 << b)) != 0)
                                            .map(|b| {
                                                sd_cfg
                                                    .fault_map
                                                    .get(&b.to_string())
                                                    .cloned()
                                                    .unwrap_or_else(|| format!("Bit{}", b))
                                            })
                                            .collect();
                                        st.bookmarks.push(BookmarkEvent {
                                            time_s: ts,
                                            kind: "fault".to_string(),
                                            text: format!(
                                                "故障触发 (0x{:04X}): {}",
                                                mv,
                                                names.join(" | ")
                                            ),
                                        });
                                    }
                                }
                                st.prev_fault = Some(mv);
                            }
                        }
                    }
                }
            }
        }

        let dt_fps = now.duration_since(st.fps_last_tick).as_secs_f64();
        if dt_fps >= 0.5 {
            st.measured_hz = (st.fps_counter_frames as f64 / dt_fps).round();
            if st.rx_counter_bytes > 0 {
                st.rx_kbps = ((st.rx_counter_bytes as f64 / 1024.0) / dt_fps * 10.0).round() / 10.0;
                st.rx_counter_bytes = 0;
            }
            st.fps_counter_frames = 0;
            st.fps_last_tick = now;
        }
        st.enforce_ring_cap();
        drop(st);
        self.bump_data_seq();
    }

    /// Ultra-fast UTF-8 / GBK-safe CSV loader in native Rust (< 25 ms for 50k rows!)
    pub fn load_csv_instant(
        &self,
        path: &Path,
        sample_hz_override: Option<f64>,
        adopt_headers: bool,
    ) -> Result<BTreeMap<String, serde_json::Value>, String> {
        self.stop_streams();
        let raw_bytes =
            fs::read(path).map_err(|e| format!("无法读取 CSV 文件 {:?}: {}", path, e))?;
        // Strip UTF-8 BOM if present
        let bytes = if raw_bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            &raw_bytes[3..]
        } else {
            &raw_bytes[..]
        };

        let mut lines_iter = bytes.split(|&b| b == b'\n');
        let first_line = loop {
            match lines_iter.next() {
                Some(l) => {
                    let trimmed = trim_ascii(l);
                    if !trimmed.is_empty() {
                        break trimmed;
                    }
                }
                None => return Err("CSV 文件为空".to_string()),
            }
        };

        // Determine if first line is header or numeric data
        let first_fields: Vec<&[u8]> = first_line
            .split(|&b| b == b',')
            .map(trim_ascii)
            .filter(|s| !s.is_empty())
            .collect();
        let has_header = first_fields
            .iter()
            .any(|fld| std::str::from_utf8(fld).ok().and_then(|s| s.parse::<f64>().ok()).is_none());

        let headers: Vec<String> = if has_header {
            first_fields
                .iter()
                .map(|b| decode_text_smart(b))
                .collect()
        } else {
            Vec::new()
        };

        let has_time_col = headers.first().map_or(false, |h| {
            matches!(
                h.trim().to_ascii_lowercase().as_str(),
                "time_s" | "time" | "t" | "timestamp"
            )
        });

        let (filt_on, thresh, nominal_hz) = {
            let prof = self.profile.read().unwrap();
            (
                prof.spike_filter_enabled,
                prof.spike_threshold,
                prof.nominal_sample_hz,
            )
        };
        let hz = sample_hz_override.unwrap_or(nominal_hz).max(1.0);
        let dt = 1.0 / hz;

        let mut timestamps = Vec::with_capacity(65536);
        let mut columns: Vec<Vec<f32>> = Vec::new();
        let mut expected_cols: Option<usize> = None;
        let mut rejected = 0u64;

        let mut parse_row = |line_bytes: &[u8]| {
            let trimmed = trim_ascii(line_bytes);
            if trimmed.is_empty() {
                return;
            }
            let mut parts: Vec<&[u8]> = trimmed.split(|&b| b == b',').map(trim_ascii).collect();
            while parts.last().map_or(false, |p| p.is_empty()) {
                parts.pop();
            }
            if parts.is_empty() {
                return;
            }
            let mut nums = Vec::with_capacity(parts.len());
            for p in parts {
                if let Ok(s) = std::str::from_utf8(p) {
                    if let Ok(v) = s.parse::<f64>() {
                        nums.push(v);
                        continue;
                    }
                }
                rejected += 1;
                return;
            }

            let (t_val, vals_slice) = if has_time_col && nums.len() >= 2 {
                (nums[0], &nums[1..])
            } else {
                (timestamps.len() as f64 * dt, &nums[..])
            };

            let n_cols = *expected_cols.get_or_insert(vals_slice.len());
            if columns.is_empty() {
                columns = (0..n_cols).map(|_| Vec::with_capacity(65536)).collect();
            }

            if filt_on
                && vals_slice
                    .iter()
                    .any(|&v| !v.is_finite() || (v as f32).abs() >= thresh)
            {
                rejected += 1;
                return;
            }

            timestamps.push(t_val);
            for c_i in 0..n_cols {
                let fv = vals_slice.get(c_i).copied().unwrap_or(0.0) as f32;
                columns[c_i].push(if fv.is_finite() { fv } else { 0.0 });
            }
        };

        if !has_header {
            parse_row(first_line);
        }
        for line_b in lines_iter {
            parse_row(line_b);
        }

        if timestamps.is_empty() {
            return Err("CSV 中未解析到有效数值行".to_string());
        }

        // Trim to latest max_frames if huge
        if timestamps.len() > DEFAULT_MAX_FRAMES {
            let drop_n = timestamps.len() - DEFAULT_MAX_FRAMES;
            timestamps.drain(0..drop_n);
            let t_zero = timestamps[0];
            for t in &mut timestamps {
                *t -= t_zero;
            }
            for col in &mut columns {
                col.drain(0..drop_n);
            }
        }

        let n_rows = timestamps.len();
        let n_cols = columns.len();

        // Update profile capacity & optional headers
        let sd_cfg = {
            let mut prof = self.profile.write().unwrap();
            prof.ensure_channel_capacity(n_cols);
            if adopt_headers && !headers.is_empty() {
                let ch_hdrs = if has_time_col { &headers[1..] } else { &headers[..] };
                for (i, h) in ch_hdrs.iter().enumerate() {
                    if i < prof.channels.len() && !h.trim().is_empty() {
                        prof.channels[i].name = h.trim().to_string();
                    }
                }
            }
            let _ = prof.save(None);
            prof.status_decode.clone()
        };

        // Vectorized scan for state & fault bookmarks across loaded CSV
        let mut bookmarks = Vec::new();
        if sd_cfg.enabled {
            if sd_cfg.state_enabled {
                if let Some(ec_idx) = sd_cfg.state_channel {
                    if ec_idx < n_cols {
                        let col = &columns[ec_idx];
                        for i in 1..n_rows {
                            let prev = col[i - 1].round() as i32;
                            let cur = col[i].round() as i32;
                            if prev != cur {
                                let p_lbl = sd_cfg
                                    .state_enum_map
                                    .get(&prev.to_string())
                                    .map(|e| e.label.clone())
                                    .unwrap_or_else(|| prev.to_string());
                                let c_lbl = sd_cfg
                                    .state_enum_map
                                    .get(&cur.to_string())
                                    .map(|e| e.label.clone())
                                    .unwrap_or_else(|| cur.to_string());
                                bookmarks.push(BookmarkEvent {
                                    time_s: timestamps[i],
                                    kind: "state".to_string(),
                                    text: format!("状态跳变: {} -> {}", p_lbl, c_lbl),
                                });
                            }
                        }
                    }
                }
            }
            if sd_cfg.fault_enabled {
                if let Some(mc_idx) = sd_cfg.fault_channel {
                    if mc_idx < n_cols {
                        let col = &columns[mc_idx];
                        for i in 1..n_rows {
                            let prev = (col[i - 1].round() as i64 & 0xFFFF) as u16;
                            let cur = (col[i].round() as i64 & 0xFFFF) as u16;
                            if sd_cfg.fault_mode == "enum" {
                                if cur != 0 && cur != prev {
                                    let name = sd_cfg
                                        .fault_map
                                        .get(&cur.to_string())
                                        .cloned()
                                        .unwrap_or_else(|| format!("故障码 {}", cur));
                                    bookmarks.push(BookmarkEvent {
                                        time_s: timestamps[i],
                                        kind: "fault".to_string(),
                                        text: format!("故障触发 ({}): {}", cur, name),
                                    });
                                }
                            } else {
                                let newly = cur & !prev;
                                if newly != 0 {
                                    let names: Vec<String> = (0..16)
                                        .filter(|b| (newly & (1 << b)) != 0)
                                        .map(|b| {
                                            sd_cfg
                                                .fault_map
                                                .get(&b.to_string())
                                                .cloned()
                                                .unwrap_or_else(|| format!("Bit{}", b))
                                        })
                                        .collect();
                                    bookmarks.push(BookmarkEvent {
                                        time_s: timestamps[i],
                                        kind: "fault".to_string(),
                                        text: format!(
                                            "故障触发 (0x{:04X}): {}",
                                            cur,
                                            names.join(" | ")
                                        ),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        bookmarks.sort_by(|a, b| a.time_s.partial_cmp(&b.time_s).unwrap_or(std::cmp::Ordering::Equal));

        let t_first = timestamps[0];
        let t_last = *timestamps.last().unwrap_or(&0.0);
        let est_hz = if n_rows > 2 && (t_last - t_first) > 1e-6 {
            ((n_rows - 1) as f64 / (t_last - t_first)).round()
        } else {
            hz
        };

        let fname = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("data.csv")
            .to_string();

        // Cache parsed rows in memory for instant 1-click replay / restart from 0s, T1, or T2
        {
            let c = columns.len();
            let ts_norm: Vec<f64> = timestamps.iter().map(|&t| (t - t_first).max(0.0)).collect();
            let mut r = Vec::with_capacity(n_rows);
            for i in 0..n_rows {
                let mut row = Vec::with_capacity(c);
                for col in &columns {
                    row.push(col[i]);
                }
                r.push(row);
            }
            if let Ok(mut rc) = self.replay_cache.lock() {
                *rc = Some(CachedReplayCsv {
                    path: path.to_path_buf(),
                    fname: fname.clone(),
                    ts_norm,
                    rows: r,
                    base_hz: est_hz.max(1.0),
                });
            }
        }

        {
            let mut peaks = Vec::with_capacity(columns.len());
            let mut mins = Vec::with_capacity(columns.len());
            for col in &columns {
                let mut pk = f32::NAN;
                let mut mn = f32::INFINITY;
                for &v in col {
                    if v.is_finite() {
                        if !pk.is_finite() || v.abs() > pk.abs() {
                            pk = v;
                        }
                        if v < mn {
                            mn = v;
                        }
                    }
                }
                peaks.push(pk);
                mins.push(mn);
            }
            let mut st = self.store.write().unwrap();
            st.clear_waveforms();
            st.timestamps = timestamps;
            st.columns = columns;
            st.col_peaks = peaks;
            st.col_mins = mins;
            st.total_received = n_rows as u64;
            st.spike_rejected = rejected;
            st.measured_hz = est_hz;
            st.source_label = format!("CSV: {}", fname);
            st.bookmarks = bookmarks;
            st.cursor_enabled = true;
            st.cursor_t1 = t_first + (t_last - t_first) * 0.24;
            st.cursor_t2 = t_first + (t_last - t_first) * 0.42;
        }
        self.paused.store(false, Ordering::Relaxed);
        self.bump_data_seq();

        let mut info = BTreeMap::new();
        info.insert("file".to_string(), serde_json::json!(path.to_string_lossy()));
        info.insert("frames_loaded".to_string(), serde_json::json!(n_rows));
        info.insert("channels".to_string(), serde_json::json!(n_cols));
        info.insert(
            "duration_s".to_string(),
            serde_json::json!(((t_last - t_first) * 1000.0).round() / 1000.0),
        );
        info.insert("sample_rate_hz".to_string(), serde_json::json!(est_hz));
        info.insert("bookmarks_detected".to_string(), serde_json::json!(self.store.read().unwrap().bookmarks.len()));
        Ok(info)
    }

    pub fn set_replay_speed(&self, speed_mult: f64) -> f64 {
        let spd = if speed_mult.is_finite() && speed_mult > 0.0 {
            speed_mult.clamp(0.01, 100.0)
        } else {
            1.0
        };
        self.replay_speed_bits.store(spd.to_bits(), Ordering::Relaxed);
        if let Ok(mut prof) = self.profile.write() {
            prof.replay_speed = spd;
            let _ = prof.save(None);
        }
        if let Ok(rm) = self.replay_meta.lock() {
            if let Some((ref fname, base_hz)) = *rm {
                if self.is_streaming.load(Ordering::Relaxed) {
                    if let Ok(mut st) = self.store.write() {
                        st.source_label = format!("流回放: {} ({}x)", fname, fmt_speed_mult(spd));
                        st.measured_hz = (base_hz * spd).round();
                    }
                    self.bump_data_seq();
                }
            }
        }
        spd
    }

    pub fn start_csv_replay(self: &Arc<Self>, path: &Path, speed_mult: f64) -> Result<String, String> {
        let mode = self.profile.read().unwrap().replay_start_mode.clone();
        let res = self.start_csv_replay_ex(Some(path), speed_mult, Some(&mode))?;
        Ok(res
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("流回放")
            .to_string())
    }

    pub fn start_csv_replay_ex(
        self: &Arc<Self>,
        path: Option<&Path>,
        speed_mult: f64,
        start_mode: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        let spd_init = self.set_replay_speed(speed_mult);
        let eff_mode = start_mode
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.profile.read().unwrap().replay_start_mode.clone());
        if let Ok(mut prof) = self.profile.write() {
            prof.replay_start_mode = eff_mode.clone();
            let _ = prof.save(None);
        }

        // Reuse in-memory cached CSV if restarting or if same file was already loaded via 打开CSV!
        let cached: CachedReplayCsv = {
            let existing = self.replay_cache.lock().unwrap().clone();
            match (path, existing) {
                (None, Some(c)) => c,
                (Some(req_p), Some(c)) if c.path == req_p => c,
                (Some(req_p), _) => {
                    let prof_clone = self.profile.read().unwrap().clone();
                    let tmp = TelemetryEngine::new(prof_clone);
                    tmp.load_csv_instant(req_p, None, false)?;
                    let c = tmp
                        .replay_cache
                        .lock()
                        .unwrap()
                        .clone()
                        .ok_or("解析 CSV 缓存失败")?;
                    *self.replay_cache.lock().unwrap() = Some(c.clone());
                    c
                }
                (None, None) => return Err("尚未打开任何 CSV 文件，请先选择 CSV".to_string()),
            }
        };

        let (cur_t1, cur_t2) = {
            let st = self.store.read().unwrap();
            (st.cursor_t1, st.cursor_t2)
        };

        let max_csv_t = cached.ts_norm.last().copied().unwrap_or(0.0);
        let req_start_t = match eff_mode.as_str() {
            "t1" => cur_t1.max(0.0),
            "t2" => cur_t2.max(0.0),
            _ => 0.0,
        };
        let (actual_start_t, fallback_zero) = if req_start_t >= max_csv_t && max_csv_t > 0.0 {
            (0.0, true)
        } else {
            (req_start_t, false)
        };

        let start_idx = cached
            .ts_norm
            .partition_point(|&t| t < actual_start_t)
            .min(cached.rows.len().saturating_sub(1));

        self.stop_streams();
        let label = format!("流回放: {} ({}x)", cached.fname, fmt_speed_mult(spd_init));
        if let Ok(mut rm) = self.replay_meta.lock() {
            *rm = Some((cached.fname.clone(), cached.base_hz));
        }
        let (my_ep, stop) =
            self.begin_stream_session(&label, Some((cached.base_hz * spd_init).round()));

        let csv_ts = cached.ts_norm;
        let rows = cached.rows;

        // Synchronously emit initial frame at start_idx BEFORE thread spawns so buffer is clean & deterministic
        let mut init_idx = start_idx;
        let mut init_cursor = actual_start_t;
        if init_idx < rows.len() {
            init_cursor = init_cursor.max(csv_ts[init_idx]);
            self.replay_playhead_bits
                .store(init_cursor.to_bits(), Ordering::SeqCst);
            self.ingest_frames_internal(
                &rows[init_idx..init_idx + 1],
                None,
                Some(&csv_ts[init_idx..init_idx + 1]),
                Some(my_ep),
            );
            init_idx += 1;
        } else {
            self.replay_playhead_bits
                .store(actual_start_t.to_bits(), Ordering::SeqCst);
        }

        let eng = Arc::clone(self);
        let handle = thread::spawn(move || {
            let _timer_guard = HighResTimerGuard::acquire();
            let mut idx = init_idx;
            let mut sim_cursor = init_cursor;
            let mut last_tick = Instant::now();

            while !stop.load(Ordering::Relaxed) && idx < rows.len() {
                thread::sleep(Duration::from_millis(4));
                if stop.load(Ordering::Relaxed) || eng.stream_epoch.load(Ordering::SeqCst) != my_ep {
                    break;
                }
                let now = Instant::now();
                let wall_dt = now.duration_since(last_tick).as_secs_f64();
                last_tick = now;

                if eng.paused.load(Ordering::Relaxed) {
                    continue;
                }

                let cur_spd =
                    f64::from_bits(eng.replay_speed_bits.load(Ordering::Relaxed)).clamp(0.01, 100.0);
                sim_cursor += wall_dt * cur_spd;
                eng.replay_playhead_bits
                    .store(sim_cursor.to_bits(), Ordering::Relaxed);

                let end = csv_ts[idx..].partition_point(|&t| t <= sim_cursor) + idx;
                if end > idx {
                    eng.ingest_frames_internal(
                        &rows[idx..end],
                        None,
                        Some(&csv_ts[idx..end]),
                        Some(my_ep),
                    );
                    idx = end;
                } else {
                    eng.bump_data_seq();
                }
            }
            if eng.stream_epoch.load(Ordering::SeqCst) == my_ep {
                if let Ok(mut rm) = eng.replay_meta.lock() {
                    *rm = None;
                }
                eng.replay_playhead_bits
                    .store(f64::NAN.to_bits(), Ordering::SeqCst);
                eng.is_streaming.store(false, Ordering::SeqCst);
                eng.bump_data_seq();
            }
        });
        *self.worker_handle.lock().unwrap() = Some(handle);

        Ok(serde_json::json!({
            "status": "replaying",
            "source": label,
            "start_time_s": (actual_start_t * 1000.0).round() / 1000.0,
            "fallback_zero": fallback_zero,
        }))
    }

    /// O(delta_N) incremental derived math cache during live streaming / replay!
    fn ensure_math_cache(&self) {
        let d_seq = self.data_seq.load(Ordering::Relaxed);
        let m_seq = self.math_seq.load(Ordering::Relaxed);
        {
            let st = self.store.read().unwrap();
            if st.cached_math_seq == (d_seq, m_seq) {
                return;
            }
        }

        let prof = self.profile.read().unwrap().clone();
        let mut st = self.store.write().unwrap();
        if st.cached_math_seq == (d_seq, m_seq) {
            return;
        }
        let len = st.timestamps.len();
        if len == 0 || prof.math_channels.is_empty() {
            st.cached_math.clear();
            st.cached_math_errors.clear();
            st.cached_math_len = len;
            st.cached_math_mseq = m_seq;
            st.cached_math_seq = (d_seq, m_seq);
            return;
        }

        let can_incremental = st.cached_math_mseq == m_seq
            && st.cached_math_len > 0
            && st.cached_math_len <= len
            && prof
                .math_channels
                .iter()
                .all(|mc| st.cached_math.get(&mc.id).map(|v| v.len() == st.cached_math_len).unwrap_or(false));

        if can_incremental {
            if st.cached_math_len == len {
                st.cached_math_seq = (d_seq, m_seq);
                return;
            }
            let overlap = 256usize.min(st.cached_math_len);
            let s0 = st.cached_math_len - overlap;
            let sub_len = len - s0;

            let mut env: HashMap<String, Vec<f32>> = HashMap::new();
            env.insert(
                "t".to_string(),
                st.timestamps[s0..len].iter().map(|&x| x as f32).collect(),
            );
            for (i, col) in st.columns.iter().enumerate() {
                env.insert(format!("CH{}", i), col[s0..len].to_vec());
            }
            for ch in &prof.channels {
                if ch.index < st.columns.len() && !ch.alias.is_empty() {
                    env.insert(ch.alias.clone(), st.columns[ch.index][s0..len].to_vec());
                }
            }

            for mc in &prof.math_channels {
                match compile_expression(&mc.expression)
                    .and_then(|ast| eval_vectorized(&ast, sub_len, &env))
                {
                    Ok(mut vec) => {
                        for x in &mut vec {
                            if !x.is_finite() {
                                *x = 0.0;
                            }
                        }
                        if !mc.alias.is_empty() {
                            env.insert(mc.alias.clone(), vec.clone());
                        }
                        env.insert(mc.id.clone(), vec.clone());
                        if let Some(existing) = st.cached_math.get_mut(&mc.id) {
                            existing.extend_from_slice(&vec[overlap..]);
                        }
                    }
                    Err(e) => {
                        st.cached_math_errors.insert(mc.id.clone(), e);
                        if let Some(existing) = st.cached_math.get_mut(&mc.id) {
                            existing.resize(len, 0.0);
                        }
                    }
                }
            }
            st.cached_math_len = len;
            st.cached_math_seq = (d_seq, m_seq);
            return;
        }

        // Full evaluation (initial load or formula edited — clone only referenced series!)
        st.cached_math.clear();
        st.cached_math_errors.clear();
        let all_exprs: String = prof
            .math_channels
            .iter()
            .map(|m| m.expression.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let mut env: HashMap<String, Vec<f32>> = HashMap::new();
        if all_exprs.contains('t') {
            env.insert(
                "t".to_string(),
                st.timestamps.iter().map(|&x| x as f32).collect(),
            );
        }
        for (i, col) in st.columns.iter().enumerate() {
            let ck = format!("CH{}", i);
            if all_exprs.contains(&ck) {
                env.insert(ck, col.clone());
            }
        }
        for ch in &prof.channels {
            if ch.index < st.columns.len() && !ch.alias.is_empty() && all_exprs.contains(&ch.alias) {
                env.insert(ch.alias.clone(), st.columns[ch.index].clone());
            }
        }

        for mc in &prof.math_channels {
            match compile_expression(&mc.expression)
                .and_then(|ast| eval_vectorized(&ast, len, &env))
            {
                Ok(mut vec) => {
                    for x in &mut vec {
                        if !x.is_finite() {
                            *x = 0.0;
                        }
                    }
                    if !mc.alias.is_empty() {
                        env.insert(mc.alias.clone(), vec.clone());
                    }
                    env.insert(mc.id.clone(), vec.clone());
                    st.cached_math.insert(mc.id.clone(), vec);
                }
                Err(e) => {
                    st.cached_math_errors.insert(mc.id.clone(), e);
                    st.cached_math.insert(mc.id.clone(), vec![0.0; len]);
                }
            }
        }
        st.cached_math_len = len;
        st.cached_math_mseq = m_seq;
        st.cached_math_seq = (d_seq, m_seq);
    }

    /// High-speed viewport query with pixel-bucket Min-Max envelope decimation (< 1 ms for 500k points!)
    pub fn query_viewport(
        &self,
        t_min_req: Option<f64>,
        t_max_req: Option<f64>,
        pixel_width: usize,
        follow_tail_sec: Option<f64>,
        follow_anchor_ratio: Option<f64>,
    ) -> ViewportPayload {
        self.ensure_math_cache();
        let prof = self.profile.read().unwrap();
        let st = self.store.read().unwrap();
        let d_seq = self.data_seq.load(Ordering::Relaxed);
        let n = st.timestamps.len();

        if n == 0 {
            let (empty_min, empty_max) = if let Some(left_sec) = follow_tail_sec {
                let anchor = follow_anchor_ratio.unwrap_or(1.0).clamp(0.10, 1.00);
                let left_w = left_sec.max(0.0005);
                let total_w = left_w / anchor;
                (0.0, total_w)
            } else {
                (0.0, 10.0)
            };
            return ViewportPayload {
                data_seq: d_seq,
                source_label: st.source_label.clone(),
                protocol: prof.connection.protocol.clone(),
                interface_type: prof.connection.interface_type.clone(),
                is_streaming: self.is_streaming.load(Ordering::Relaxed),
                paused: self.paused.load(Ordering::Relaxed),
                buffered_frames: 0,
                sample_rate_hz: st.measured_hz,
                rx_kbps: st.rx_kbps,
                full_t_min: 0.0,
                full_t_max: empty_max,
                view_t_min: empty_min,
                view_t_max: empty_max,
                cursor_enabled: st.cursor_enabled,
                cursor_t1: st.cursor_t1,
                cursor_t2: st.cursor_t2,
                sample_time: 0.0,
                spike_rejected: st.spike_rejected,
                status_decode_enabled: prof.status_decode.enabled,
                state_enabled: prof.status_decode.state_enabled,
                state_label: "IDLE 待机".to_string(),
                state_color: "#71717A".to_string(),
                fault_enabled: prof.status_decode.fault_enabled,
                fault_code: 0,
                active_faults: Vec::new(),
                isr_enabled: prof.status_decode.isr_enabled,
                isr_label: prof.status_decode.isr_label.clone(),
                isr_full_scale: prof.status_decode.isr_full_scale.max(1.0),
                isr_unit: prof.status_decode.isr_unit.clone(),
                isr_cycles: 0,
                cards: Vec::new(),
                channel_vals: BTreeMap::new(),
                channel_peaks: BTreeMap::new(),
                channel_mins: BTreeMap::new(),
                series_tails: BTreeMap::new(),
                traces: Vec::new(),
                xy_x: Vec::new(),
                xy_y: Vec::new(),
                math_errors: st.cached_math_errors.clone(),
                bookmark_count: st.bookmarks.len(),
                console_count: st.console_lines.len(),
            };
        }

        let full_t_min = st.timestamps[0];
        let full_t_max = st.timestamps[n - 1];

        let (view_t_min, view_t_max) = if let Some(win_sec) = follow_tail_sec {
            let playhead = f64::from_bits(self.replay_playhead_bits.load(Ordering::Relaxed));
            let head_t = if playhead.is_finite() && self.is_streaming.load(Ordering::Relaxed) {
                playhead.max(full_t_max)
            } else {
                full_t_max
            };
            let anchor = follow_anchor_ratio.unwrap_or(1.0).clamp(0.10, 1.00);
            let left_w = win_sec.max(0.0005);
            let total_w = left_w / anchor;
            let vmin = head_t - left_w;
            (vmin, vmin + total_w)
        } else {
            let lo = t_min_req.unwrap_or(full_t_min);
            let hi = t_max_req.unwrap_or(full_t_max);
            if hi <= lo {
                (full_t_min, full_t_max.max(full_t_min + 0.01))
            } else {
                (lo, hi)
            }
        };

        // O(log N) binary search for viewport slice
        let i0 = st.timestamps.partition_point(|&t| t < view_t_min).saturating_sub(1);
        let i1 = (st.timestamps.partition_point(|&t| t <= view_t_max) + 1).min(n);
        let buckets = ((pixel_width * 2) / 3).clamp(240, 900);

        let mut traces = Vec::new();
        for ch in &prof.channels {
            if ch.visible && ch.plot_id != "none" && ch.index < st.columns.len() {
                let (dt, dy) =
                    decimate_minmax(&st.timestamps[i0..i1], &st.columns[ch.index][i0..i1], buckets);
                let label = if ch.unit.is_empty() {
                    ch.name.clone()
                } else {
                    format!("{} ({})", ch.name, ch.unit)
                };
                let g = if ch.display_gain.is_finite() {
                    ch.display_gain
                } else {
                    1.0
                };
                traces.push(DecimatedTrace {
                    key: format!("CH{}", ch.index),
                    plot_id: ch.plot_id.clone(),
                    label,
                    color: ch.color.clone(),
                    width: ch.width,
                    gain: g,
                    t: dt,
                    y: dy,
                });
            }
        }

        for mc in &prof.math_channels {
            if mc.visible && mc.plot_id != "none" {
                if let Some(col) = st.cached_math.get(&mc.id) {
                    if col.len() == n {
                        let (dt, dy) =
                            decimate_minmax(&st.timestamps[i0..i1], &col[i0..i1], buckets);
                        let label = if mc.unit.is_empty() {
                            format!("∑ {}", mc.name)
                        } else {
                            format!("∑ {} ({})", mc.name, mc.unit)
                        };
                        traces.push(DecimatedTrace {
                            key: mc.id.clone(),
                            plot_id: mc.plot_id.clone(),
                            label,
                            color: mc.color.clone(),
                            width: mc.width,
                            gain: 1.0,
                            t: dt,
                            y: dy,
                        });
                    }
                }
            }
        }

        // Frame readout at configured reference point (latest / T1 / T2)
        let ref_idx = match prof.drawer_data_source.as_str() {
            "t1" => nearest_time_index(&st.timestamps, st.cursor_t1),
            "t2" => nearest_time_index(&st.timestamps, st.cursor_t2),
            _ => n - 1,
        };
        let snap = build_frame_readout(&prof, &st, ref_idx);

        // Optional X-Y Lissajous tail points
        let mut xy_x = Vec::new();
        let mut xy_y = Vec::new();
        if prof.xy_config.enabled {
            let tail = prof.xy_config.tail_points.clamp(32, 8000);
            let s_idx = n.saturating_sub(tail);
            if let (Some(col_x), Some(col_y)) = (
                resolve_series(&prof, &st, &prof.xy_config.x_source),
                resolve_series(&prof, &st, &prof.xy_config.y_source),
            ) {
                xy_x.extend_from_slice(&col_x[s_idx..n]);
                xy_y.extend_from_slice(&col_y[s_idx..n]);
            }
        }

        ViewportPayload {
            data_seq: d_seq,
            source_label: st.source_label.clone(),
            protocol: prof.connection.protocol.clone(),
            interface_type: prof.connection.interface_type.clone(),
            is_streaming: self.is_streaming.load(Ordering::Relaxed),
            paused: self.paused.load(Ordering::Relaxed),
            buffered_frames: n,
            sample_rate_hz: st.measured_hz,
            rx_kbps: st.rx_kbps,
            full_t_min,
            full_t_max,
            view_t_min,
            view_t_max,
            cursor_enabled: st.cursor_enabled,
            cursor_t1: st.cursor_t1,
            cursor_t2: st.cursor_t2,
            sample_time: snap.sample_time,
            spike_rejected: st.spike_rejected,
            status_decode_enabled: snap.status_decode_enabled,
            state_enabled: snap.state_enabled,
            state_label: snap.state_label,
            state_color: snap.state_color,
            fault_enabled: snap.fault_enabled,
            fault_code: snap.fault_code,
            active_faults: snap.active_faults,
            isr_enabled: snap.isr_enabled,
            isr_label: snap.isr_label,
            isr_full_scale: snap.isr_full_scale,
            isr_unit: snap.isr_unit,
            isr_cycles: snap.isr_cycles,
            cards: snap.cards,
            channel_vals: snap.channel_vals,
            channel_peaks: snap.channel_peaks,
            channel_mins: snap.channel_mins,
            series_tails: snap.series_tails,
            traces,
            xy_x,
            xy_y,
            math_errors: st.cached_math_errors.clone(),
            bookmark_count: st.bookmarks.len(),
            console_count: st.console_lines.len(),
        }
    }

    /// O(log N) single-frame telemetry readout for any timestamp (hover / cursor T1 / cursor T2 / latest)
    pub fn query_readout_at(&self, inspect_time: Option<f64>) -> ReadoutSnapshot {
        self.ensure_math_cache();
        let prof = self.profile.read().unwrap();
        let st = self.store.read().unwrap();
        let n = st.timestamps.len();
        if n == 0 {
            return ReadoutSnapshot {
                has_data: false,
                sample_time: 0.0,
                status_decode_enabled: prof.status_decode.enabled,
                state_enabled: prof.status_decode.state_enabled,
                state_label: "IDLE 待机".to_string(),
                state_color: "#71717A".to_string(),
                fault_enabled: prof.status_decode.fault_enabled,
                fault_code: 0,
                active_faults: Vec::new(),
                isr_enabled: prof.status_decode.isr_enabled,
                isr_label: prof.status_decode.isr_label.clone(),
                isr_full_scale: prof.status_decode.isr_full_scale.max(1.0),
                isr_unit: prof.status_decode.isr_unit.clone(),
                isr_cycles: 0,
                cards: Vec::new(),
                channel_vals: BTreeMap::new(),
                channel_peaks: BTreeMap::new(),
                channel_mins: BTreeMap::new(),
                series_tails: BTreeMap::new(),
            };
        }
        let idx = match inspect_time {
            Some(t) if t.is_finite() => nearest_time_index(&st.timestamps, t),
            _ => n - 1,
        };
        build_frame_readout(&prof, &st, idx)
    }

    pub fn set_cursors(&self, enabled: Option<bool>, t1: Option<f64>, t2: Option<f64>) {
        let mut st = self.store.write().unwrap();
        if let Some(en) = enabled {
            st.cursor_enabled = en;
        }
        if let Some(v1) = t1 {
            st.cursor_t1 = v1;
        }
        if let Some(v2) = t2 {
            st.cursor_t2 = v2;
        }
    }

    pub fn extract_slice(
        &self,
        t1: f64,
        t2: f64,
        name: &str,
        persist_in_bank: bool,
    ) -> Result<WaveformSlice, String> {
        self.ensure_math_cache();
        let prof = self.profile.read().unwrap();
        let mut st = self.store.write().unwrap();
        if st.timestamps.is_empty() {
            return Err("当前没有波形数据可供切片".to_string());
        }
        let (lo, hi) = if t1 <= t2 { (t1, t2) } else { (t2, t1) };
        let i0 = st.timestamps.partition_point(|&t| t < lo);
        let i1 = st.timestamps.partition_point(|&t| t <= hi);
        if i1 <= i0 {
            return Err(format!("选区 [{:.3}s ~ {:.3}s] 内没有采样点", lo, hi));
        }

        let t0 = st.timestamps[i0];
        let t_rel: Vec<f64> = st.timestamps[i0..i1].iter().map(|&t| t - t0).collect();
        let sample_count = t_rel.len();

        let mut channel_keys = Vec::new();
        let mut labels = BTreeMap::new();
        let mut data = BTreeMap::new();

        for ch in &prof.channels {
            if ch.index < st.columns.len() {
                let k = if !ch.alias.is_empty() {
                    ch.alias.clone()
                } else {
                    format!("CH{}", ch.index)
                };
                let lbl = if ch.unit.is_empty() {
                    ch.name.clone()
                } else {
                    format!("{} ({})", ch.name, ch.unit)
                };
                channel_keys.push(k.clone());
                labels.insert(k.clone(), lbl);
                data.insert(k, st.columns[ch.index][i0..i1].to_vec());
            }
        }
        for mc in &prof.math_channels {
            if let Some(col) = st.cached_math.get(&mc.id) {
                if col.len() >= i1 {
                    let k = if !mc.alias.is_empty() {
                        mc.alias.clone()
                    } else {
                        mc.id.clone()
                    };
                    let lbl = if mc.unit.is_empty() {
                        format!("∑ {}", mc.name)
                    } else {
                        format!("∑ {} ({})", mc.name, mc.unit)
                    };
                    channel_keys.push(k.clone());
                    labels.insert(k.clone(), lbl);
                    data.insert(k, col[i0..i1].to_vec());
                }
            }
        }

        let default_key = if data.contains_key("speed_est") {
            "speed_est".to_string()
        } else {
            channel_keys.first().cloned().unwrap_or_default()
        };

        let idx_num = st.slices.len() + 1;
        let sid = format!("S{}_{}", &now_timestamp_label(), idx_num);
        let sname = if name.trim().is_empty() {
            format!("切片_{} ({:.2}s~{:.2}s)", idx_num, lo, hi)
        } else {
            name.trim().to_string()
        };
        let color = TRACE_PALETTE[(idx_num - 1) % TRACE_PALETTE.len()].to_string();

        let sl = WaveformSlice {
            meta: SliceMeta {
                slice_id: sid,
                name: sname,
                note: String::new(),
                created_at: now_timestamp_label(),
                sample_rate_hz: st.measured_hz,
                sample_count,
                channel_keys,
                labels,
                enabled: true,
                selected_key: default_key,
                scale_x: 1.0,
                offset_x: 0.0,
                scale_y: 1.0,
                offset_y: 0.0,
                color,
                line_style: if idx_num % 2 == 0 {
                    "dash".to_string()
                } else {
                    "solid".to_string()
                },
                width: 1.7,
            },
            t_rel,
            data,
        };

        if persist_in_bank {
            sl.save_to_disk()?;
            st.slices.push(sl.clone());
        }
        Ok(sl)
    }

    pub fn import_external_csv_as_slice(&self, path: &Path) -> Result<SliceMeta, String> {
        let prof_clone = self.profile.read().unwrap().clone();
        let tmp = TelemetryEngine::new(prof_clone);
        tmp.load_csv_instant(path, None, false)?;
        let (t0, t1) = {
            let st = tmp.store.read().unwrap();
            (st.timestamps[0], *st.timestamps.last().unwrap())
        };
        let sname = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("外部CSV切片");
        let mut sl = tmp.extract_slice(t0, t1, sname, false)?;
        sl.meta.note = "本地导入实验 CSV".to_string();
        sl.save_to_disk()?;
        let meta = sl.meta.clone();
        self.store.write().unwrap().slices.push(sl);
        Ok(meta)
    }

    pub fn capture_full_buffer_slice(&self, name: &str) -> Result<SliceMeta, String> {
        let (t0, t1) = {
            let st = self.store.read().unwrap();
            if st.timestamps.is_empty() {
                return Err("当前示波缓冲区为空，请先连接或打开 CSV".to_string());
            }
            (st.timestamps[0], *st.timestamps.last().unwrap())
        };
        let sl = self.extract_slice(t0, t1, name, true)?;
        Ok(sl.meta)
    }

    pub fn get_analysis_overview(&self) -> serde_json::Value {
        let st = self.store.read().unwrap();
        let d_seq = self.data_seq.load(Ordering::Relaxed);
        let n = st.timestamps.len();
        let dur = if n >= 2 {
            (st.timestamps[n - 1] - st.timestamps[0]).max(0.0)
        } else {
            0.0
        };
        let metas: Vec<SliceMeta> = st.slices.iter().map(|s| s.meta.clone()).collect();
        serde_json::json!({
            "slices": metas,
            "live_buffer": {
                "available": n >= 2,
                "frames": n,
                "sample_rate_hz": st.measured_hz,
                "duration_s": round6(dur),
                "source_label": st.source_label,
                "data_seq": d_seq,
            }
        })
    }

    /// Returns uniformly-strided series data for static analysis (FFT, digital filters, step PK, X-Y, overlay)
    pub fn get_analysis_run_bundle(
        &self,
        slice_id: &str,
        max_samples: usize,
    ) -> Result<serde_json::Value, String> {
        let cap = max_samples.clamp(512, 65536);

        if slice_id == "__live_buffer__" {
            self.ensure_math_cache();
            let prof = self.profile.read().unwrap();
            let st = self.store.read().unwrap();
            let n = st.timestamps.len();
            if n < 2 {
                return Err("当前示波缓冲区暂无足够采样点".to_string());
            }
            let stride = ((n + cap - 1) / cap).max(1);
            let t0 = st.timestamps[0];
            let t_vec: Vec<f64> = (0..n)
                .step_by(stride)
                .map(|i| round6(st.timestamps[i] - t0))
                .collect();
            let hz_eff = if t_vec.len() >= 2 && (t_vec.last().unwrap() - t_vec[0]) > 1e-6 {
                ((t_vec.len() - 1) as f64 / (t_vec.last().unwrap() - t_vec[0])).round().max(1.0)
            } else {
                (st.measured_hz / stride as f64).max(1.0)
            };

            let mut channels = Vec::new();
            for ch in &prof.channels {
                if ch.index < st.columns.len() {
                    let col = &st.columns[ch.index];
                    let key = if !ch.alias.is_empty() {
                        ch.alias.clone()
                    } else {
                        format!("CH{}", ch.index)
                    };
                    let data_vec: Vec<f32> = (0..n)
                        .step_by(stride)
                        .map(|i| {
                            let v = col[i];
                            if v.is_finite() { v } else { 0.0 }
                        })
                        .collect();
                    channels.push(serde_json::json!({
                        "key": key,
                        "name": ch.name,
                        "unit": ch.unit,
                        "color": ch.color,
                        "data": data_vec,
                    }));
                }
            }
            for mc in &prof.math_channels {
                if let Some(col) = st.cached_math.get(&mc.id) {
                    if col.len() >= n {
                        let key = if !mc.alias.is_empty() {
                            mc.alias.clone()
                        } else {
                            mc.id.clone()
                        };
                        let data_vec: Vec<f32> = (0..n)
                            .step_by(stride)
                            .map(|i| {
                                let v = col[i];
                                if v.is_finite() { v } else { 0.0 }
                            })
                            .collect();
                        channels.push(serde_json::json!({
                            "key": key,
                            "name": format!("∑ {}", mc.name),
                            "unit": mc.unit,
                            "color": mc.color,
                            "data": data_vec,
                        }));
                    }
                }
            }

            return Ok(serde_json::json!({
                "slice_id": "__live_buffer__",
                "name": "当前示波缓冲 (实时快照)",
                "note": st.source_label,
                "color": "#0EA5E9",
                "hz": hz_eff,
                "raw_count": n,
                "t": t_vec,
                "channels": channels,
            }));
        }

        let prof = self.profile.read().unwrap();
        let st = self.store.read().unwrap();
        let sl = st
            .slices
            .iter()
            .find(|s| s.meta.slice_id == slice_id)
            .ok_or_else(|| format!("未找到实验记录 {}", slice_id))?;

        let n = sl.t_rel.len();
        let stride = ((n + cap - 1) / cap).max(1);
        let t_vec: Vec<f64> = (0..n).step_by(stride).map(|i| round6(sl.t_rel[i])).collect();
        let hz_eff = if t_vec.len() >= 2 && (t_vec.last().unwrap() - t_vec[0]) > 1e-6 {
            ((t_vec.len() - 1) as f64 / (t_vec.last().unwrap() - t_vec[0])).round().max(1.0)
        } else {
            (sl.meta.sample_rate_hz / stride as f64).max(1.0)
        };

        let mut channels = Vec::new();
        for (idx, k) in sl.meta.channel_keys.iter().enumerate() {
            if let Some(col) = sl.data.get(k) {
                let m = col.len().min(n);
                let data_vec: Vec<f32> = (0..m)
                    .step_by(stride)
                    .map(|i| {
                        let v = col[i];
                        if v.is_finite() { v } else { 0.0 }
                    })
                    .collect();

                // Match unit & color from current profile if possible
                let mut ch_name = sl.meta.labels.get(k).cloned().unwrap_or_else(|| k.clone());
                let mut ch_unit = String::new();
                let mut ch_color = TRACE_PALETTE[idx % TRACE_PALETTE.len()].to_string();
                if let Some(pc) = prof
                    .channels
                    .iter()
                    .find(|c| c.alias == *k || format!("CH{}", c.index) == *k)
                {
                    ch_name = pc.name.clone();
                    ch_unit = pc.unit.clone();
                    ch_color = pc.color.clone();
                } else if let Some(pm) = prof
                    .math_channels
                    .iter()
                    .find(|m| m.alias == *k || m.id == *k)
                {
                    ch_name = format!("∑ {}", pm.name);
                    ch_unit = pm.unit.clone();
                    ch_color = pm.color.clone();
                }

                channels.push(serde_json::json!({
                    "key": k,
                    "name": ch_name,
                    "unit": ch_unit,
                    "color": ch_color,
                    "data": data_vec,
                }));
            }
        }

        Ok(serde_json::json!({
            "slice_id": sl.meta.slice_id,
            "name": sl.meta.name,
            "note": sl.meta.note,
            "color": sl.meta.color,
            "offset_x": sl.meta.offset_x,
            "offset_y": sl.meta.offset_y,
            "hz": hz_eff,
            "raw_count": n,
            "t": t_vec,
            "channels": channels,
        }))
    }

    pub fn auto_align_slices(&self, ch_key: Option<&str>, threshold: Option<f32>) -> BTreeMap<String, f64> {
        let mut st = self.store.write().unwrap();
        let mut edge_times: Vec<(usize, f64)> = Vec::new();

        for (idx, sl) in st.slices.iter().enumerate() {
            if !sl.meta.enabled {
                continue;
            }
            let key = ch_key.unwrap_or(&sl.meta.selected_key);
            let Some(raw) = sl.data.get(key) else {
                continue;
            };
            if raw.len() < 4 {
                continue;
            }
            let mut min_v = f32::INFINITY;
            let mut max_v = f32::NEG_INFINITY;
            for &v in raw {
                let sv = v * sl.meta.scale_y + sl.meta.offset_y;
                min_v = min_v.min(sv);
                max_v = max_v.max(sv);
            }
            if (max_v - min_v).abs() < 1e-9 {
                continue;
            }
            let th = threshold.unwrap_or_else(|| min_v + 0.5 * (max_v - min_v));
            if let Some(pos) = raw
                .iter()
                .position(|&v| (v * sl.meta.scale_y + sl.meta.offset_y) >= th)
            {
                let t_cross = sl.t_rel[pos] * sl.meta.scale_x;
                edge_times.push((idx, t_cross));
            }
        }

        let mut aligned = BTreeMap::new();
        if let Some(&(_, ref_t)) = edge_times.first() {
            for (idx, tc) in edge_times {
                let sl = &mut st.slices[idx];
                sl.meta.offset_x = ((ref_t - tc) * 10000.0).round() / 10000.0;
                let _ = sl.save_to_disk();
                aligned.insert(sl.meta.name.clone(), sl.meta.offset_x);
            }
        }
        aligned
    }

    pub fn get_overlay_render_data(&self, pixel_width: usize) -> serde_json::Value {
        let st = self.store.read().unwrap();
        let buckets = pixel_width.clamp(240, 2400);
        let mut metas = Vec::new();
        let mut curves = Vec::new();

        for sl in &st.slices {
            metas.push(sl.meta.clone());
            if !sl.meta.enabled {
                continue;
            }
            if let Some(raw) = sl.data.get(&sl.meta.selected_key) {
                let tx: Vec<f64> = sl
                    .t_rel
                    .iter()
                    .map(|&t| t * sl.meta.scale_x + sl.meta.offset_x)
                    .collect();
                let vy: Vec<f32> = raw
                    .iter()
                    .map(|&y| y * sl.meta.scale_y + sl.meta.offset_y)
                    .collect();
                let (dt, dy) = decimate_minmax(&tx, &vy, buckets);
                let ch_lbl = sl
                    .meta
                    .labels
                    .get(&sl.meta.selected_key)
                    .cloned()
                    .unwrap_or_else(|| sl.meta.selected_key.clone());
                curves.push(serde_json::json!({
                    "slice_id": sl.meta.slice_id,
                    "label": format!("{} [{}]", sl.meta.name, ch_lbl),
                    "color": sl.meta.color,
                    "line_style": sl.meta.line_style,
                    "width": sl.meta.width,
                    "t": dt,
                    "y": dy,
                }));
            }
        }

        serde_json::json!({
            "slices": metas,
            "curves": curves,
        })
    }

    pub fn compute_window_stats(
        &self,
        t1_opt: Option<f64>,
        t2_opt: Option<f64>,
        req_channels: Option<&[String]>,
    ) -> Result<serde_json::Value, String> {
        self.ensure_math_cache();
        let prof = self.profile.read().unwrap();
        let st = self.store.read().unwrap();
        if st.timestamps.is_empty() {
            return Err("当前缓冲区为空".to_string());
        }
        let raw_t1 = t1_opt.unwrap_or(st.cursor_t1);
        let raw_t2 = t2_opt.unwrap_or(st.cursor_t2);
        let (lo, hi) = if raw_t1 <= raw_t2 {
            (raw_t1, raw_t2)
        } else {
            (raw_t2, raw_t1)
        };

        let i0 = st.timestamps.partition_point(|&t| t < lo);
        let i1 = st.timestamps.partition_point(|&t| t <= hi);
        if i1 <= i0 {
            return Err(format!("区间 [{:.3}s ~ {:.3}s] 内无采样点", lo, hi));
        }

        let keys: Vec<String> = if let Some(rk) = req_channels {
            rk.to_vec()
        } else {
            let mut ks = Vec::new();
            for ch in &prof.channels {
                if (ch.visible || ch.show_card)
                    && ch.decoder_type == "float"
                    && ch.index < st.columns.len()
                {
                    ks.push(if !ch.alias.is_empty() {
                        ch.alias.clone()
                    } else {
                        format!("CH{}", ch.index)
                    });
                }
            }
            for mc in &prof.math_channels {
                if mc.visible || mc.show_card {
                    ks.push(if !mc.alias.is_empty() {
                        mc.alias.clone()
                    } else {
                        mc.id.clone()
                    });
                }
            }
            ks
        };

        let mut stats = Vec::new();
        for k in &keys {
            if let Some(series) = resolve_series(&prof, &st, k) {
                let sub = &series[i0..i1];
                if sub.is_empty() {
                    continue;
                }
                let n = sub.len() as f64;
                let mut sum = 0.0f64;
                let mut sq_sum = 0.0f64;
                let mut min_v = f64::INFINITY;
                let mut max_v = f64::NEG_INFINITY;
                for &v in sub {
                    let dv = v as f64;
                    sum += dv;
                    sq_sum += dv * dv;
                    min_v = min_v.min(dv);
                    max_v = max_v.max(dv);
                }
                let mean = sum / n;
                let rms = (sq_sum / n).sqrt();
                let var = ((sq_sum / n) - mean * mean).max(0.0);
                let std_dev = var.sqrt();
                let tail_start = sub.len() - (sub.len() / 5).max(1);
                let steady: f64 = sub[tail_start..].iter().map(|&v| v as f64).sum::<f64>()
                    / (sub.len() - tail_start) as f64;
                let init = sub[0] as f64;
                let step = steady - init;
                let overshoot = if step.abs() > 1e-6 {
                    if step > 0.0 {
                        ((max_v - steady) / step * 100.0).max(0.0)
                    } else {
                        ((steady - min_v) / (-step) * 100.0).max(0.0)
                    }
                } else {
                    0.0
                };

                stats.push(ChannelWindowStat {
                    key: k.clone(),
                    label: resolve_label(&prof, k),
                    mean: round6(mean),
                    rms: round6(rms),
                    std_dev: round6(std_dev),
                    min: round6(min_v),
                    max: round6(max_v),
                    p2p: round6(max_v - min_v),
                    overshoot_pct: ((overshoot * 100.0).round() / 100.0),
                });
            }
        }

        let bms: Vec<BookmarkEvent> = st
            .bookmarks
            .iter()
            .filter(|b| b.time_s >= lo && b.time_s <= hi)
            .cloned()
            .collect();

        Ok(serde_json::json!({
            "t_start": round6(st.timestamps[i0]),
            "t_end": round6(st.timestamps[i1 - 1]),
            "duration_s": round6(st.timestamps[i1 - 1] - st.timestamps[i0]),
            "sample_count": i1 - i0,
            "sample_rate_hz": st.measured_hz,
            "stats": stats,
            "bookmarks_in_window": bms,
        }))
    }

    pub fn compute_fft(
        &self,
        channel_key: &str,
        t1_opt: Option<f64>,
        t2_opt: Option<f64>,
    ) -> Result<FftResult, String> {
        self.ensure_math_cache();
        let prof = self.profile.read().unwrap();
        let st = self.store.read().unwrap();
        if st.timestamps.len() < 8 {
            return Err("采样点不足 8 个，无法计算 FFT".to_string());
        }
        let raw_t1 = t1_opt.unwrap_or(st.cursor_t1);
        let raw_t2 = t2_opt.unwrap_or(st.cursor_t2);
        let (lo, hi) = if raw_t1 <= raw_t2 {
            (raw_t1, raw_t2)
        } else {
            (raw_t2, raw_t1)
        };
        let mut i0 = st.timestamps.partition_point(|&t| t < lo);
        let mut i1 = st.timestamps.partition_point(|&t| t <= hi);
        if i1.saturating_sub(i0) < 8 {
            i0 = 0;
            i1 = st.timestamps.len();
        }
        // Cap FFT window to 65536 points for instant sub-millisecond response
        if i1 - i0 > 65536 {
            i1 = i0 + 65536;
        }

        let series = resolve_series(&prof, &st, channel_key)
            .ok_or_else(|| format!("未找到通道 '{}'", channel_key))?;
        let sub = &series[i0..i1];
        let n = sub.len();
        let dt = if n > 1 {
            ((st.timestamps[i1 - 1] - st.timestamps[i0]) / (n - 1) as f64).max(1e-9)
        } else {
            1.0 / st.measured_hz.max(1.0)
        };
        let fs = 1.0 / dt;

        let dc: f64 = sub.iter().map(|&v| v as f64).sum::<f64>() / n as f64;
        let mut buffer: Vec<Complex<f64>> = Vec::with_capacity(n);
        let mut win_sum = 0.0f64;
        for (i, &v) in sub.iter().enumerate() {
            let w = 0.5 * (1.0 - (2.0 * std::f64::consts::PI * i as f64 / (n - 1).max(1) as f64).cos());
            win_sum += w;
            buffer.push(Complex {
                re: (v as f64 - dc) * w,
                im: 0.0,
            });
        }
        let coherent_gain = (win_sum / n as f64).max(1e-9);

        let mut planner = FftPlanner::<f64>::new();
        let fft = planner.plan_fft_forward(n);
        fft.process(&mut buffer);

        let half = n / 2 + 1;
        let mut freqs = Vec::with_capacity(half);
        let mut amps = Vec::with_capacity(half);
        let scale = 2.0 / (n as f64 * coherent_gain);

        for k in 0..half {
            freqs.push(k as f64 * fs / n as f64);
            if k == 0 {
                amps.push(dc.abs() as f32);
            } else {
                amps.push((buffer[k].norm() * scale) as f32);
            }
        }

        // Top 8 AC harmonic peaks (skipping DC bin 0)
        let mut order: Vec<usize> = (1..half).collect();
        order.sort_by(|&a, &b| amps[b].partial_cmp(&amps[a]).unwrap_or(std::cmp::Ordering::Equal));
        let mut used_bins: Vec<usize> = Vec::new();
        let mut peaks = Vec::new();
        for b in order {
            if used_bins.iter().any(|&ub| ub.abs_diff(b) <= 2) {
                continue;
            }
            used_bins.push(b);
            peaks.push(FftPeak {
                freq_hz: ((freqs[b] * 100.0).round() / 100.0),
                amplitude: round6(amps[b] as f64),
            });
            if peaks.len() >= 8 {
                break;
            }
        }

        // Decimate FFT curve if > 1600 bins for ultra-crisp spectrum rendering
        let (out_f, out_a) = if half > 1600 {
            decimate_minmax(&freqs[1..], &amps[1..], 1200)
        } else if half > 1 {
            (freqs[1..].to_vec(), amps[1..].to_vec())
        } else {
            (freqs, amps)
        };

        Ok(FftResult {
            channel: channel_key.to_string(),
            label: resolve_label(&prof, channel_key),
            sample_rate_hz: (fs * 10.0).round() / 10.0,
            n_samples: n,
            dc_component: round6(dc),
            peaks,
            freqs: out_f,
            amps: out_a,
        })
    }
}

fn resolve_series<'a>(
    prof: &StudioProfile,
    st: &'a InnerStore,
    key: &str,
) -> Option<&'a [f32]> {
    if let Some(rest) = key.strip_prefix("CH") {
        if let Ok(idx) = rest.parse::<usize>() {
            if idx < st.columns.len() {
                return Some(&st.columns[idx]);
            }
        }
    }
    for ch in &prof.channels {
        if (ch.alias == key || ch.name == key) && ch.index < st.columns.len() {
            return Some(&st.columns[ch.index]);
        }
    }
    if let Some(vec) = st.cached_math.get(key) {
        return Some(vec);
    }
    for mc in &prof.math_channels {
        if mc.alias == key || mc.name == key {
            if let Some(vec) = st.cached_math.get(&mc.id) {
                return Some(vec);
            }
        }
    }
    None
}

fn resolve_label(prof: &StudioProfile, key: &str) -> String {
    for ch in &prof.channels {
        if key == format!("CH{}", ch.index) || ch.alias == key || ch.name == key {
            return if ch.unit.is_empty() {
                ch.name.clone()
            } else {
                format!("{} ({})", ch.name, ch.unit)
            };
        }
    }
    for mc in &prof.math_channels {
        if mc.id == key || mc.alias == key || mc.name == key {
            return if mc.unit.is_empty() {
                format!("∑ {}", mc.name)
            } else {
                format!("∑ {} ({})", mc.name, mc.unit)
            };
        }
    }
    key.to_string()
}

/// Preserves exact peaks & spikes across arbitrary zoom levels in O(N) sequential L1 scan (compact 2-point envelope!)
pub fn decimate_minmax(t: &[f64], y: &[f32], max_buckets: usize) -> (Vec<f64>, Vec<f32>) {
    let n = t.len().min(y.len());
    if n == 0 {
        return (Vec::new(), Vec::new());
    }
    if n <= max_buckets * 2 || max_buckets == 0 {
        let out_t: Vec<f64> = t[..n].iter().map(|&v| round6(v)).collect();
        let out_y: Vec<f32> = y[..n]
            .iter()
            .map(|&v| if v.is_finite() { v } else { 0.0 })
            .collect();
        return (out_t, out_y);
    }

    let mut out_t = Vec::with_capacity(max_buckets * 2 + 2);
    let mut out_y = Vec::with_capacity(max_buckets * 2 + 2);
    let mut last_pushed_idx = usize::MAX;

    let mut push_pt = |ix: usize| {
        if ix != last_pushed_idx {
            out_t.push(round6(t[ix]));
            let v = y[ix];
            out_y.push(if v.is_finite() { v } else { 0.0 });
            last_pushed_idx = ix;
        }
    };

    push_pt(0);
    for b in 0..max_buckets {
        let s = b * n / max_buckets;
        let e = ((b + 1) * n / max_buckets).min(n);
        if s >= e {
            continue;
        }
        let mut min_idx = s;
        let mut max_idx = s;
        let mut min_v = if y[s].is_finite() { y[s] } else { 0.0 };
        let mut max_v = min_v;
        for i in (s + 1)..e {
            let v = y[i];
            if !v.is_finite() {
                continue;
            }
            if v < min_v {
                min_v = v;
                min_idx = i;
            }
            if v > max_v {
                max_v = v;
                max_idx = i;
            }
        }

        if min_idx <= max_idx {
            push_pt(min_idx);
            push_pt(max_idx);
        } else {
            push_pt(max_idx);
            push_pt(min_idx);
        }
    }
    push_pt(n - 1);
    (out_t, out_y)
}

fn trim_ascii(mut b: &[u8]) -> &[u8] {
    while let Some(&first) = b.first() {
        if first.is_ascii_whitespace() {
            b = &b[1..];
        } else {
            break;
        }
    }
    while let Some(&last) = b.last() {
        if last.is_ascii_whitespace() {
            b = &b[..b.len() - 1];
        } else {
            break;
        }
    }
    b
}

fn decode_text_smart(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    // On Windows, fallback via MultiByteToWideChar(CP_ACP = 936 GBK) so GBK CSV headers decode cleanly!
    #[cfg(target_os = "windows")]
    {
        extern "system" {
            fn MultiByteToWideChar(
                CodePage: u32,
                dwFlags: u32,
                lpMultiByteStr: *const u8,
                cbMultiByte: i32,
                lpWideCharStr: *mut u16,
                cchWideChar: i32,
            ) -> i32;
        }
        unsafe {
            let needed = MultiByteToWideChar(0, 0, bytes.as_ptr(), bytes.len() as i32, std::ptr::null_mut(), 0);
            if needed > 0 {
                let mut wide = vec![0u16; needed as usize];
                let written = MultiByteToWideChar(
                    0,
                    0,
                    bytes.as_ptr(),
                    bytes.len() as i32,
                    wide.as_mut_ptr(),
                    needed,
                );
                if written > 0 {
                    return String::from_utf16_lossy(&wide[..written as usize]);
                }
            }
        }
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn fmt_speed_mult(spd: f64) -> String {
    let r = (spd * 100.0).round() / 100.0;
    if (r - r.round()).abs() < 1e-6 {
        format!("{:.1}", r)
    } else if (r * 10.0 - (r * 10.0).round()).abs() < 1e-6 {
        format!("{:.1}", r)
    } else {
        format!("{:.2}", r)
    }
}

fn round6(v: f64) -> f64 {
    if !v.is_finite() {
        0.0
    } else {
        (v * 1_000_000.0).round() / 1_000_000.0
    }
}

fn nearest_time_index(timestamps: &[f64], target_t: f64) -> usize {
    let n = timestamps.len();
    if n <= 1 {
        return 0;
    }
    let pos = timestamps.partition_point(|&t| t < target_t);
    if pos == 0 {
        0
    } else if pos >= n {
        n - 1
    } else if (target_t - timestamps[pos - 1]).abs() <= (timestamps[pos] - target_t).abs() {
        pos - 1
    } else {
        pos
    }
}

fn build_frame_readout(prof: &StudioProfile, st: &InnerStore, idx: usize) -> ReadoutSnapshot {
    let sd = &prof.status_decode;
    let mut cards = Vec::new();
    let mut state_label = "IDLE 待机".to_string();
    let mut state_color = "#71717A".to_string();
    let mut fault_code = 0u16;
    let mut active_faults = Vec::new();
    let mut isr_cycles = 0i32;

    if sd.enabled {
        if sd.state_enabled {
            if let Some(ch_i) = sd.state_channel {
                if ch_i < st.columns.len() {
                    let iv = st.columns[ch_i][idx].round() as i32;
                    if let Some(ent) = sd.state_enum_map.get(&iv.to_string()) {
                        state_label = ent.label.clone();
                        state_color = ent.color.clone();
                    } else {
                        state_label = format!("STATE {}", iv);
                    }
                }
            }
        }
        if sd.fault_enabled {
            if let Some(ch_i) = sd.fault_channel {
                if ch_i < st.columns.len() {
                    fault_code = (st.columns[ch_i][idx].round() as i64 & 0xFFFF) as u16;
                    if fault_code != 0 {
                        if sd.fault_mode == "enum" {
                            active_faults.push(
                                sd.fault_map
                                    .get(&fault_code.to_string())
                                    .cloned()
                                    .unwrap_or_else(|| format!("故障码 {}", fault_code)),
                            );
                        } else {
                            for b in 0..16 {
                                if (fault_code & (1 << b)) != 0 {
                                    active_faults.push(
                                        sd.fault_map
                                            .get(&b.to_string())
                                            .cloned()
                                            .unwrap_or_else(|| format!("Bit{}", b)),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        if sd.isr_enabled {
            if let Some(ch_i) = sd.isr_channel {
                if ch_i < st.columns.len() {
                    isr_cycles = (st.columns[ch_i][idx].round() as i32).max(0);
                }
            }
        }
    }

    let st_skip = if sd.enabled && sd.state_enabled {
        sd.state_channel
    } else {
        None
    };
    let flt_skip = if sd.enabled && sd.fault_enabled {
        sd.fault_channel
    } else {
        None
    };

    for ch in &prof.channels {
        if ch.index >= st.columns.len() {
            continue;
        }
        if Some(ch.index) == st_skip || Some(ch.index) == flt_skip {
            continue;
        }
        if ch.show_card {
            let val = st.columns[ch.index][idx];
            cards.push(CardReadout {
                key: format!("CH{}", ch.index),
                name: ch.name.clone(),
                unit: ch.unit.clone(),
                color: ch.color.clone(),
                value_str: format!("{:.*}", ch.decimals, val),
                raw_value: val,
            });
        }
    }

    for mc in &prof.math_channels {
        if mc.show_card {
            if let Some(col) = st.cached_math.get(&mc.id) {
                if let Some(&val) = col.get(idx) {
                    cards.push(CardReadout {
                        key: mc.id.clone(),
                        name: format!("∑ {}", mc.name),
                        unit: mc.unit.clone(),
                        color: mc.color.clone(),
                        value_str: format!("{:.*}", mc.decimals, val),
                        raw_value: val,
                    });
                }
            }
        }
    }

    // Populate full-channel instantaneous values, peaks/mins, and widget tail windows at `idx`
    let mut channel_vals = BTreeMap::new();
    let mut channel_peaks = BTreeMap::new();
    let mut channel_mins = BTreeMap::new();

    for (c_i, col) in st.columns.iter().enumerate() {
        if let Some(&val) = col.get(idx) {
            let key = format!("CH{}", c_i);
            channel_vals.insert(key.clone(), val);
            if let Some(&pk) = st.col_peaks.get(c_i) {
                if pk.is_finite() {
                    channel_peaks.insert(key.clone(), pk);
                }
            }
            if let Some(&mn) = st.col_mins.get(c_i) {
                if mn.is_finite() {
                    channel_mins.insert(key, mn);
                }
            }
        }
    }

    for mc in &prof.math_channels {
        if let Some(col) = st.cached_math.get(&mc.id) {
            if let Some(&val) = col.get(idx) {
                channel_vals.insert(mc.id.clone(), val);
                let s0 = idx.saturating_sub(2048);
                let mut pk = val;
                let mut mn = val;
                for &v in &col[s0..=idx] {
                    if v.is_finite() {
                        if v.abs() > pk.abs() {
                            pk = v;
                        }
                        if v < mn {
                            mn = v;
                        }
                    }
                }
                channel_peaks.insert(mc.id.clone(), pk);
                channel_mins.insert(mc.id.clone(), mn);
            }
        }
    }

    // Collect recent window tails only for channels used in `xy_lissajous`, `spark_stat`, or `num_grid(pp)`
    let mut tail_reqs: BTreeMap<String, usize> = BTreeMap::new();
    let mut req_tail = |k_opt: Option<&str>, len: usize| {
        if let Some(k) = k_opt {
            let kt = k.trim();
            if !kt.is_empty() {
                let entry = tail_reqs.entry(kt.to_string()).or_insert(0);
                if len > *entry {
                    *entry = len;
                }
            }
        }
    };

    for w in &prof.drawer_widgets {
        if !w.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true) {
            continue;
        }
        match w.get("type").and_then(|v| v.as_str()).unwrap_or("") {
            "xy_lissajous" => {
                let tp = w
                    .get("tailPoints")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(64)
                    .clamp(8, 800) as usize;
                req_tail(w.get("xChannel").and_then(|v| v.as_str()), tp);
                req_tail(w.get("yChannel").and_then(|v| v.as_str()), tp);
            }
            "spark_stat" => {
                let wp = w
                    .get("windowPts")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(60)
                    .clamp(8, 512) as usize;
                req_tail(w.get("channel").and_then(|v| v.as_str()), wp);
            }
            "num_grid" => {
                if let Some(arr) = w.get("cards").and_then(|v| v.as_array()) {
                    for c in arr {
                        if c.get("sub").and_then(|v| v.as_str()) == Some("pp") {
                            req_tail(c.get("ch").and_then(|v| v.as_str()), 64);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let mut series_tails = BTreeMap::new();
    for (ch_key, len) in tail_reqs {
        if let Some(col) = resolve_series(prof, st, &ch_key) {
            if idx < col.len() {
                let s0 = idx.saturating_sub(len.saturating_sub(1));
                series_tails.insert(ch_key, col[s0..=idx].to_vec());
            }
        }
    }

    ReadoutSnapshot {
        has_data: true,
        sample_time: st.timestamps[idx],
        status_decode_enabled: sd.enabled,
        state_enabled: sd.state_enabled,
        state_label,
        state_color,
        fault_enabled: sd.fault_enabled,
        fault_code,
        active_faults,
        isr_enabled: sd.isr_enabled,
        isr_label: sd.isr_label.clone(),
        isr_full_scale: sd.isr_full_scale.max(1.0),
        isr_unit: sd.isr_unit.clone(),
        isr_cycles,
        cards,
        channel_vals,
        channel_peaks,
        channel_mins,
        series_tails,
    }
}
