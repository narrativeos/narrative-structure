//! OCR 数据源适配层：将不同 OCR 引擎的输出归一化为统一的 PageMapping 结构
//!
//! 支持的数据源:
//! - MinerU: _middle.json (span 级 bbox) + content_list.json/content_list_v2.json (分页 + 类型)
//! - OpenDataLoader (Docling): kids 数组 (block 级 bbox, 左下原点坐标)
//!
//! 归一化目标: 将 OCR 后的结构化数据映射到原始 PDF 的物理页，
//! 在此基础上进行更高精度的数据映射对齐，以服务于后续的数据溯源需求。

use std::collections::HashMap;
use std::path::Path;

pub mod mineru;
pub mod opendataloader;

// =========================================================================
// 归一化数据结构
// =========================================================================

/// OCR 数据源类型
#[derive(Debug, Clone, PartialEq)]
pub enum OcrSource {
    MinerU,
    OpenDataLoader,
}

impl std::fmt::Display for OcrSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OcrSource::MinerU => write!(f, "mineru"),
            OcrSource::OpenDataLoader => write!(f, "opendataloader"),
        }
    }
}

/// 内容块类型
#[derive(Debug, Clone, PartialEq)]
pub enum BlockType {
    Paragraph,
    Heading,
    Table,
    Image,
    List,
    Caption,
    Equation,
    Header,
    Footer,
    Other(String),
}

impl std::fmt::Display for BlockType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 使用前端 TYPE_COLORS 识别的名称，确保全链路一致
        match self {
            BlockType::Paragraph => write!(f, "text"),
            BlockType::Heading => write!(f, "heading"),
            BlockType::Table => write!(f, "table"),
            BlockType::Image => write!(f, "image"),
            BlockType::List => write!(f, "list"),
            BlockType::Caption => write!(f, "caption"),
            BlockType::Equation => write!(f, "interline_equation"),
            BlockType::Header => write!(f, "header"),
            BlockType::Footer => write!(f, "footer"),
            BlockType::Other(s) => write!(f, "{}", s),
        }
    }
}

/// Span 级子元素（仅 MinerU 提供，用于高精度标注）
#[derive(Debug, Clone)]
pub struct SpanEntry {
    /// bbox 坐标 [x0, y0, x1, y1]，左上原点
    pub bbox: [f64; 4],
    /// span 文本内容
    pub content: String,
}

/// 内容块（归一化后的最小映射单元）
#[derive(Debug, Clone)]
pub struct BlockEntry {
    /// 唯一标识
    pub id: String,
    /// 块类型
    pub block_type: BlockType,
    /// bbox 坐标 [x0, y0, x1, y1]，统一为左上原点
    pub bbox: [f64; 4],
    /// 可匹配的文本内容
    pub text: String,
    /// 标题层级（仅 Heading 类型使用）
    pub level: Option<u8>,
    /// 可选的富元数据（html, captions, font, image_source 等）
    pub metadata: HashMap<String, serde_json::Value>,
    /// span 级子元素（仅 MinerU 提供，为空时使用 block 级 bbox）
    pub spans: Vec<SpanEntry>,
}

/// 单页条目
#[derive(Debug, Clone)]
pub struct PageEntry {
    /// 页码（0-indexed，内部使用）
    pub page_idx: usize,
    /// 页码（1-indexed，对外输出和前端展示）
    pub page_num: u32,
    /// 页面尺寸 [width, height]
    pub page_size: [f64; 2],
    /// 该页的所有内容块
    pub blocks: Vec<BlockEntry>,
}

/// 统一的 OCR 物理映射结构（数据源无关）
#[derive(Debug, Clone)]
pub struct PageMapping {
    /// 数据源类型
    pub source: OcrSource,
    /// 总页数
    pub page_count: usize,
    /// 按页组织的内容
    pub pages: Vec<PageEntry>,
}

impl PageMapping {
    /// 获取指定页的所有文本（用于匹配）
    pub fn get_page_text(&self, page_idx: usize) -> Option<String> {
        self.pages.iter()
            .find(|p| p.page_idx == page_idx)
            .map(|p| {
                p.blocks.iter()
                    .filter(|b| matches!(b.block_type, BlockType::Paragraph | BlockType::Heading | BlockType::List | BlockType::Caption | BlockType::Equation))
                    .map(|b| b.text.clone())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
    }

    /// 获取所有有文本内容的块（带页码信息）
    pub fn get_text_blocks(&self) -> Vec<(usize, &BlockEntry)> {
        let mut result = Vec::new();
        for page in &self.pages {
            for block in &page.blocks {
                if !block.text.is_empty() {
                    result.push((page.page_idx, block));
                }
            }
        }
        result
    }
}

// =========================================================================
// OcrAdapter trait
// =========================================================================

/// OCR 数据源适配器 trait
pub trait OcrAdapter {
    /// 适配器名称
    fn name(&self) -> &str;

    /// 加载归一化的 PageMapping
    fn load_page_mapping(&self, assets_dir: &Path) -> Result<PageMapping, String>;
}

// =========================================================================
// 数据源检测
// =========================================================================

/// 检测 assets 目录中的数据源类型
pub fn detect_ocr_source(assets_dir: &Path) -> Option<OcrSource> {
    // 检查 MinerU 特征文件
    if crate::project_manager::find_file_in_dir(assets_dir, |n| n.ends_with("_middle.json"))
        .is_some()
        || crate::project_manager::find_file_in_dir(assets_dir, |n| n.contains("content_list") && n.ends_with(".json"))
            .is_some()
    {
        return Some(OcrSource::MinerU);
    }

    // 检查 OpenDataLoader (Docling) 特征文件
    if crate::project_manager::find_file_in_dir(assets_dir, |n| n == "docling.json" || n.ends_with("_docling.json"))
        .is_some()
    {
        return Some(OcrSource::OpenDataLoader);
    }

    None
}

/// 根据检测到的数据源创建对应的适配器
pub fn create_adapter(source: &OcrSource) -> Box<dyn OcrAdapter> {
    match source {
        OcrSource::MinerU => Box::new(mineru::MinerUAdapter),
        OcrSource::OpenDataLoader => Box::new(opendataloader::OpenDataLoaderAdapter),
    }
}

// =========================================================================
// 辅助函数
// =========================================================================

/// 从 MinerU 的 block_type 字符串映射到归一化的 BlockType
pub fn map_mineru_block_type(type_str: &str) -> BlockType {
    match type_str {
        "title" | "heading" => BlockType::Heading,
        "text" | "paragraph" => BlockType::Paragraph,
        "table" => BlockType::Table,
        "image" | "figure" => BlockType::Image,
        "list" => BlockType::List,
        "caption" => BlockType::Caption,
        "equation" | "formula" => BlockType::Equation,
        "header" => BlockType::Header,
        "footer" => BlockType::Footer,
        other => BlockType::Other(other.to_string()),
    }
}

/// 从 Docling 的 type 字符串映射到归一化的 BlockType
pub fn map_docling_block_type(type_str: &str) -> BlockType {
    match type_str {
        "heading" => BlockType::Heading,
        "paragraph" => BlockType::Paragraph,
        "table" => BlockType::Table,
        "image" => BlockType::Image,
        "list" | "list item" => BlockType::List,
        "caption" => BlockType::Caption,
        "formula" | "equation" => BlockType::Equation,
        "header" => BlockType::Header,
        "footer" => BlockType::Footer,
        "text block" => BlockType::Paragraph,
        other => BlockType::Other(other.to_string()),
    }
}

/// 将 Docling 的坐标系统 [left, bottom, right, top] 转换为左上原点 [x0, y0, x1, y1]
/// 需要传入页面高度来进行 y 轴翻转
pub fn convert_docling_bbox(bbox: &[f64; 4], page_height: f64) -> [f64; 4] {
    // Docling: [left, bottom, right, top] (左下原点)
    // 目标: [x0, y0, x1, y1] (左上原点)
    [
        bbox[0],                          // left → x0
        page_height - bbox[3],            // top → y0 (翻转)
        bbox[2],                          // right → x1
        page_height - bbox[1],            // bottom → y1 (翻转)
    ]
}

/// 解析 bbox JSON 数组 [x0, y0, x1, y1]
pub fn parse_bbox(val: Option<&serde_json::Value>) -> Option<[f64; 4]> {
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