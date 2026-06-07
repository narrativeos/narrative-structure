//! 页码映射算法：_middle.json bbox spans → MD 行块页码标定
//!
//! 流程: flatten_bbox_spans → mark_header_footer_candidates → match_lines_to_pages → apply_bbox_page_mapping

use std::collections::HashMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::Emitter;
use unicode_normalization::UnicodeNormalization;

use crate::markdown_parser::ParsedBlock;

// =========================================================================
// 数据结构
// =========================================================================

/// 导入进度事件载荷
#[derive(Clone, serde::Serialize)]
pub struct ImportProgress {
    pub stage: String,
    pub percent: u8,
    pub detail: String,
}

/// 展平后的单个 bbox span（来自 _middle.json）
#[derive(Debug, Clone)]
pub struct BboxSpan {
    /// 页码（0-indexed）
    pub page_idx: usize,
    /// bbox 坐标 [x0, y0, x1, y1]
    pub bbox: [f64; 4],
    /// span 文本内容
    pub content: String,
    /// para_block 类型: title, text, image, table, etc.
    #[allow(dead_code)]
    pub block_type: String,
    /// 启发式标记：是否为疑似页头/页脚（可能被 MinerU 丢弃）
    pub is_candidate_discard: bool,
}

// =========================================================================
// 公开接口
// =========================================================================

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

fn emit_stage_progress(
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

/// 一站式页码映射: 读取 _middle.json → 展开 bbox → 标记页头页脚 → 匹配 → 写入 block metadata
pub fn apply_bbox_page_mapping(app: &tauri::AppHandle, middle_path: &Path, blocks: &mut [ParsedBlock]) {
    let Ok(mut spans) = flatten_bbox_spans(middle_path) else {
        eprintln!("[page-map] 无法展开 _middle.json bbox spans");
        return;
    };

    let span_count = spans.len();
    emit_progress(app, "匹配页码", 6, &format!("展开 {} 个 bbox span，标记页头页脚...", span_count));

    mark_header_footer_candidates(&mut spans);

    emit_progress(app, "匹配页码", 10, &format!("开始滑动窗口匹配 {} 行...", blocks.len()));

    let md_lines: Vec<String> = blocks.iter().map(|b| b.content.clone()).collect();
    let pages = match_lines_to_pages(app, &md_lines, &spans);

    let total = pages.len();
    let mut distinct_count = 0usize;
    let mut prev_page = 0usize;
    for &p in &pages {
        if p != prev_page {
            distinct_count += 1;
            prev_page = p;
        }
    }
    eprintln!(
        "[page-map] 共 {} 行, 匹配到 {} 个不同的页码 (首行: p{})",
        total, distinct_count,
        pages.first().copied().unwrap_or(1)
    );

    emit_progress(app, "匹配页码", 70, &format!("匹配完成，{} 个不同页码", distinct_count));

    for (block, page) in blocks.iter_mut().zip(pages.iter()) {
        block.metadata = format!("{{\"page\":{}}}", page);
    }
}

// =========================================================================
// bbox 展平
// =========================================================================

/// 将 _middle.json 的嵌套结构展开为扁平的 Vec<BboxSpan>
fn flatten_bbox_spans(json_path: &Path) -> Result<Vec<BboxSpan>, String> {
    let content = fs::read_to_string(json_path)
        .map_err(|e| format!("读取 middle.json 失败: {}", e))?;
    let root: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("解析 middle.json 失败: {}", e))?;
    let pdf_info = root.get("pdf_info")
        .and_then(|v| v.as_array())
        .ok_or("middle.json 缺少 pdf_info")?;

    let mut spans: Vec<BboxSpan> = Vec::new();

    for page in pdf_info {
        let page_idx = page.get("page_idx")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        if let Some(para_blocks) = page.get("para_blocks").and_then(|v| v.as_array()) {
            for pb in para_blocks {
                let block_type = pb.get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("text")
                    .to_string();
                let bbox = parse_bbox(pb.get("bbox"));

                if let Some(lines) = pb.get("lines").and_then(|v| v.as_array()) {
                    for line in lines {
                        let line_bbox = parse_bbox(line.get("bbox"));
                        if let Some(line_spans) = line.get("spans").and_then(|v| v.as_array()) {
                            for span in line_spans {
                                let content = span.get("content")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                if content.is_empty() {
                                    continue;
                                }
                                let span_bbox = parse_bbox(span.get("bbox"))
                                    .or(line_bbox)
                                    .or(bbox)
                                    .unwrap_or([0.0, 0.0, 0.0, 0.0]);

                                spans.push(BboxSpan {
                                    page_idx,
                                    bbox: span_bbox,
                                    content,
                                    block_type: block_type.clone(),
                                    is_candidate_discard: false,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(spans)
}

/// 解析 bbox JSON 数组 [x0, y0, x1, y1]
fn parse_bbox(val: Option<&serde_json::Value>) -> Option<[f64; 4]> {
    let arr = val?.as_array()?;
    if arr.len() < 4 {
        return None;
    }
    Some([
        arr[0].as_f64()?,
        arr[1].as_f64()?,
        arr[2].as_f64()?,
        arr[3].as_f64()?,
    ])
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

// =========================================================================
// 相似度计算（备用）
// =========================================================================

/// 计算两个字符串的 trigram Jaccard 相似度
#[allow(dead_code)]
pub fn trigram_jaccard(a: &str, b: &str) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    if a_chars.len() < 3 || b_chars.len() < 3 {
        let a_set: std::collections::HashSet<String> = a_chars.windows(2)
            .map(|w| w.iter().collect())
            .collect();
        let b_set: std::collections::HashSet<String> = b_chars.windows(2)
            .map(|w| w.iter().collect())
            .collect();
        if a_set.is_empty() && b_set.is_empty() {
            return if a == b { 1.0 } else { 0.0 };
        }
        let intersection = a_set.intersection(&b_set).count();
        let union = a_set.union(&b_set).count();
        return intersection as f64 / union as f64;
    }

    let a_grams: std::collections::HashSet<String> = a_chars.windows(3)
        .map(|w| w.iter().collect())
        .collect();
    let b_grams: std::collections::HashSet<String> = b_chars.windows(3)
        .map(|w| w.iter().collect())
        .collect();

    let intersection = a_grams.intersection(&b_grams).count();
    let union = a_grams.union(&b_grams).count();
    intersection as f64 / union as f64
}

/// 计算 md 行与 span 文本的匹配分数
fn similarity_score(norm_line: &str, norm_span: &str) -> f64 {
    if norm_line.is_empty() || norm_span.is_empty() {
        return 0.0;
    }
    if norm_line == norm_span {
        return 1.0;
    }
    if norm_line.contains(norm_span) {
        return 0.97;
    }
    let lnw: String = norm_line.chars().filter(|c| !c.is_whitespace()).collect();
    let snw: String = norm_span.chars().filter(|c| !c.is_whitespace()).collect();
    if !snw.is_empty() && lnw.contains(&snw) {
        return 0.92;
    }
    let base = trigram_jaccard(norm_line, norm_span);
    if base > 0.85 {
        0.9 + (base - 0.85) * 0.7
    } else {
        base
    }
}

/// 计算可用作匹配的 span 分数
fn span_match_score(
    norm_line: &str,
    norm_span: &str,
    span_len: usize,
    is_candidate_discard: bool,
    repeat_count: usize,
) -> f64 {
    let mut score = similarity_score(norm_line, norm_span);
    if score <= 0.0 {
        return 0.0;
    }
    let length_weight = (span_len as f64 / 40.0).min(0.35);
    score += length_weight;
    if is_candidate_discard {
        score *= 0.6;
    }
    if repeat_count > 1 {
        score *= 0.7_f64.powi((repeat_count as i32 - 1).min(4));
    }
    score
}

/// 模糊子串匹配（备用，当前算法使用预归一化后的 contains）
#[allow(dead_code)]
pub fn fuzzy_substring_match(line: &str, span_text: &str) -> bool {
    let norm_line = normalize_text(line);
    let norm_span = normalize_text(span_text);

    if norm_span.is_empty() {
        return false;
    }

    if norm_line.contains(&norm_span) {
        return true;
    }

    let line_chars: Vec<char> = norm_line.chars().collect();
    let span_len = norm_span.chars().count();

    if span_len == 0 || line_chars.len() < span_len {
        return false;
    }

    if span_len > line_chars.len() {
        return trigram_jaccard(&norm_line, &norm_span) >= 0.6;
    }

    let step = (span_len / 2).max(1);
    let threshold = 0.55;

    for start in (0..=line_chars.len() - span_len).step_by(step) {
        let window: String = line_chars[start..start + span_len].iter().collect();
        if trigram_jaccard(&window, &norm_span) >= threshold {
            return true;
        }
    }

    if line_chars.len() >= span_len {
        let last_start = line_chars.len() - span_len;
        if last_start > 0 && !last_start.is_multiple_of(step) {
            let window: String = line_chars[last_start..].iter().collect();
            if trigram_jaccard(&window, &norm_span) >= threshold {
                return true;
            }
        }
    }

    false
}

// =========================================================================
// 页头页脚标记
// =========================================================================

/// 启发式标记疑似被 MinerU 丢弃的页头/页脚 bbox
fn mark_header_footer_candidates(spans: &mut [BboxSpan]) {
    let mut page_heights: HashMap<usize, f64> = HashMap::new();
    for s in spans.iter() {
        let h = s.bbox[3] - s.bbox[1];
        page_heights
            .entry(s.page_idx)
            .and_modify(|max_h| { if h > *max_h { *max_h = h; } })
            .or_insert(h);
    }

    let mut page_max_y: HashMap<usize, f64> = HashMap::new();
    for s in spans.iter() {
        page_max_y
            .entry(s.page_idx)
            .and_modify(|max_y| { if s.bbox[3] > *max_y { *max_y = s.bbox[3]; } })
            .or_insert(s.bbox[3]);
    }

    let mut content_page_count: HashMap<String, usize> = HashMap::new();
    for s in spans.iter() {
        let norm = normalize_text(&s.content);
        if norm.len() >= 2 && norm.len() <= 50 {
            *content_page_count.entry(norm).or_insert(0) += 1;
        }
    }

    for s in spans.iter_mut() {
        let page_h = page_max_y.get(&s.page_idx).copied().unwrap_or(1000.0);
        if page_h <= 0.0 {
            continue;
        }

        let in_top = s.bbox[1] < page_h * 0.15;
        let in_bottom = s.bbox[3] > page_h * 0.85;
        if !in_top && !in_bottom {
            continue;
        }

        let norm_content = normalize_text(&s.content);
        if norm_content.len() > 30 {
            continue;
        }

        let repeat_count = content_page_count.get(&norm_content).copied().unwrap_or(1);
        let looks_like_page_num = norm_content.chars().all(|c| c.is_ascii_digit() || c == '-' || c == '.');

        if repeat_count >= 3 || looks_like_page_num {
            s.is_candidate_discard = true;
        }
    }
}

// =========================================================================
// 核心匹配算法
// =========================================================================

/// 两遍匹配 MD 行 → bbox spans → 页码
///
/// Pass 1: 滑动窗口匹配，不匹配的挂起（None）
/// Pass 2: 用前后已知页码插值回填挂起行
///
/// 返回与 md_lines 等长的 Vec<usize> (1-indexed 页码)。
fn match_lines_to_pages(
    app: &tauri::AppHandle,
    md_lines: &[String],
    spans: &[BboxSpan],
) -> Vec<usize> {
    let n = md_lines.len();
    let mut pages: Vec<Option<usize>> = vec![None; n];
    let mut span_cursor: usize = 0;

    // ---- 预归一化 ----
    let norm_lines: Vec<String> = md_lines.iter().map(|line| {
        let t = line.trim();
        let s = if t.starts_with('#') { t.trim_start_matches('#').trim() } else { t };
        normalize_text(s)
    }).collect();
    let norm_spans: Vec<String> = spans.iter()
        .map(|s| normalize_text(&s.content))
        .collect();

    let mut span_freq: HashMap<String, usize> = HashMap::new();
    for norm_span in norm_spans.iter() {
        *span_freq.entry(norm_span.clone()).or_insert(0) += 1;
    }
    let span_counts: Vec<usize> = norm_spans.iter().map(|s| *span_freq.get(s).unwrap_or(&1)).collect();

    let mut page_max_y: HashMap<usize, f64> = HashMap::new();
    for span in spans.iter() {
        page_max_y
            .entry(span.page_idx)
            .and_modify(|max_y| if span.bbox[3] > *max_y { *max_y = span.bbox[3]; })
            .or_insert(span.bbox[3]);
    }

    let max_skip: usize = 80;
    let log_interval = (n / 20).max(200);
    let progress_update_interval = (n / 80).max(50);
    let progress_stage = "匹配页码";

    // ===== Pass 1: 匹配 + 挂起 =====
    let mut last_page = 1usize;
    let mut last_top = 0.0;
    for (i, norm_line) in norm_lines.iter().enumerate() {
        if i % log_interval == 0 {
            eprintln!("[page-map] pass1 {}/{} (cursor={}/{})", i, n, span_cursor, spans.len());
        }
        if i % progress_update_interval == 0 || i + 1 == n {
            emit_stage_progress(app, progress_stage, i, n, 10, 70, &format!("pass1 匹配行 {}/{}", i + 1, n));
        }

        if norm_line.is_empty() {
            continue;
        }

        let trimmed = md_lines[i].trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut best_score = 0.0;
        let mut best_page: Option<usize> = None;
        let mut best_pos = span_cursor;
        let search_end = (span_cursor + max_skip).min(spans.len());

        for pos in span_cursor..search_end {
            let norm_span = &norm_spans[pos];
            let span = &spans[pos];
            if norm_span.is_empty() {
                continue;
            }

            let score = span_match_score(
                norm_line,
                norm_span,
                norm_span.len(),
                span.is_candidate_discard,
                span_counts[pos],
            );
            if score <= 0.0 {
                continue;
            }

            let dynamic_threshold = if norm_line.len() < 12 {
                0.44
            } else if norm_line.len() < 30 {
                0.52
            } else {
                0.58
            };

            if score < dynamic_threshold {
                continue;
            }

            let candidate_page = span.page_idx + 1;
            let page_gap = if candidate_page > last_page {
                candidate_page - last_page
            } else {
                last_page - candidate_page
            };
            let page_penalty = if candidate_page < last_page {
                0.55
            } else {
                1.0 - (page_gap as f64 * 0.08).min(0.35)
            };

            let span_distance = (pos.saturating_sub(span_cursor)) as f64;
            let window_size = (search_end.saturating_sub(span_cursor)).max(1) as f64;
            let position_penalty = 1.0 - (span_distance / window_size) * 0.35;

            let mut y_penalty = 1.0;
            let candidate_top = span.bbox[1];
            let page_height = page_max_y.get(&span.page_idx).copied().unwrap_or(1000.0);
            if candidate_page == last_page {
                if candidate_top >= last_top {
                    y_penalty += ((candidate_top - last_top) / page_height).min(0.15);
                } else {
                    y_penalty *= 0.65;
                }
            } else if candidate_page == last_page + 1 {
                if candidate_top > page_height * 0.3 {
                    y_penalty *= 0.85;
                } else {
                    y_penalty += 0.05;
                }
            } else if candidate_page > last_page + 1 {
                y_penalty *= 0.75;
            }

            let adjusted_score = score * page_penalty * position_penalty * y_penalty;
            if adjusted_score > best_score {
                best_score = adjusted_score;
                best_page = Some(span.page_idx);
                best_pos = pos;
            }
        }

        if let Some(page_idx) = best_page {
            pages[i] = Some(page_idx + 1);
            span_cursor = best_pos + 1;
            last_page = page_idx + 1;
            last_top = spans[best_pos].bbox[1];
        } else if span_cursor < spans.len() {
            span_cursor += 1;
        }
    }

    // ===== Pass 2: 最近邻插值回填挂起行 =====
    eprintln!("[page-map] pass2 最近邻回填 (cursor={}/{})", span_cursor, spans.len());
    emit_progress(app, "匹配页码", 70, "开始 pass2 最近邻回填...");

    let pass2_interval = (n / 20).max(50);
    for i in 0..n {
        if i % pass2_interval == 0 {
            emit_stage_progress(app, "匹配页码", i, n, 70, 90, &format!("pass2 回填行 {}/{}", i + 1, n));
        }
        if pages[i].is_some() {
            continue;
        }

        let mut before_idx = None;
        let mut before_page = 1usize;
        for j in (0..i).rev() {
            if let Some(p) = pages[j] {
                before_idx = Some(j);
                before_page = p;
                break;
            }
        }

        let mut after_idx = None;
        let mut after_page = before_page;
        for j in (i + 1)..n {
            if let Some(p) = pages[j] {
                after_idx = Some(j);
                after_page = p;
                break;
            }
        }

        pages[i] = Some(match (before_idx, after_idx) {
            (Some(bi), Some(ai)) => {
                let dist_before = i - bi;
                let dist_after = ai - i;
                if dist_before <= dist_after { before_page } else { after_page }
            }
            (Some(_), None) => before_page,
            (None, Some(_)) => after_page,
            _ => 1,
        });
    }

    let matched = pages.iter().filter(|p| p.is_some()).count();
    let result: Vec<usize> = pages.iter().map(|p| p.unwrap_or(1)).collect();
    eprintln!("[page-map] 完成: {} 行匹配, {} 行插值推断", matched, n - matched);
    result
}
