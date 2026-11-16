use crate::engine::{decimate_minmax, TelemetryEngine};
use rustfft::{num_complex::Complex, FftPlanner};
use serde_json::{json, Value};
use std::f64::consts::PI;

#[derive(Clone)]
pub struct AnalysisSeries {
    pub key: String,
    pub name: String,
    pub unit: String,
    pub data: Vec<f32>,
}

#[derive(Clone)]
pub struct AnalysisBundle {
    pub slice_id: String,
    pub name: String,
    pub note: String,
    pub hz: f64,
    pub raw_count: usize,
    pub offset_x: f64,
    pub offset_y: f64,
    pub t: Vec<f64>,
    pub channels: Vec<AnalysisSeries>,
}

fn r4(v: f64) -> f64 {
    if !v.is_finite() {
        0.0
    } else {
        (v * 10000.0).round() / 10000.0
    }
}

fn r2(v: f64) -> f64 {
    if !v.is_finite() {
        0.0
    } else {
        (v * 100.0).round() / 100.0
    }
}

pub fn load_bundle_typed(
    eng: &TelemetryEngine,
    slice_id: &str,
    max_samples: usize,
) -> Result<AnalysisBundle, String> {
    // Allow matching by slice_id OR slice name
    let resolved_id = if slice_id == "__live_buffer__" || slice_id.trim().is_empty() {
        "__live_buffer__".to_string()
    } else {
        let st = eng.store.read().unwrap();
        if let Some(found) = st
            .slices
            .iter()
            .find(|s| s.meta.slice_id == slice_id || s.meta.name == slice_id)
        {
            found.meta.slice_id.clone()
        } else {
            slice_id.to_string()
        }
    };

    let raw = eng.get_analysis_run_bundle(&resolved_id, max_samples)?;
    let t: Vec<f64> = raw
        .get("t")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|x| x.as_f64()).collect())
        .unwrap_or_default();

    let mut channels = Vec::new();
    if let Some(ch_arr) = raw.get("channels").and_then(|v| v.as_array()) {
        for c in ch_arr {
            let key = c.get("key").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let name = c.get("name").and_then(|v| v.as_str()).unwrap_or(&key).to_string();
            let unit = c.get("unit").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let data: Vec<f32> = c
                .get("data")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|x| x.as_f64().unwrap_or(0.0) as f32)
                        .collect()
                })
                .unwrap_or_default();
            channels.push(AnalysisSeries {
                key,
                name,
                unit,
                data,
            });
        }
    }

    Ok(AnalysisBundle {
        slice_id: raw
            .get("slice_id")
            .and_then(|v| v.as_str())
            .unwrap_or(&resolved_id)
            .to_string(),
        name: raw
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&resolved_id)
            .to_string(),
        note: raw
            .get("note")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        hz: raw.get("hz").and_then(|v| v.as_f64()).unwrap_or(1000.0),
        raw_count: raw.get("raw_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
        offset_x: raw.get("offset_x").and_then(|v| v.as_f64()).unwrap_or(0.0),
        offset_y: raw.get("offset_y").and_then(|v| v.as_f64()).unwrap_or(0.0),
        t,
        channels,
    })
}

pub fn find_series<'a>(
    eng: &TelemetryEngine,
    bundle: &'a AnalysisBundle,
    query: &str,
) -> Option<&'a AnalysisSeries> {
    let q = query.trim();
    if q.is_empty() {
        return bundle.channels.first();
    }
    if let Some(s) = bundle
        .channels
        .iter()
        .find(|c| c.key.eq_ignore_ascii_case(q) || c.name.eq_ignore_ascii_case(q))
    {
        return Some(s);
    }
    // Resolve CH{i} or M{i} via current profile alias
    let prof = eng.profile.read().unwrap();
    if let Some(stripped) = q.strip_prefix("CH").or_else(|| q.strip_prefix("ch")) {
        if let Ok(idx) = stripped.parse::<usize>() {
            if let Some(pc) = prof.channels.iter().find(|c| c.index == idx) {
                if let Some(s) = bundle
                    .channels
                    .iter()
                    .find(|c| c.key == pc.alias || c.name == pc.name)
                {
                    return Some(s);
                }
            }
            if idx < bundle.channels.len() {
                return Some(&bundle.channels[idx]);
            }
        }
    }
    if let Some(pm) = prof
        .math_channels
        .iter()
        .find(|m| m.id.eq_ignore_ascii_case(q) || m.alias.eq_ignore_ascii_case(q))
    {
        if let Some(s) = bundle
            .channels
            .iter()
            .find(|c| c.key == pm.alias || c.key == pm.id)
        {
            return Some(s);
        }
    }
    bundle.channels.iter().find(|c| c.name.contains(q))
}

/// 1. Step Response Dynamic Metrics (tr, tp, overshoot %, ts, steady-state ripple P-P, tracking RMSE)
pub fn compute_step_metrics(
    t: &[f64],
    fbk: &[f32],
    ref_opt: Option<&[f32]>,
    band_frac: f64,
) -> Value {
    let n = fbk.len().min(t.len());
    if n < 16 {
        return json!({ "valid": false, "reason": "采样点数不足 (< 16)" });
    }
    let n0 = (n * 6 / 100).max(3).min(n);
    let y0: f64 = fbk[..n0].iter().map(|&v| v as f64).sum::<f64>() / n0 as f64;

    let s_start = (n * 80 / 100).min(n - 1);
    let mut y_inf = 0.0f64;
    let mut ss_min = f64::INFINITY;
    let mut ss_max = f64::NEG_INFINITY;
    for &v in &fbk[s_start..n] {
        let vf = v as f64;
        y_inf += vf;
        ss_min = ss_min.min(vf);
        ss_max = ss_max.max(vf);
    }
    y_inf /= (n - s_start).max(1) as f64;

    let step_amp = y_inf - y0;
    if step_amp.abs() < 1e-4 {
        return json!({
            "valid": false,
            "reason": "未检测到显著阶跃幅值变化",
            "y0": r4(y0),
            "y_steady": r4(y_inf),
            "steady_ripple_p2p": r4(ss_max - ss_min),
        });
    }
    let sign = if step_amp >= 0.0 { 1.0 } else { -1.0 };

    let mut t0 = t[0];
    let mut used_ref = false;
    if let Some(ry) = ref_opt {
        if ry.len() >= n && (ry[n - 1] as f64 - ry[0] as f64).abs() > 1e-4 {
            used_ref = true;
            let r_mid = 0.5 * (ry[0] as f64 + ry[n - 1] as f64);
            for i in 1..n {
                let a = ry[i - 1] as f64 - r_mid;
                let b = ry[i] as f64 - r_mid;
                if a * b <= 0.0 && (ry[i] - ry[i - 1]).abs() > 1e-9 {
                    t0 = t[i];
                    break;
                }
            }
        }
    }
    if !used_ref {
        let th03 = y0 + 0.03 * step_amp;
        for i in 1..n {
            if sign * (fbk[i] as f64 - th03) >= 0.0 {
                t0 = t[i];
                break;
            }
        }
    }

    let y10 = y0 + 0.10 * step_amp;
    let y90 = y0 + 0.90 * step_amp;
    let mut t10: Option<f64> = None;
    let mut t90: Option<f64> = None;
    let mut peak_val = y0;
    let mut t_peak = t0;

    for i in 0..n {
        if t[i] < t0 {
            continue;
        }
        let yv = fbk[i] as f64;
        if t10.is_none() && sign * (yv - y10) >= 0.0 {
            t10 = Some(t[i]);
        }
        if t90.is_none() && sign * (yv - y90) >= 0.0 {
            t90 = Some(t[i]);
        }
        if sign * (yv - peak_val) > 0.0 {
            peak_val = yv;
            t_peak = t[i];
        }
    }

    let rise_ms = match (t10, t90) {
        (Some(a), Some(b)) => ((b - a) * 1000.0).max(0.0),
        _ => 0.0,
    };
    let peak_ms = ((t_peak - t0) * 1000.0).max(0.0);
    let overshoot_pct = (((peak_val - y_inf) / step_amp) * 100.0).max(0.0);

    let tol = step_amp.abs() * band_frac.clamp(0.005, 0.25);
    let mut last_out_idx = 0usize;
    for i in 0..n {
        if (fbk[i] as f64 - y_inf).abs() > tol {
            last_out_idx = i;
        }
    }
    let settle_ms = if last_out_idx < n.saturating_sub(3) {
        ((t[last_out_idx] - t0) * 1000.0).max(0.0)
    } else {
        ((t[n - 1] - t0) * 1000.0).max(0.0)
    };

    let mut sq_sum = 0.0f64;
    for i in 0..n {
        let target = ref_opt
            .filter(|r| r.len() >= n)
            .map(|r| r[i] as f64)
            .unwrap_or(y_inf);
        let d = fbk[i] as f64 - target;
        sq_sum += d * d;
    }
    let rmse = (sq_sum / n as f64).sqrt();

    json!({
        "valid": true,
        "y0": r4(y0),
        "y_steady": r4(y_inf),
        "step_amplitude": r4(step_amp),
        "t0_s": r4(t0),
        "rise_time_ms": r2(rise_ms),
        "peak_time_ms": r2(peak_ms),
        "peak_value": r4(peak_val),
        "overshoot_pct": r2(overshoot_pct),
        "settle_band_pct": r2(band_frac * 100.0),
        "settling_time_ms": r2(settle_ms),
        "steady_ripple_p2p": r4(ss_max - ss_min),
        "tracking_rmse": r4(rmse),
    })
}

/// 2. Windowed FFT + THD% + Electrical Harmonic Order Diagnosis (1fe, 2fe, 5fe, 6fe, 7fe, 1fm)
pub fn compute_harmonic_thd(
    samples: &[f32],
    hz: f64,
    win_type: &str,
    pole_pairs: u32,
    avg_rpm_opt: Option<f64>,
    include_spectrum: bool,
) -> Value {
    let mut p2 = 1usize;
    while p2 * 2 <= samples.len() && p2 < 16384 {
        p2 *= 2;
    }
    if p2 < 32 {
        return json!({
            "valid": false,
            "reason": "采样点数不足 (< 32)，无法执行有效 FFT",
        });
    }

    let start = samples.len() - p2;
    let slice = &samples[start..start + p2];
    let mean: f64 = slice.iter().map(|&x| x as f64).sum::<f64>() / p2 as f64;

    let mut buffer: Vec<Complex<f64>> = Vec::with_capacity(p2);
    let mut win_sum = 0.0f64;
    let denom = (p2 - 1).max(1) as f64;
    for (i, &val) in slice.iter().enumerate() {
        let phase = 2.0 * PI * (i as f64) / denom;
        let w = match win_type {
            "flattop" => {
                0.21557895 - 0.41663158 * phase.cos() + 0.277263158 * (2.0 * phase).cos()
                    - 0.083578947 * (3.0 * phase).cos()
                    + 0.006947368 * (4.0 * phase).cos()
            }
            "rect" => 1.0,
            _ => 0.5 * (1.0 - phase.cos()), // hann
        };
        win_sum += w;
        buffer.push(Complex::new((val as f64 - mean) * w, 0.0));
    }

    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(p2);
    fft.process(&mut buffer);

    let half = p2 / 2;
    let mut freqs = Vec::with_capacity(half);
    let mut amps = Vec::with_capacity(half);
    let norm = win_sum.max(1e-9);
    for k in 0..half {
        freqs.push(k as f64 * hz / p2 as f64);
        let mag = buffer[k].norm();
        amps.push(if k == 0 {
            mean.abs() as f32
        } else {
            ((2.0 * mag) / norm) as f32
        });
    }

    let mut raw_peaks: Vec<(usize, f64, f64)> = Vec::new();
    for k in 2..(half.saturating_sub(1)) {
        let a = amps[k] as f64;
        if a >= amps[k - 1] as f64 && a >= amps[k + 1] as f64 && a > 1e-6 {
            raw_peaks.push((k, freqs[k], a));
        }
    }
    raw_peaks.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    let mut thd_pct = 0.0f64;
    if let Some(&(f_idx, _, fund_amp)) = raw_peaks.first() {
        if fund_amp > 1e-5 {
            let mut harm_sq = 0.0f64;
            for k in 1..half {
                if (k as isize - f_idx as isize).unsigned_abs() > 2 {
                    let av = amps[k] as f64;
                    harm_sq += av * av;
                }
            }
            thd_pct = (harm_sq.sqrt() / fund_amp) * 100.0;
        }
    }

    let fm = avg_rpm_opt.unwrap_or(0.0).abs() / 60.0;
    let fe = fm * (pole_pairs.max(1) as f64);

    let explain = |f: f64| -> String {
        if fe > 1.5 {
            let eo = f / fe;
            if (eo - 1.0).abs() < 0.08 {
                return "1×fe 电基波分量".to_string();
            }
            if (eo - 2.0).abs() < 0.08 {
                return "2×fe 二次电谐波（常见于两相电流采样增益不对称或偏置残余）".to_string();
            }
            if (eo - 5.0).abs() < 0.12 || (eo - 7.0).abs() < 0.12 {
                return format!(
                    "{}×fe 特征谐波（逆变器死区非线性效应 / 定子反电势谐波）",
                    eo.round() as i32
                );
            }
            if (eo - 6.0).abs() < 0.12 {
                return "6×fe 六倍频脉动（定子 5·7 次谐波映射至 d-q 旋转轴转矩/电流脉动）".to_string();
            }
            if (eo - 12.0).abs() < 0.15 {
                return "12×fe 十二倍频脉动（二阶死区与高次空间谐波映射）".to_string();
            }
        }
        if fm > 0.5 && (f / fm - 1.0).abs() < 0.12 {
            return "1×fm 机械转频（联轴器对中偏差 / 机械负载偏心）".to_string();
        }
        if fe > 1.5 {
            format!("约 {:.2}×fe", f / fe)
        } else {
            format!("占采样率 {:.1}%", (f / hz.max(1.0)) * 100.0)
        }
    };

    let peaks_json: Vec<Value> = raw_peaks
        .iter()
        .take(10)
        .map(|&(_, f, a)| {
            json!({
                "freq_hz": r2(f),
                "amplitude": r4(a),
                "amp_db": r2(20.0 * a.max(1e-6).log10()),
                "diagnosis": explain(f),
            })
        })
        .collect();

    let mut out = json!({
        "valid": true,
        "fft_points": p2,
        "sample_rate_hz": r2(hz),
        "freq_resolution_hz": r4(hz / p2 as f64),
        "window_type": win_type,
        "dc_offset": r4(mean),
        "thd_pct": r2(thd_pct),
        "pole_pairs": pole_pairs.max(1),
        "estimated_mech_hz_fm": if fm > 0.1 { json!(r2(fm)) } else { Value::Null },
        "estimated_elec_hz_fe": if fe > 0.1 { json!(r2(fe)) } else { Value::Null },
        "top_peaks": peaks_json,
    });

    if include_spectrum {
        let (df, da) = if freqs.len() > 240 {
            decimate_minmax(&freqs, &amps, 120)
        } else {
            (freqs, amps)
        };
        out["spectrum_preview"] = json!({ "freqs_hz": df, "amps": da });
    }
    out
}

/// 3. Offline Digital Filter Sandbox + MCU C Code Generator
pub fn evaluate_digital_filter(
    samples: &[f32],
    hz: f64,
    filter_type: &str,
    param: f64,
) -> Value {
    let n = samples.len();
    if n < 8 {
        return json!({ "valid": false, "reason": "采样点数不足 (< 8)" });
    }
    let dt = 1.0 / hz.max(1.0);
    let mut out = vec![0.0f32; n];
    let delay_ms: f64;
    let c_code: String;

    match filter_type {
        "biquad_lpf" => {
            let fc = param.max(0.5).min(hz * 0.48);
            let w0 = (2.0 * PI * fc) / hz.max(1.0);
            let cosw = w0.cos();
            let sinw = w0.sin();
            let alpha = sinw / (2.0 * 0.70710678);
            let a0 = 1.0 + alpha;
            let b0 = (1.0 - cosw) * 0.5 / a0;
            let b1 = (1.0 - cosw) / a0;
            let b2 = b0;
            let a1 = (-2.0 * cosw) / a0;
            let a2 = (1.0 - alpha) / a0;
            let mut x1 = samples[0] as f64;
            let mut x2 = x1;
            let mut y1 = x1;
            let mut y2 = x1;
            for i in 0..n {
                let x0 = samples[i] as f64;
                let y0 = b0 * x0 + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
                out[i] = y0 as f32;
                x2 = x1;
                x1 = x0;
                y2 = y1;
                y1 = y0;
            }
            delay_ms = (0.225 / fc) * 1000.0;
            c_code = format!(
                "// 二阶 Butterworth 低通滤波器 (fs = {:.0}Hz, fc = {:.1}Hz, Q = 0.7071)\nconst float b0 = {:.6}f, b1 = {:.6}f, b2 = {:.6}f;\nconst float a1 = {:.6}f, a2 = {:.6}f;\nstatic float x1 = 0.0f, x2 = 0.0f, y1 = 0.0f, y2 = 0.0f;\nfloat y0 = b0*x0 + b1*x1 + b2*x2 - a1*y1 - a2*y2;\nx2 = x1; x1 = x0; y2 = y1; y1 = y0;",
                hz, fc, b0, b1, b2, a1, a2
            );
        }
        "movavg" => {
            let win = (param.round() as usize).clamp(2, 256);
            let mut sum = 0.0f64;
            for i in 0..n {
                sum += samples[i] as f64;
                if i >= win {
                    sum -= samples[i - win] as f64;
                }
                let denom = (i + 1).min(win) as f64;
                out[i] = (sum / denom) as f32;
            }
            delay_ms = ((win - 1) as f64 * 0.5) * dt * 1000.0;
            c_code = format!(
                "// 滑动平均窗滤波器 (窗口 N = {})\n#define MAV_N {}\nstatic float buf[MAV_N] = {{0}}; static uint16_t idx = 0; static float acc = 0.0f;\nacc += raw_x - buf[idx];\nbuf[idx] = raw_x;\nif (++idx >= MAV_N) idx = 0;\nfloat y_filt = acc * {:.6}f;",
                win,
                win,
                1.0 / (win as f64)
            );
        }
        "notch" => {
            let f0 = param.max(1.0).min(hz * 0.48);
            let q = 4.0;
            let w0 = (2.0 * PI * f0) / hz.max(1.0);
            let cosw = w0.cos();
            let alpha = w0.sin() / (2.0 * q);
            let a0 = 1.0 + alpha;
            let b0 = 1.0 / a0;
            let b1 = (-2.0 * cosw) / a0;
            let b2 = 1.0 / a0;
            let a1 = b1;
            let a2 = (1.0 - alpha) / a0;
            let mut x1 = samples[0] as f64;
            let mut x2 = x1;
            let mut y1 = x1;
            let mut y2 = x1;
            for i in 0..n {
                let x0 = samples[i] as f64;
                let y0 = b0 * x0 + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
                out[i] = y0 as f32;
                x2 = x1;
                x1 = x0;
                y2 = y1;
                y1 = y0;
            }
            delay_ms = (1.0 / (PI * (f0 / q))) * 100.0;
            c_code = format!(
                "// 二阶陷波器 Notch Filter (fs = {:.0}Hz, 中心抑制 f0 = {:.1}Hz, Q = 4.0)\nconst float b0 = {:.6}f, b1 = {:.6}f, b2 = {:.6}f, a1 = {:.6}f, a2 = {:.6}f;\nstatic float x1 = 0.0f, x2 = 0.0f, y1 = 0.0f, y2 = 0.0f;\nfloat y0 = b0*x0 + b1*x1 + b2*x2 - a1*y1 - a2*y2;\nx2 = x1; x1 = x0; y2 = y1; y1 = y0;",
                hz, f0, b0, b1, b2, a1, a2
            );
        }
        "zerophase" => {
            let fc = param.max(0.5).min(hz * 0.48);
            let rc = 1.0 / (2.0 * PI * fc);
            let alpha = dt / (rc + dt);
            let mut fwd = vec![0.0f64; n];
            let mut y = samples[0] as f64;
            for i in 0..n {
                y += alpha * (samples[i] as f64 - y);
                fwd[i] = y;
            }
            let mut yb = fwd[n - 1];
            for i in (0..n).rev() {
                yb += alpha * (fwd[i] - yb);
                out[i] = yb as f32;
            }
            delay_ms = 0.0;
            c_code = "// 零相位双向滤波 (Forward-Backward FiltFilt)\n// 仅适用于上位机离线获取无相位滞后的理想真值基准，实时下位机闭环请选用 iir1 或 biquad_lpf".to_string();
        }
        _ => {
            // Default: iir1
            let fc = param.max(0.5).min(hz * 0.48);
            let rc = 1.0 / (2.0 * PI * fc);
            let alpha = dt / (rc + dt);
            let mut y = samples[0] as f64;
            for i in 0..n {
                y += alpha * (samples[i] as f64 - y);
                out[i] = y as f32;
            }
            delay_ms = rc * 1000.0;
            c_code = format!(
                "// 一阶低通 IIR 滤波器 (fs = {:.0}Hz, fc = {:.1}Hz)\n// 等效一阶时间常数 tau = {:.2} ms\nstatic float y_lpf = 0.0f;\nconst float alpha = {:.6}f;\ny_lpf += alpha * (raw_x - y_lpf);",
                hz,
                fc,
                rc * 1000.0,
                alpha
            );
        }
    }

    let s0 = (n * 70 / 100).min(n - 1);
    let mut r_min = f64::INFINITY;
    let mut r_max = f64::NEG_INFINITY;
    let mut f_min = f64::INFINITY;
    let mut f_max = f64::NEG_INFINITY;
    let mut diff_sq = 0.0f64;
    for i in 0..n {
        let rv = samples[i] as f64;
        let fv = out[i] as f64;
        diff_sq += (rv - fv) * (rv - fv);
        if i >= s0 {
            r_min = r_min.min(rv);
            r_max = r_max.max(rv);
            f_min = f_min.min(fv);
            f_max = f_max.max(fv);
        }
    }
    let raw_pp = (r_max - r_min).max(0.0);
    let filt_pp = (f_max - f_min).max(0.0);
    let atten_pct = if raw_pp > 1e-6 {
        ((1.0 - filt_pp / raw_pp) * 100.0).max(0.0)
    } else {
        0.0
    };

    json!({
        "valid": true,
        "filter_type": filter_type,
        "param": r2(param),
        "sample_rate_hz": r2(hz),
        "raw_steady_p2p": r4(raw_pp),
        "filtered_steady_p2p": r4(filt_pp),
        "ripple_attenuation_pct": r2(atten_pct),
        "group_delay_ms": r2(delay_ms),
        "removed_noise_rms": r4((diff_sq / n as f64).sqrt()),
        "mcu_c_code": c_code,
    })
}

/// 4. Auto-Alignment & Residual Difference (Overlay Tab)
pub fn compute_auto_shift_sec(
    t: &[f64],
    y: &[f32],
    manual_dt: f64,
    align_mode: &str,
    thresh: f64,
) -> f64 {
    let n = t.len().min(y.len());
    if align_mode == "manual" || n < 4 {
        return manual_dt;
    }
    let mut min_v = f64::INFINITY;
    let mut max_v = f64::NEG_INFINITY;
    let mut max_idx = 0usize;
    for i in 0..n {
        let v = y[i] as f64;
        if v < min_v {
            min_v = v;
        }
        if v > max_v {
            max_v = v;
            max_idx = i;
        }
    }
    if align_mode == "peak" {
        return -t[max_idx] + manual_dt;
    }
    let target = if align_mode == "threshold" {
        thresh
    } else {
        0.5 * (min_v + max_v)
    };
    for i in 1..n {
        let y0 = y[i - 1] as f64;
        let y1 = y[i] as f64;
        if (y0 < target && y1 >= target) || (y0 > target && y1 <= target) {
            let denom = if (y1 - y0).abs() > 1e-12 {
                y1 - y0
            } else {
                1.0
            };
            let frac = (target - y0) / denom;
            let t_cross = t[i - 1] + frac * (t[i] - t[i - 1]);
            return -t_cross + manual_dt;
        }
    }
    manual_dt
}

pub fn compute_overlay_diff(
    t_a: &[f64],
    y_a: &[f32],
    shift_a: f64,
    t_b: &[f64],
    y_b: &[f32],
    shift_b: f64,
    norm_100: bool,
) -> Value {
    let na = t_a.len().min(y_a.len());
    let nb = t_b.len().min(y_b.len());
    if na < 2 || nb < 2 {
        return json!({ "valid": false, "reason": "曲线点数不足" });
    }
    let map_norm = |arr: &[f32]| -> Vec<f64> {
        if !norm_100 {
            return arr.iter().map(|&v| v as f64).collect();
        }
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for &v in arr {
            lo = lo.min(v as f64);
            hi = hi.max(v as f64);
        }
        let span = (hi - lo).max(1e-6);
        arr.iter().map(|&v| ((v as f64 - lo) / span) * 100.0).collect()
    };

    let va = map_norm(&y_a[..na]);
    let vb = map_norm(&y_b[..nb]);
    let xa: Vec<f64> = t_a[..na].iter().map(|&t| t + shift_a).collect();
    let xb: Vec<f64> = t_b[..nb].iter().map(|&t| t + shift_b).collect();

    let mut max_abs = 0.0f64;
    let mut sum_signed = 0.0f64;
    let mut sq = 0.0f64;
    for i in 0..na {
        let tx = xa[i];
        let pos = xb.partition_point(|&x| x < tx);
        let idx_b = if pos == 0 {
            0
        } else if pos >= nb {
            nb - 1
        } else if (xb[pos - 1] - tx).abs() <= (xb[pos] - tx).abs() {
            pos - 1
        } else {
            pos
        };
        let d = va[i] - vb[idx_b];
        max_abs = max_abs.max(d.abs());
        sum_signed += d;
        sq += d * d;
    }
    json!({
        "valid": true,
        "shift_a_ms": r2(shift_a * 1000.0),
        "shift_b_ms": r2(shift_b * 1000.0),
        "normalized_0_100_pct": norm_100,
        "max_abs_diff": r4(max_abs),
        "mean_signed_diff": r4(sum_signed / na as f64),
        "rmse_diff": r4((sq / na as f64).sqrt()),
    })
}

/// 5. X-Y Phase Portrait / Envelope Statistics
pub fn compute_xy_envelope(x: &[f32], y: &[f32], x_label: &str, y_label: &str) -> Value {
    let n = x.len().min(y.len());
    if n < 4 {
        return json!({ "valid": false, "reason": "采样点不足 (< 4)" });
    }
    let mut x_min = f64::INFINITY;
    let mut x_max = f64::NEG_INFINITY;
    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;
    let mut r_min = f64::INFINITY;
    let mut r_max = 0.0f64;
    let mut r_sum = 0.0f64;

    for i in 0..n {
        let xv = x[i] as f64;
        let yv = y[i] as f64;
        x_min = x_min.min(xv);
        x_max = x_max.max(xv);
        y_min = y_min.min(yv);
        y_max = y_max.max(yv);
        let r = xv.hypot(yv);
        r_min = r_min.min(r);
        r_max = r_max.max(r);
        r_sum += r;
    }
    let r_mean = r_sum / n as f64;
    let x_center = 0.5 * (x_min + x_max);
    let y_center = 0.5 * (y_min + y_max);
    let aspect_ratio = if (y_max - y_min).abs() > 1e-6 {
        (x_max - x_min) / (y_max - y_min)
    } else {
        1.0
    };

    json!({
        "valid": true,
        "x_channel": x_label,
        "y_channel": y_label,
        "samples": n,
        "x_range": [r4(x_min), r4(x_max)],
        "y_range": [r4(y_min), r4(y_max)],
        "x_p2p": r4(x_max - x_min),
        "y_p2p": r4(y_max - y_min),
        "center_offset": [r4(x_center), r4(y_center)],
        "xy_span_aspect_ratio": r4(aspect_ratio),
        "vector_magnitude_max": r4(r_max),
        "vector_magnitude_min": r4(r_min),
        "vector_magnitude_mean": r4(r_mean),
        "magnitude_ripple_pct": if r_mean > 1e-6 { r2(((r_max - r_min) / r_mean) * 100.0) } else { 0.0 },
    })
}

/// 6. Full-Channel Statistical Matrix
pub fn compute_full_stats(bundle: &AnalysisBundle) -> Value {
    let rows: Vec<Value> = bundle
        .channels
        .iter()
        .map(|ch| {
            let n = ch.data.len();
            if n == 0 {
                return json!({ "key": ch.key, "name": ch.name, "unit": ch.unit, "samples": 0 });
            }
            let mut sum = 0.0f64;
            let mut sq = 0.0f64;
            let mut mn = f64::INFINITY;
            let mut mx = f64::NEG_INFINITY;
            for &v in &ch.data {
                let vf = v as f64;
                sum += vf;
                sq += vf * vf;
                mn = mn.min(vf);
                mx = mx.max(vf);
            }
            let nf = n as f64;
            let mean = sum / nf;
            let rms = (sq / nf).sqrt();
            let std_dev = (sq / nf - mean * mean).max(0.0).sqrt();
            json!({
                "key": ch.key,
                "name": ch.name,
                "unit": ch.unit,
                "mean": r4(mean),
                "rms": r4(rms),
                "min": r4(mn),
                "max": r4(mx),
                "p2p": r4(mx - mn),
                "std_dev": r4(std_dev),
            })
        })
        .collect();

    json!({
        "slice_id": bundle.slice_id,
        "name": bundle.name,
        "note": bundle.note,
        "sample_rate_hz": r2(bundle.hz),
        "samples": bundle.t.len(),
        "channels": rows,
    })
}
