//! MinerU OCR 适配器
//!
//! 读取 MinerU 的输出文件:
//! - 优先使用 content_list_v2.json（有 bbox + 分页 + 富元数据）
//! - 降级使用 content_list.json（有 page_idx 但无 bbox）
//! - 补充读取 _middle.json 的 spans 数据（用于 SpanEntry 填充）

use std::collections::{HashMap, BTreeMap};
use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::{
    BlockEntry, BlockType, OcrAdapter, OcrSource, PageEntry, PageMapping, SpanEntry,
    map_mineru_block_type, parse_bbox,
};

// =========================================================================
// MinerU 数据结构
// =========================================================================

/// content_list_v2.json 的 block 条目
#[derive(Debug, Clone, Deserialize)]
struct ContentV2Block {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    content: Option<serde_json::Value>,
    #[serde(default)]
    bbox: Option<Vec<f64>>,
    #[serde(default)]
    level: Option<u8>,
}

/// content_list_v2.json 的顶层结构：数组 of 数组（每页一个子数组）
/// 格式: [[block1, block2, ...], [block1, block2, ...], ...]

/// content_list.json 的条目（扁平结构）
#[derive(Debug, Clone, Deserialize)]
struct ContentListItem {
    #[serde(rename = "type")]
    item_type: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    page_idx: usize,
}

// =========================================================================
// MinerUAdapter
// =========================================================================

/// MinerU OCR 适配器
pub struct MinerUAdapter;

impl OcrAdapter for MinerUAdapter {
    fn name(&self) -> &str {
        "mineru"
    }

    fn load_page_mapping(&self, assets_dir: &Path) -> Result<PageMapping, String> {
        // 策略: 优先 v2 → 降级 content_list → 补充 middle spans
        
        // 1. 尝试加载 content_list_v2.json
        let v2_path = super::super::project_manager::find_file_in_dir(assets_dir, |n| {
            n.contains("content_list") && n.contains("v2") && n.ends_with(".json")
        });
        
        let (pages, page_count) = if let Some(path) = v2_path {
            match load_content_v2(&path) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("[ocr_adapter/mineru] content_list_v2 加载失败: {}，尝试降级", e);
                    load_fallback_content_list(assets_dir)?
                }
            }
        } else {
            load_fallback_content_list(assets_dir)?
        };

        // 2. 尝试加载 _middle.json 补充 spans
        let middle_path = super::super::project_manager::find_file_in_dir(assets_dir, |n| {
            n.ends_with("_middle.json")
        });
        
        let mut mapping = PageMapping {
            source: OcrSource::MinerU,
            page_count,
            pages,
        };

        if let Some(path) = middle_path {
            enrich_with_spans(&mut mapping, &path);
        }

        Ok(mapping)
    }
}

/// 加载 content_list_v2.json
/// 格式: [[block1, block2, ...], [block1, block2, ...], ...]
/// 外层数组索引 = page_idx (0-based)
fn load_content_v2(path: &Path) -> Result<(Vec<PageEntry>, usize), String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("读取 content_list_v2.json 失败: {}", e))?;
    // 解析为嵌套数组: Vec<Vec<ContentV2Block>>
    let pages_data: Vec<Vec<ContentV2Block>> = serde_json::from_str(&content)
        .map_err(|e| format!("解析 content_list_v2.json 失败: {}", e))?;

    let mut pages = Vec::new();
    for (page_idx, blocks_data) in pages_data.into_iter().enumerate() {
        let mut blocks = Vec::new();
        for (idx, block_data) in blocks_data.iter().enumerate() {
            let block_type = map_mineru_block_type(&block_data.block_type);
            let bbox = block_data.bbox.as_ref()
                .and_then(|b| if b.len() >= 4 { Some([b[0], b[1], b[2], b[3]]) } else { None })
                .unwrap_or([0.0, 0.0, 0.0, 0.0]);

            // 从 content 对象中提取文本
            let text = extract_text_from_content(&block_data.content, &block_data.block_type);
            
            // 提取富元数据
            let mut metadata = HashMap::new();
            if let Some(ref c) = block_data.content {
                if let Some(html) = c.get("html").and_then(|v| v.as_str()) {
                    metadata.insert("html".to_string(), serde_json::Value::String(html.to_string()));
                }
                if let Some(captions) = c.get("captions") {
                    metadata.insert("captions".to_string(), captions.clone());
                }
                if let Some(image_source) = c.get("image_source").and_then(|v| v.as_str()) {
                    metadata.insert("image_source".to_string(), serde_json::Value::String(image_source.to_string()));
                }
            }

            blocks.push(BlockEntry {
                id: format!("{}_block_{}", page_idx, idx),
                block_type,
                bbox,
                text,
                level: block_data.level,
                metadata,
                spans: Vec::new(),
            });
        }

        pages.push(PageEntry {
            page_idx,
            page_size: [0.0, 0.0], // v2 没有 page_size，后续由 _middle.json 补充
            blocks,
        });
    }

    let page_count = pages.len();
    Ok((pages, page_count))
}

/// 降级加载 content_list.json
fn load_fallback_content_list(assets_dir: &Path) -> Result<(Vec<PageEntry>, usize), String> {
    let path = super::super::project_manager::find_file_in_dir(assets_dir, |n| {
        n.contains("content_list") && !n.contains("v2") && n.ends_with(".json")
    }).ok_or("未找到 content_list.json 或 content_list_v2.json")?;

    let content = fs::read_to_string(&path)
        .map_err(|e| format!("读取 content_list.json 失败: {}", e))?;
    let items: Vec<ContentListItem> = serde_json::from_str(&content)
        .map_err(|e| format!("解析 content_list.json 失败: {}", e))?;

    // 按 page_idx 分组
    let mut page_map: BTreeMap<usize, Vec<ContentListItem>> = BTreeMap::new();
    for item in items {
        page_map.entry(item.page_idx).or_default().push(item);
    }

    let mut pages = Vec::new();
    for (page_idx, items) in page_map {
        let mut blocks = Vec::new();
        for (idx, item) in items.iter().enumerate() {
            let text = item.text.as_deref()
                .or(item.content.as_deref())
                .unwrap_or("")
                .to_string();

            blocks.push(BlockEntry {
                id: format!("{}_block_{}", page_idx, idx),
                block_type: map_mineru_block_type(&item.item_type),
                bbox: [0.0, 0.0, 0.0, 0.0], // content_list 没有 bbox
                text,
                level: None,
                metadata: HashMap::new(),
                spans: Vec::new(),
            });
        }

        pages.push(PageEntry {
            page_idx,
            page_size: [0.0, 0.0],
            blocks,
        });
    }

    let page_count = pages.iter().map(|p| p.page_idx).max().unwrap_or(0) + 1;
    Ok((pages, page_count))
}

/// 从 content JSON 对象中提取文本
fn extract_text_from_content(content: &Option<serde_json::Value>, _block_type: &str) -> String {
    if let Some(c) = content {
        // 优先直接 text 字段
        if let Some(text) = c.get("text").and_then(|v| v.as_str()) {
            return text.to_string();
        }
        // 对于 table/image 等，用 captions
        if let Some(captions) = c.get("captions") {
            if let Some(arr) = captions.as_array() {
                let texts: Vec<String> = arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .filter(|s| !s.is_empty())
                    .collect();
                if !texts.is_empty() {
                    return texts.join(" ");
                }
            }
        }
        // 回退 html 中的文本
        if let Some(html) = c.get("html").and_then(|v| v.as_str()) {
            // 简单去除 HTML 标签
            let stripped: String = html.chars()
                .filter(|c| *c != '<' && *c != '>')
                .collect();
            if !stripped.trim().is_empty() {
                return stripped.trim().to_string();
            }
        }
    }
    String::new()
}

/// 用 _middle.json 的 spans 数据补充 PageMapping
fn enrich_with_spans(mapping: &mut PageMapping, middle_path: &Path) {
    let Ok(content) = fs::read_to_string(middle_path) else {
        eprintln!("[ocr_adapter/mineru] 无法读取 _middle.json");
        return;
    };
    
    let root: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[ocr_adapter/mineru] 解析 _middle.json 失败: {}", e);
            return;
        }
    };

    let pdf_info = match root.get("pdf_info").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => {
            eprintln!("[ocr_adapter/mineru] _middle.json 缺少 pdf_info");
            return;
        }
    };

    for page_data in pdf_info {
        let page_idx = match page_data.get("page_idx").and_then(|v| v.as_u64()) {
            Some(idx) => idx as usize,
            None => continue,
        };

        // 获取 page_size
        if let Some(page_size) = page_data.get("page_size").and_then(|v| v.as_array()) {
            if page_size.len() >= 2 {
                if let Some(page_entry) = mapping.pages.iter_mut().find(|p| p.page_idx == page_idx) {
                    page_entry.page_size = [
                        page_size[0].as_f64().unwrap_or(0.0),
                        page_size[1].as_f64().unwrap_or(0.0),
                    ];
                }
            }
        }

        let para_blocks = match page_data.get("para_blocks").and_then(|v| v.as_array()) {
            Some(arr) => arr,
            None => continue,
        };

        for (pb_idx, pb) in para_blocks.iter().enumerate() {
            let block_type = pb.get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("text");
            let block_bbox = parse_bbox(pb.get("bbox"));

            // 收集所有 spans
            let mut spans = Vec::new();
            let mut combined_text = String::new();

            if let Some(lines) = pb.get("lines").and_then(|v| v.as_array()) {
                for line in lines {
                    let line_bbox = parse_bbox(line.get("bbox"));
                    if let Some(line_spans) = line.get("spans").and_then(|v| v.as_array()) {
                        for span in line_spans {
                            let span_content = span.get("content")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            if span_content.is_empty() {
                                continue;
                            }

                            let span_bbox = parse_bbox(span.get("bbox"))
                                .or(line_bbox)
                                .or(block_bbox)
                                .unwrap_or([0.0, 0.0, 0.0, 0.0]);

                            spans.push(SpanEntry {
                                bbox: span_bbox,
                                content: span_content.to_string(),
                            });

                            if !combined_text.is_empty() {
                                combined_text.push(' ');
                            }
                            combined_text.push_str(span_content);
                        }
                    }
                }
            }

            // 找到对应的 block 并补充 spans
            if let Some(page_entry) = mapping.pages.iter_mut().find(|p| p.page_idx == page_idx) {
                if let Some(block) = page_entry.blocks.iter_mut().find(|b| {
                    b.id == format!("{}_block_{}", page_idx, pb_idx)
                        || matches_block_type(&b.block_type, block_type)
                }) {
                    if !spans.is_empty() {
                        block.spans = spans;
                    }
                    if !combined_text.is_empty() && block.text.is_empty() {
                        block.text = combined_text;
                    }
                    if let Some(bbox) = block_bbox {
                        if block.bbox == [0.0, 0.0, 0.0, 0.0] {
                            block.bbox = bbox;
                        }
                    }
                }
            }
        }
    }
}

/// 判断 block_type 是否匹配
fn matches_block_type(block: &BlockType, type_str: &str) -> bool {
    let mapped = map_mineru_block_type(type_str);
    *block == mapped
}