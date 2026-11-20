#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use base64::Engine as _;
use fluxscope_core::engine::{ReadoutSnapshot, TelemetryEngine, ViewportPayload};
use fluxscope_core::math_eval::compile_expression;
use fluxscope_core::mcp_tools::{
    build_universal_ai_prompt, execute_mcp_tool, mcp_tools_schema, resolve_mcp_exe_path,
    run_stdio_mcp_bridge, start_embedded_http_mcp, GuiHooks, MCP_HTTP_ADDR,
};
use fluxscope_core::models::{
    app_data_dir, now_timestamp_label, screenshots_dir, slices_dir, SliceMeta, StudioProfile,
};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

struct AppState {
    engine: Arc<TelemetryEngine>,
    mcp_online: bool,
    screenshot_waiters: Arc<Mutex<Vec<Sender<Result<PathBuf, String>>>>>,
}

#[tauri::command]
fn get_bootstrap(state: State<'_, AppState>) -> Value {
    let prof = state.engine.profile.read().unwrap().clone();
    json!({
        "profile": prof,
        "ports": TelemetryEngine::list_serial_ports(),
        "mcp_online": state.mcp_online,
        "mcp_http_url": format!("http://{}/mcp", MCP_HTTP_ADDR),
        "mcp_exe_path": resolve_mcp_exe_path().to_string_lossy(),
        "data_dir": app_data_dir().to_string_lossy(),
        "ai_prompt": build_universal_ai_prompt(),
        "mcp_tools": mcp_tools_schema(),
    })
}

#[tauri::command]
fn get_viewport(
    state: State<'_, AppState>,
    t_min: Option<f64>,
    t_max: Option<f64>,
    pixel_width: usize,
    follow_tail_sec: Option<f64>,
    follow_anchor_ratio: Option<f64>,
) -> ViewportPayload {
    state.engine.query_viewport(
        t_min,
        t_max,
        pixel_width,
        follow_tail_sec,
        follow_anchor_ratio,
    )
}

#[tauri::command]
fn get_readout_at(state: State<'_, AppState>, inspect_time: Option<f64>) -> ReadoutSnapshot {
    state.engine.query_readout_at(inspect_time)
}

#[tauri::command]
fn check_data_seq(state: State<'_, AppState>) -> Value {
    let seq = state
        .engine
        .data_seq
        .load(std::sync::atomic::Ordering::Relaxed);
    let streaming = state
        .engine
        .is_streaming
        .load(std::sync::atomic::Ordering::Relaxed);
    let paused = state
        .engine
        .paused
        .load(std::sync::atomic::Ordering::Relaxed);
    json!({
        "data_seq": seq,
        "is_streaming": streaming,
        "paused": paused,
    })
}

#[tauri::command]
fn save_profile(state: State<'_, AppState>, mut profile: StudioProfile) -> Result<Value, String> {
    // Validate math expressions and return warnings if any
    let mut math_warnings = Vec::new();
    for mc in &profile.math_channels {
        if let Err(e) = compile_expression(&mc.expression) {
            math_warnings.push(format!("{} ({}): {}", mc.name, mc.id, e));
        }
    }
    profile.status_decode.initialized = true;
    profile.sync_channels_with_status_decode();
    profile.save(None)?;
    let ret_prof = profile.clone();
    *state.engine.profile.write().unwrap() = profile;
    state.engine.bump_math_seq();
    state.engine.bump_data_seq();
    Ok(json!({ "ok": true, "math_warnings": math_warnings, "profile": ret_prof }))
}

#[tauri::command]
fn set_cursors_cmd(
    state: State<'_, AppState>,
    enabled: Option<bool>,
    t1: Option<f64>,
    t2: Option<f64>,
) -> Value {
    state.engine.set_cursors(enabled, t1, t2);
    json!({ "ok": true })
}

#[tauri::command]
fn cut_slice_cmd(
    state: State<'_, AppState>,
    t1: f64,
    t2: f64,
    name: String,
) -> Result<SliceMeta, String> {
    let sl = state.engine.extract_slice(t1, t2, &name, true)?;
    Ok(sl.meta)
}

#[tauri::command]
fn export_cursor_csv_cmd(
    state: State<'_, AppState>,
    t1: f64,
    t2: f64,
    path: String,
) -> Result<Value, String> {
    let sl = state.engine.extract_slice(t1, t2, "_cursor_export", false)?;
    let saved = sl.export_csv(Path::new(&path))?;
    Ok(json!({
        "path": saved.to_string_lossy(),
        "samples": sl.meta.sample_count,
    }))
}

#[tauri::command]
fn get_overlay_data(state: State<'_, AppState>, pixel_width: usize) -> Value {
    state.engine.get_overlay_render_data(pixel_width)
}

#[tauri::command]
fn update_slice_cmd(
    state: State<'_, AppState>,
    slice_id: String,
    patch: Value,
) -> Result<Value, String> {
    execute_mcp_tool(
        &state.engine,
        None,
        "foc_manage_slices",
        &json!({
            "action": "update",
            "slice_id": slice_id,
            "enabled": patch.get("enabled"),
            "selected_key": patch.get("selected_key"),
            "scale_x": patch.get("scale_x"),
            "offset_x": patch.get("offset_x"),
            "scale_y": patch.get("scale_y"),
            "offset_y": patch.get("offset_y"),
            "color": patch.get("color"),
            "line_style": patch.get("line_style"),
        }),
    )?;
    if patch.get("name").is_some() || patch.get("note").is_some() {
        let mut st = state.engine.store.write().unwrap();
        if let Some(sl) = st.slices.iter_mut().find(|s| s.meta.slice_id == slice_id) {
            if let Some(new_name) = patch.get("name").and_then(|v| v.as_str()) {
                sl.meta.name = new_name.to_string();
            }
            if let Some(new_note) = patch.get("note").and_then(|v| v.as_str()) {
                sl.meta.note = new_note.to_string();
            }
            let _ = sl.save_to_disk();
        }
    }
    Ok(json!({ "ok": true }))
}

#[tauri::command]
fn capture_full_slice_cmd(state: State<'_, AppState>, name: String) -> Result<SliceMeta, String> {
    state.engine.capture_full_buffer_slice(&name)
}

#[tauri::command]
fn get_analysis_overview_cmd(state: State<'_, AppState>) -> Value {
    state.engine.get_analysis_overview()
}

#[tauri::command]
fn get_analysis_run_cmd(
    state: State<'_, AppState>,
    slice_id: String,
    max_samples: Option<usize>,
) -> Result<Value, String> {
    state
        .engine
        .get_analysis_run_bundle(&slice_id, max_samples.unwrap_or(16384))
}

#[tauri::command]
fn save_png_file_cmd(path: String, data_url: String) -> Result<String, String> {
    let b64 = data_url
        .split_once(',')
        .map(|(_, r)| r)
        .unwrap_or(&data_url);
    let png_bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| format!("PNG 解码失败: {}", e))?;
    if let Some(parent) = Path::new(&path).parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&path, png_bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

#[tauri::command]
fn delete_slice_cmd(state: State<'_, AppState>, slice_id: String) -> Result<Value, String> {
    let mut st = state.engine.store.write().unwrap();
    st.slices.retain(|s| s.meta.slice_id != slice_id);
    let _ = fs::remove_file(slices_dir().join(format!("{}.fslice", slice_id)));
    Ok(json!({ "ok": true }))
}

#[tauri::command]
fn export_slice_csv_cmd(
    state: State<'_, AppState>,
    slice_id: String,
    path: String,
) -> Result<Value, String> {
    execute_mcp_tool(
        &state.engine,
        None,
        "foc_manage_slices",
        &json!({
            "action": "export_csv",
            "slice_id": slice_id,
            "export_csv_path": path,
        }),
    )
}

#[tauri::command]
fn import_csv_slice_cmd(state: State<'_, AppState>, path: String) -> Result<SliceMeta, String> {
    state.engine.import_external_csv_as_slice(Path::new(&path))
}

#[tauri::command]
fn auto_align_slices_cmd(
    state: State<'_, AppState>,
    channel: Option<String>,
) -> Result<Value, String> {
    let aligned = state.engine.auto_align_slices(channel.as_deref(), None);
    Ok(json!({ "aligned": aligned }))
}

#[tauri::command]
fn compute_stats_and_fft(
    state: State<'_, AppState>,
    fft_channel: String,
    t1: Option<f64>,
    t2: Option<f64>,
) -> Result<Value, String> {
    let stats = state.engine.compute_window_stats(t1, t2, None)?;
    let fft = state.engine.compute_fft(&fft_channel, t1, t2)?;
    Ok(json!({
        "window_stats": stats,
        "fft": fft,
    }))
}

#[tauri::command]
fn get_bookmarks_and_console(state: State<'_, AppState>) -> Value {
    let st = state.engine.store.read().unwrap();
    json!({
        "bookmarks": st.bookmarks,
        "console_lines": st.console_lines,
    })
}

#[tauri::command]
fn clear_console_cmd(state: State<'_, AppState>) -> Value {
    state.engine.store.write().unwrap().console_lines.clear();
    json!({ "ok": true })
}

#[tauri::command]
fn serial_action(state: State<'_, AppState>, args: Value) -> Result<Value, String> {
    execute_mcp_tool(&state.engine, None, "foc_stream_and_serial_control", &args)
}

#[tauri::command]
fn profile_action(state: State<'_, AppState>, args: Value) -> Result<Value, String> {
    let res = execute_mcp_tool(&state.engine, None, "foc_save_or_load_profile", &args)?;
    let prof = state.engine.profile.read().unwrap().clone();
    Ok(json!({ "result": res, "profile": prof }))
}

#[tauri::command]
fn submit_screenshot_data(
    state: State<'_, AppState>,
    data_url: String,
    target: String,
) -> Result<String, String> {
    let b64 = data_url
        .split_once(',')
        .map(|(_, r)| r)
        .unwrap_or(&data_url);
    let png_bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| format!("PNG 解码失败: {}", e))?;
    let out_path =
        screenshots_dir().join(format!("fluxscope_{}_{}.png", target, now_timestamp_label()));
    fs::write(&out_path, png_bytes).map_err(|e| e.to_string())?;
    let mut waiters = state.screenshot_waiters.lock().unwrap();
    for tx in waiters.drain(..) {
        let _ = tx.send(Ok(out_path.clone()));
    }
    Ok(out_path.to_string_lossy().into_owned())
}

/// Native OS Open/Save File Dialog (zero external dependency, runs off UI thread)
#[tauri::command]
fn pick_file_dialog(mode: String, default_name: Option<String>, filter_type: Option<String>) -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        let handle = std::thread::spawn(move || win32_file_dialog(&mode, default_name.as_deref(), filter_type.as_deref()));
        handle.join().ok().flatten()
    }
    #[cfg(target_os = "macos")]
    {
        let handle = std::thread::spawn(move || macos_file_dialog(&mode, default_name.as_deref(), filter_type.as_deref()));
        handle.join().ok().flatten()
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let handle = std::thread::spawn(move || linux_file_dialog(&mode, default_name.as_deref(), filter_type.as_deref()));
        handle.join().ok().flatten()
    }
}

#[cfg(target_os = "macos")]
fn macos_file_dialog(mode: &str, default_name: Option<&str>, filter_type: Option<&str>) -> Option<String> {
    let mut cmd = std::process::Command::new("/usr/bin/osascript");
    let ftype = filter_type.unwrap_or("csv");
    if mode == "save" {
        let def_name = default_name.unwrap_or(if ftype == "png" { "export.png" } else { "export.csv" });
        cmd.arg("-e")
            .arg("on run argv")
            .arg("-e")
            .arg("POSIX path of (choose file name with prompt \"保存文件\" default name (item 1 of argv))")
            .arg("-e")
            .arg("end run")
            .arg("--")
            .arg(def_name);
    } else {
        cmd.arg("-e")
            .arg("on run argv")
            .arg("-e")
            .arg("set ftype to item 1 of argv")
            .arg("-e")
            .arg("if ftype is \"png\" then")
            .arg("-e")
            .arg("POSIX path of (choose file of type {\"png\", \"public.png\"} with prompt \"选择 PNG 图片\")")
            .arg("-e")
            .arg("else if ftype is \"csv\" then")
            .arg("-e")
            .arg("POSIX path of (choose file of type {\"csv\", \"txt\", \"public.comma-separated-values-text\", \"public.plain-text\"} with prompt \"选择 CSV 数据文件\")")
            .arg("-e")
            .arg("else")
            .arg("-e")
            .arg("POSIX path of (choose file with prompt \"选择文件\")")
            .arg("-e")
            .arg("end if")
            .arg("-e")
            .arg("end run")
            .arg("--")
            .arg(ftype);
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let mut path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if path.is_empty() {
        return None;
    }
    if mode == "save" {
        let ext = if ftype == "png" { ".png" } else { ".csv" };
        if !path.to_lowercase().ends_with(ext) {
            path.push_str(ext);
        }
    }
    Some(path)
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn linux_file_dialog(mode: &str, default_name: Option<&str>, filter_type: Option<&str>) -> Option<String> {
    let mut cmd = std::process::Command::new("zenity");
    cmd.arg("--file-selection");
    if mode == "save" {
        cmd.arg("--save").arg("--confirm-overwrite");
        if let Some(dn) = default_name {
            cmd.arg(format!("--filename={}", dn));
        }
    }
    match filter_type.unwrap_or("csv") {
        "png" => {
            cmd.arg("--file-filter=PNG Image (*.png) | *.png");
        }
        "csv" => {
            cmd.arg("--file-filter=CSV Data (*.csv, *.txt) | *.csv *.txt");
        }
        _ => {}
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if path.is_empty() { None } else { Some(path) }
}

#[cfg(target_os = "windows")]
fn win32_file_dialog(mode: &str, default_name: Option<&str>, filter_type: Option<&str>) -> Option<String> {
    #[repr(C)]
    #[allow(non_snake_case)]
    struct OPENFILENAMEW {
        lStructSize: u32,
        hwndOwner: usize,
        hInstance: usize,
        lpstrFilter: *const u16,
        lpstrCustomFilter: *mut u16,
        nMaxCustFilter: u32,
        nFilterIndex: u32,
        lpstrFile: *mut u16,
        nMaxFile: u32,
        lpstrFileTitle: *mut u16,
        nMaxFileTitle: u32,
        lpstrInitialDir: *const u16,
        lpstrTitle: *const u16,
        Flags: u32,
        nFileOffset: u16,
        nFileExtension: u16,
        lpstrDefExt: *const u16,
        lCustData: usize,
        lpfnHook: usize,
        lpTemplateName: *const u16,
        pvReserved: usize,
        dwReserved: u32,
        FlagsEx: u32,
    }

    #[link(name = "comdlg32")]
    extern "system" {
        fn GetOpenFileNameW(lpofn: *mut OPENFILENAMEW) -> i32;
        fn GetSaveFileNameW(lpofn: *mut OPENFILENAMEW) -> i32;
    }

    let filter_str = match filter_type {
        Some("json") => "JSON 预设文件 (*.json)\0*.json\0所有文件 (*.*)\0*.*\0\0",
        Some("png") => "PNG 分析图卡 (*.png)\0*.png\0所有文件 (*.*)\0*.*\0\0",
        _ => "CSV 波形数据 (*.csv)\0*.csv\0所有文件 (*.*)\0*.*\0\0",
    };
    let def_ext_str = match filter_type {
        Some("json") => "json\0",
        Some("png") => "png\0",
        _ => "csv\0",
    };

    let filter_w: Vec<u16> = filter_str.encode_utf16().collect();
    let def_ext_w: Vec<u16> = def_ext_str.encode_utf16().collect();
    let mut file_buf = vec![0u16; 1024];
    if let Some(dn) = default_name {
        for (i, ch) in dn.encode_utf16().take(500).enumerate() {
            file_buf[i] = ch;
        }
    }

    let init_dir_w: Option<Vec<u16>> = None;

    const OFN_PATHMUSTEXIST: u32 = 0x00000800;
    const OFN_FILEMUSTEXIST: u32 = 0x00001000;
    const OFN_OVERWRITEPROMPT: u32 = 0x00000002;
    const OFN_NOCHANGEDIR: u32 = 0x00000008;

    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: 0,
        hInstance: 0,
        lpstrFilter: filter_w.as_ptr(),
        lpstrCustomFilter: std::ptr::null_mut(),
        nMaxCustFilter: 0,
        nFilterIndex: 1,
        lpstrFile: file_buf.as_mut_ptr(),
        nMaxFile: file_buf.len() as u32,
        lpstrFileTitle: std::ptr::null_mut(),
        nMaxFileTitle: 0,
        lpstrInitialDir: init_dir_w
            .as_ref()
            .map(|v| v.as_ptr())
            .unwrap_or(std::ptr::null()),
        lpstrTitle: std::ptr::null(),
        Flags: if mode == "save" {
            OFN_PATHMUSTEXIST | OFN_OVERWRITEPROMPT | OFN_NOCHANGEDIR
        } else {
            OFN_PATHMUSTEXIST | OFN_FILEMUSTEXIST | OFN_NOCHANGEDIR
        },
        nFileOffset: 0,
        nFileExtension: 0,
        lpstrDefExt: def_ext_w.as_ptr(),
        lCustData: 0,
        lpfnHook: 0,
        lpTemplateName: std::ptr::null(),
        pvReserved: 0,
        dwReserved: 0,
        FlagsEx: 0,
    };

    let ok = unsafe {
        if mode == "save" {
            GetSaveFileNameW(&mut ofn)
        } else {
            GetOpenFileNameW(&mut ofn)
        }
    };
    if ok != 0 {
        let len = file_buf.iter().position(|&c| c == 0).unwrap_or(0);
        if len > 0 {
            return Some(String::from_utf16_lossy(&file_buf[..len]));
        }
    }
    None
}

fn parse_cli_csv_arg() -> Option<PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--csv" && i + 1 < args.len() {
            let p = PathBuf::from(&args[i + 1]);
            if p.is_file() {
                return Some(p);
            }
        } else if args[i].to_lowercase().ends_with(".csv") {
            let p = PathBuf::from(&args[i]);
            if p.is_file() {
                return Some(p);
            }
        }
        i += 1;
    }
    None
}

#[cfg(feature = "live-ui")]
fn resolve_live_ui_dir() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        for anc in exe.ancestors().skip(1).take(4) {
            let cand = anc.join("ui");
            if cand.join("index.html").is_file() {
                return Some(cand);
            }
        }
    }
    let fallback = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.join("ui"));
    fallback.filter(|d| d.join("index.html").is_file())
}

#[cfg(feature = "live-ui")]
fn start_live_ui_static_server(ui_dir: PathBuf) {
    std::thread::spawn(move || {
        let Ok(srv) = tiny_http::Server::http("127.0.0.1:18581") else {
            return;
        };
        for req in srv.incoming_requests() {
            let raw_url = req.url().split('?').next().unwrap_or("/");
            let rel = raw_url.trim_start_matches('/');
            let rel = if rel.is_empty() { "index.html" } else { rel };
            if rel.contains("..") {
                let _ = req.respond(tiny_http::Response::empty(403));
                continue;
            }
            let file_path = ui_dir.join(rel);
            match fs::read(&file_path) {
                Ok(bytes) => {
                    let ctype = if rel.ends_with(".html") {
                        "text/html; charset=utf-8"
                    } else if rel.ends_with(".css") {
                        "text/css; charset=utf-8"
                    } else if rel.ends_with(".js") {
                        "application/javascript; charset=utf-8"
                    } else if rel.ends_with(".svg") {
                        "image/svg+xml"
                    } else if rel.ends_with(".png") {
                        "image/png"
                    } else if rel.ends_with(".json") {
                        "application/json; charset=utf-8"
                    } else {
                        "application/octet-stream"
                    };
                    let mut resp = tiny_http::Response::from_data(bytes);
                    if let Ok(h) = tiny_http::Header::from_bytes(&b"Content-Type"[..], ctype.as_bytes()) {
                        resp.add_header(h);
                    }
                    if let Ok(h) = tiny_http::Header::from_bytes(
                        &b"Cache-Control"[..],
                        &b"no-store, no-cache, must-revalidate"[..],
                    ) {
                        resp.add_header(h);
                    }
                    let _ = req.respond(resp);
                }
                Err(_) => {
                    let _ = req.respond(tiny_http::Response::empty(404));
                }
            }
        }
    });
}

#[cfg(feature = "live-ui")]
fn snapshot_ui_mtimes(ui_dir: &Path) -> Vec<(PathBuf, std::time::SystemTime, u64)> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(ui_dir) {
        for ent in rd.flatten() {
            let p = ent.path();
            if p.is_file() {
                if let Ok(meta) = p.metadata() {
                    if let Ok(mt) = meta.modified() {
                        out.push((p, mt, meta.len()));
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[cfg(feature = "live-ui")]
fn start_live_ui_hot_reload_watcher(ui_dir: PathBuf, handle: AppHandle) {
    std::thread::spawn(move || {
        let mut last = snapshot_ui_mtimes(&ui_dir);
        loop {
            std::thread::sleep(Duration::from_millis(300));
            let cur = snapshot_ui_mtimes(&ui_dir);
            if cur != last {
                std::thread::sleep(Duration::from_millis(80));
                last = snapshot_ui_mtimes(&ui_dir);
                if let Some(w) = handle.get_webview_window("main") {
                    let _ = w.eval("window.location.reload()");
                }
            }
        }
    });
}

fn main() {
    if std::env::args().any(|a| a == "--mcp" || a == "--mcp-stdio") {
        run_stdio_mcp_bridge();
        return;
    }

    #[cfg(feature = "live-ui")]
    let live_ui_dir = {
        let d = resolve_live_ui_dir();
        if let Some(ref p) = d {
            start_live_ui_static_server(p.clone());
        }
        d
    };

    let profile = StudioProfile::load_or_default(None);
    let engine = TelemetryEngine::new(profile);

    if let Some(csv_p) = parse_cli_csv_arg() {
        let _ = engine.load_csv_instant(&csv_p, None, false);
    }

    let waiters: Arc<Mutex<Vec<Sender<Result<PathBuf, String>>>>> =
        Arc::new(Mutex::new(Vec::new()));

    let eng_setup = Arc::clone(&engine);
    let waiters_setup = Arc::clone(&waiters);

    tauri::Builder::default()
        .setup(move |app| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_title("FluxScope");
                if let Some(ic) = app.default_window_icon() {
                    let _ = w.set_icon(ic.clone());
                }
            }
            let handle: AppHandle = app.handle().clone();
            #[cfg(feature = "live-ui")]
            if let Some(ui_d) = live_ui_dir {
                start_live_ui_hot_reload_watcher(ui_d, handle.clone());
            }

            let handle_cmd = handle.clone();
            let handle_front = handle.clone();
            let handle_shot = handle.clone();
            let waiters_hook = Arc::clone(&waiters_setup);

            let hooks = Arc::new(GuiHooks {
                notify_ui: Box::new(move |kind: &str| {
                    let _ = handle.emit("mcp-updated", json!({ "kind": kind }));
                }),
                send_ui_cmd: Box::new(move |payload: Value| {
                    let _ = handle_cmd.emit("mcp-ui-cmd", payload);
                }),
                bring_to_front: Box::new(move || {
                    if let Some(w) = handle_front.get_webview_window("main") {
                        let _ = w.unminimize();
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }),
                capture_png: Box::new(move |target: &str| {
                    let (tx, rx) = mpsc::channel();
                    waiters_hook.lock().unwrap().push(tx);
                    let _ = handle_shot.emit("request-screenshot", json!({ "target": target }));
                    rx.recv_timeout(Duration::from_secs(5))
                        .map_err(|_| "截图响应超时".to_string())?
                }),
            });

            let mcp_ok = start_embedded_http_mcp(Arc::clone(&eng_setup), hooks);
            app.manage(AppState {
                engine: Arc::clone(&eng_setup),
                mcp_online: mcp_ok,
                screenshot_waiters: waiters_setup,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_bootstrap,
            get_viewport,
            get_readout_at,
            check_data_seq,
            save_profile,
            set_cursors_cmd,
            cut_slice_cmd,
            export_cursor_csv_cmd,
            get_overlay_data,
            update_slice_cmd,
            delete_slice_cmd,
            export_slice_csv_cmd,
            import_csv_slice_cmd,
            auto_align_slices_cmd,
            capture_full_slice_cmd,
            get_analysis_overview_cmd,
            get_analysis_run_cmd,
            save_png_file_cmd,
            compute_stats_and_fft,
            get_bookmarks_and_console,
            clear_console_cmd,
            serial_action,
            profile_action,
            submit_screenshot_data,
            pick_file_dialog,
        ])
        .run(tauri::generate_context!())
        .expect("Failed to start FluxScope");
}
