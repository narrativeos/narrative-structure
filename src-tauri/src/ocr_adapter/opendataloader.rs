//! OpenDataLoader (Docling) OCR 适配器
//!
//! 读取 DoclingDocument JSON 输出:
//! - kids 数组（按页组织）
//! - 坐标系统: [left, bottom, right, top] (左下原点，PDF 标准)
//! - 页码: 1-indexed
//! - 支持的类型: paragraph, heading, caption, table, text block, list, image, header/footer

use std::collections::{HashMap, BTreeMap};
use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::{
    BlockEntry, BlockType, OcrAdapter, OcrSource, PageEntry, PageMapping,
    map_docling_block_type, parse_bbox, convert_docling_bbox,
};

/// Docling 的顶层输出
#[derive(Debug, Clone, Deserialize)]
struct DoclingDocument {
    #[serde(rename = "number of pages", default)]
    number_of_pages: usize,
    #[serde(default)]
    kids: Vec<serde_json::Value>,
    #[serde(default)]
    pages: Vec<serde_json::Value>,
}

// =========================================================================
// OpenDataLoaderAdapter
// =========================================================================

/// OpenDataLoader (Docling) OCR 适配器
pub struct OpenDataLoaderAdapter;

impl OcrAdapter for OpenDataLoaderAdapter {
    fn name(&self) -> &str {
        "opendataloader"
    }

    fn load_page_mapping(&self, assets_dir: &Path) -> Result<PageMapping, String> {
        // 查找 Docling JSON 文件
        let docling_path = super::super::project_manager::find_file_in_dir(assets_dir, |n| {
            n == "docling.json" || n.ends_with("_docling.json")
        }).ok_or("未找到 Docling JSON 文件")?;

        let content = fs::read_to_string(&docling_path)
            .map_err(|e| format!("读取 Docling JSON 失败: {}", e))?;
        
        let doc: DoclingDocument = serde_json::from_str(&content)
            .map_err(|e| format!("解析 Docling JSON 失败: {}", e))?;

        // 尝试从 pages 信息中获取页面尺寸
        let mut page_sizes: HashMap<usize, [f64; 2]> = HashMap::new();
        if let Some(pages_arr) = doc.pages.first().and_then(|v| v.get("pages").and_then(|v| v.as_array())) {
            for page_obj in pages_arr {
                if let Some(page_num) = page_obj.get("page number").and_then(|v| v.as_u64()) {
                    let page_num = page_num as usize;
                    if let Some(size) = page_obj.get("page size").and_then(|v| v.as_array()) {
                        if size.len() >= 2 {
                            page_sizes.insert(page_num, [
                                size[0].as_f64().unwrap_or(0.0),
                                size[1].as_f64().unwrap_or(0.0),
                            ]);
                        }
                    }
                }
            }
        }

        // 解析 kids 数组，按页分组
        let mut page_map: BTreeMap<usize, Vec<BlockEntry>> = BTreeMap::new();

        for kid_value in &doc.kids {
            parse_docling_element(kid_value, &page_sizes, &mut page_map);
        }

        // 转换为 PageMapping
        let mut pages = Vec::new();
        for (page_num_1idx, blocks) in page_map {
            let page_idx = page_num_1idx.saturating_sub(1); // 转为 0-indexed
            let page_size = page_sizes.get(&page_num_1idx).copied().unwrap_or([0.0, 0.0]);

            pages.push(PageEntry {
                page_idx,
                page_size,
                blocks,
            });
        }

        let page_count = doc.number_of_pages.max(pages.len());

        Ok(PageMapping {
            source: OcrSource::OpenDataLoader,
            page_count,
            pages,
        })
    }
}

/// 递归解析 Docling 元素
fn parse_docling_element(
    value: &serde_json::Value,
    page_sizes: &HashMap<usize, [f64; 2]>,
    page_map: &mut BTreeMap<usize, Vec<BlockEntry>>,
) {
    // 提取基本字段
    let element_type = value.get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("paragraph")
        .to_string();
    
    let page_number = value.get("page number")
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as usize;

    let bounding_box = parse_bbox(value.get("bounding box")).unwrap_or([0.0, 0.0, 0.0, 0.0]);
    let heading_level = value.get("heading level").and_then(|v| v.as_u64()).map(|v| v as u8);
    let content = value.get("content").and_then(|v| v.as_str()).map(|s| s.to_string());
    let id = value.get("id").and_then(|v| v.as_u64()).map(|v| v as usize);

    // 获取页面高度用于坐标转换
    let page_height = page_sizes.get(&page_number).map(|s| s[1]).unwrap_or(1000.0);
    
    // 转换坐标系统：Docling [left, bottom, right, top] → [x0, y0, x1, y1]
    let bbox = convert_docling_bbox(&bounding_box, page_height);

    let block_type = map_docling_block_type(&element_type);

    // 生成唯一 ID
    let id_str = id.map(|i| format!("docling_{}", i))
        .unwrap_or_else(|| format!("docling_p{}_{}", page_number, page_map.len()));

    // 构建元数据
    let mut metadata = HashMap::new();
    if let Some(font) = value.get("font").and_then(|v| v.as_str()) {
        metadata.insert("font".to_string(), serde_json::Value::String(font.to_string()));
    }
    if let Some(font_size) = value.get("font size").and_then(|v| v.as_f64()) {
        metadata.insert("font_size".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(font_size).unwrap()));
    }
    if let Some(text_color) = value.get("text color").and_then(|v| v.as_str()) {
        metadata.insert("text_color".to_string(), serde_json::Value::String(text_color.to_string()));
    }
    if let Some(source) = value.get("source").and_then(|v| v.as_str()) {
        metadata.insert("image_source".to_string(), serde_json::Value::String(source.to_string()));
    }
    if let Some(data) = value.get("data").and_then(|v| v.as_str()) {
        metadata.insert("image_data".to_string(), serde_json::Value::String(data.to_string()));
    }
    if let Some(format) = value.get("format").and_then(|v| v.as_str()) {
        metadata.insert("image_format".to_string(), serde_json::Value::String(format.to_string()));
    }
    if let Some(linked_id) = value.get("linked content id").and_then(|v| v.as_u64()) {
        metadata.insert("linked_content_id".to_string(), serde_json::Value::Number(serde_json::Number::from(linked_id as u64)));
    }

    // 处理特殊类型的文本提取
    let text = match block_type {
        BlockType::Table => extract_table_text(value),
        BlockType::List => extract_list_text(value),
        _ => content.unwrap_or_default(),
    };

    let block = BlockEntry {
        id: id_str,
        block_type,
        bbox,
        text,
        level: heading_level,
        metadata,
        spans: Vec::new(), // Docling 没有 span 级数据
    };

    page_map.entry(page_number).or_default().push(block);

    // 递归处理 kids（嵌套元素）
    if let Some(kids) = value.get("kids").and_then(|v| v.as_array()) {
        for kid in kids {
            parse_docling_element(kid, page_sizes, page_map);
        }
    }

    // 递归处理 list items
    if let Some(list_items) = value.get("list items").and_then(|v| v.as_array()) {
        for item in list_items {
            parse_docling_element(item, page_sizes, page_map);
        }
    }

    // 递归处理 table rows/cells
    if let Some(rows) = value.get("rows").and_then(|v| v.as_array()) {
        for row in rows {
            if let Some(cells) = row.get("cells").and_then(|v| v.as_array()) {
                for cell in cells {
                    if let Some(cell_kids) = cell.get("kids").and_then(|v| v.as_array()) {
                        for kid in cell_kids {
                            parse_docling_element(kid, page_sizes, page_map);
                        }
                    }
                }
            }
        }
    }
}

/// 从表格元素中提取文本
fn extract_table_text(value: &serde_json::Value) -> String {
    let mut texts = Vec::new();
    
    if let Some(rows) = value.get("rows").and_then(|v| v.as_array()) {
        for row in rows {
            if let Some(cells) = row.get("cells").and_then(|v| v.as_array()) {
                for cell in cells {
                    if let Some(kids) = cell.get("kids").and_then(|v| v.as_array()) {
                        for kid in kids {
                            if let Some(content) = kid.get("content").and_then(|v| v.as_str()) {
                                if !content.is_empty() {
                                    texts.push(content.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    texts.join(" ")
}

/// 从列表元素中提取文本
fn extract_list_text(value: &serde_json::Value) -> String {
    let mut texts = Vec::new();

    if let Some(content) = value.get("content").and_then(|v| v.as_str()) {
        if !content.is_empty() {
            texts.push(content.to_string());
        }
    }

    if let Some(list_items) = value.get("list items").and_then(|v| v.as_array()) {
        for item in list_items {
            if let Some(content) = item.get("content").and_then(|v| v.as_str()) {
                if !content.is_empty() {
                    texts.push(content.to_string());
                }
            }
            // 递归处理嵌套 kids
            if let Some(kids) = item.get("kids").and_then(|v| v.as_array()) {
                for kid in kids {
                    if let Some(content) = kid.get("content").and_then(|v| v.as_str()) {
                        if !content.is_empty() {
                            texts.push(content.to_string());
                        }
                    }
                }
            }
        }
    }

    texts.join(" ")
}