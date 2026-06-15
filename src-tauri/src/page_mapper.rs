//! 页码映射工具函数
//!
//! 原有的 `_middle.json` bbox 匹配算法已移除，改用 ocr_adapter::PageMapping 进行归一化页码映射。
//! 本模块仅保留公共工具函数：进度事件、日志、文本归一化。

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Emitter;
use unicode_normalization::UnicodeNormalization;

// =========================================================================
// 进度 & 日志
// =========================================================================

/// 导入进度事件载荷
#[derive(Clone, serde::Serialize)]
pub struct ImportProgress {
    pub stage: String,
    pub percent: u8,
    pub detail: String,
}

/// 发送进度事件到前端
pub fn emit_progress(app: &tauri::AppHandle, stage: &str, percent: u8, detail: &str) {
    let _ = app.emit("import-progress", ImportProgress {
        stage: stage.to_string(),
        percent,
        detail: detail.to_string(),
    });
}

/// 发送日志事件到前端（用于进度条下方的滚动日志），并可选写入项目内的 import.log
pub fn emit_log(app: &tauri::AppHandle, message: &str, log_path: Option<&Path>) {
    let timestamped = format!("[{}] {}", current_timestamp(), message);
    let _ = app.emit("import-log", timestamped.clone());
    eprintln!("{}", timestamped);
    if let Some(path) = log_path {
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).write(true).open(path) {
            let _ = writeln!(file, "{}", timestamped);
        }
    }
}

fn current_timestamp() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let secs = now.as_secs();
    let millis = now.subsec_millis();
    let days = secs / 86400;
    let day_secs = secs % 86400;
    let hours = day_secs / 3600;
    let mins = (day_secs % 3600) / 60;
    let secs_rem = day_secs % 60;
    let (year, month, day) = days_to_ymd(days as i64);
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}", year, month, day, hours, mins, secs_rem, millis)
}

fn days_to_ymd(days: i64) -> (i64, u8, u8) {
    let d = days;
    let era: i64 = if d >= 0 { d } else { d - 146096 } / 146097;
    let doe: u32 = (d - era * 146097) as u32;
    let yoe: u32 = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y: i64 = yoe as i64 + era * 400;
    let doy: u32 = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u8, d as u8)
}

pub fn emit_stage_progress(
    app: &tauri::AppHandle,
    stage: &str,
    index: usize,
    total: usize,
    min_percent: u8,
    max_percent: u8,
    detail: &str,
) {
    let percent = if total == 0 {
        max_percent
    } else {
        let ratio = ((index + 1) as f64 / total as f64).min(1.0);
        let value = min_percent as f64 + ratio * (max_percent.saturating_sub(min_percent)) as f64;
        value.round().clamp(min_percent as f64, max_percent as f64) as u8
    };
    emit_progress(app, stage, percent, detail);
}

// =========================================================================
// 文本归一化
// =========================================================================

/// 文本归一化: NFC + 合并空白符 + 去零宽字符 + trim
pub fn normalize_text(s: &str) -> String {
    let nfc: String = s.nfc().collect();
    let collapsed: String = nfc
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();
    let mut result = String::with_capacity(collapsed.len());
    let mut prev_space = false;
    for c in collapsed.chars() {
        if c == '\u{200B}' || c == '\u{200C}' || c == '\u{200D}'
            || c == '\u{FEFF}' || c == '\u{00AD}'
        {
            continue;
        }
        if c == ' ' {
            if !prev_space {
                result.push(' ');
                prev_space = true;
            }
        } else {
            result.push(c);
            prev_space = false;
        }
    }
    result.trim().to_string()
}