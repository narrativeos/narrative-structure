//! 页码映射算法：_middle.json bbox spans → MD 行块页码标定
//!
//! 流程: flatten_bbox_spans → mark_header_footer_candidates → match_lines_to_pages → apply_bbox_page_mapping

use std::collections::HashMap;
use std::fs;
use std::path::Path;

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

/// 发送日志事件到前端（用于进度条下方的滚动日志）
pub fn emit_log(app: &tauri::AppHandle, message: &str) {
    let _ = app.emit("import-log", message.to_string());
    eprintln!("{}", message);
}

/// 一站式页码映射: 读取 _middle.json → 展开 bbox → 标记页头页脚 → 匹配 → 写入 block metadata
pub fn apply_bbox_page_mapping(app: &tauri::AppHandle, middle_path: &Path, blocks: &mut [ParsedBlock]) {
    let Ok(mut spans) = flatten_bbox_spans(middle_path) else {
        eprintln!("[page-map] 无法展开 _middle.json bbox spans");
        return;
    };

    let span_count = spans.len();
    emit_progress(app, "匹配页码", 45, &format!("展开 {} 个 bbox span，标记页头页脚...", span_count));

    mark_header_footer_candidates(&mut spans);

    emit_progress(app, "匹配页码", 55, &format!("开始滑动窗口匹配 {} 行...", blocks.len()));

    let md_lines: Vec<String> = blocks.iter().map(|b| b.content.clone()).collect();
    let pages = match_lines_to_pages(&md_lines, &spans);

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

    emit_progress(app, "匹配页码", 80, &format!("匹配完成，{} 个不同页码", distinct_count));

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
                                    .or_else(|| line_bbox)
                                    .or_else(|| bbox)
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
        if last_start > 0 && last_start % step != 0 {
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

    let max_skip: usize = 50;
    let log_interval = (n / 20).max(500);

    // ===== Pass 1: 匹配 + 挂起 =====
    for (i, (md_line, norm_line)) in md_lines.iter().zip(norm_lines.iter()).enumerate() {
        if i % log_interval == 0 {
            eprintln!("[page-map] pass1 {}/{} (cursor={}/{})", i, n, span_cursor, spans.len());
        }

        let trimmed = md_line.trim();
        if trimmed.is_empty() || norm_line.is_empty() {
            continue; // 保持 None，pass2 插值
        }

        let mut matched_pages: Vec<usize> = Vec::new();
        let mut last_matched_pos: usize = span_cursor;
        let search_end = (span_cursor + max_skip).min(spans.len());

        for pos in span_cursor..search_end {
            let norm_span = &norm_spans[pos];
            let span = &spans[pos];

            let mut m = norm_line.contains(norm_span.as_str());
            if !m && norm_span.len() >= 3 {
                let lnw: String = norm_line.chars().filter(|c| !c.is_whitespace()).collect();
                let snw: String = norm_span.chars().filter(|c| !c.is_whitespace()).collect();
                m = lnw.contains(&snw);
            }

            if span.is_candidate_discard && !m {
                continue;
            }

            if m {
                matched_pages.push(span.page_idx);
                last_matched_pos = pos;
                if matched_pages.len() >= 5 {
                    break;
                }
            }
        }

        if !matched_pages.is_empty() {
            let page = matched_pages.iter().min().copied().unwrap() + 1;
            pages[i] = Some(page);
            span_cursor = last_matched_pos + 1;
        } else {
            span_cursor += 1; // 挂起，cursor 缓步前进
        }
    }

    // ===== Pass 2: 插值回填挂起行 =====
    eprintln!("[page-map] pass2 插值回填 (cursor={}/{})", span_cursor, spans.len());

    for i in 0..n {
        if pages[i].is_some() {
            continue;
        }

        let mut before = 1usize;
        for j in (0..i).rev() {
            if let Some(p) = pages[j] { before = p; break; }
        }

        let mut after = before;
        for j in (i + 1)..n {
            if let Some(p) = pages[j] { after = p; break; }
        }

        pages[i] = Some(before.min(after).max(1));
    }

    let result: Vec<usize> = pages.iter().map(|p| p.unwrap_or(1)).collect();

    let matched = pages.iter().filter(|p| p.is_some()).count();
    eprintln!("[page-map] 完成: {} 行匹配, {} 行插值推断", matched, n - matched);
    result
}
