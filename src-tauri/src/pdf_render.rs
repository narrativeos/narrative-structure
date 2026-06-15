use liteparse::{LiteParse, LiteParseConfig};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use tauri::command;
use tokio::time::{timeout, Duration};

use crate::project_manager::ProjectState;

/// PDF 页面渲染结果
#[derive(Serialize, Debug, Clone)]
pub struct PdfPageImage {
    pub page_num: u32,
    pub width: u32,
    pub height: u32,
    /// Base64 编码的 PNG 图片
    pub image_base64: String,
}

/// 简单的 PDF 渲染缓存（按项目隔离）
struct PdfCache {
    /// project_path -> (page_num -> PdfPageImage)
    maps: HashMap<String, HashMap<u32, PdfPageImage>>,
    /// 每个项目最多缓存页数
    max_pages_per_project: usize,
}

impl PdfCache {
    fn new(max_pages: usize) -> Self {
        Self {
            maps: HashMap::new(),
            max_pages_per_project: max_pages,
        }
    }

    fn get(&self, project_path: &str, page_num: u32) -> Option<&PdfPageImage> {
        self.maps.get(project_path)?.get(&page_num)
    }

    fn insert(&mut self, project_path: String, page: PdfPageImage) {
        let map = self.maps.entry(project_path).or_insert_with(HashMap::new);
        // LRU 近似：如果超过限制，清除最老的（HashMap 不保证顺序，简单清除一半）
        if map.len() >= self.max_pages_per_project {
            map.clear();
        }
        map.insert(page.page_num, page);
    }

    fn clear_project(&mut self, project_path: &str) {
        self.maps.remove(project_path);
    }
}

// 全局缓存实例（使用 LazyLock 避免 const 初始化问题）
static PDF_CACHE: LazyLock<Mutex<PdfCache>> = LazyLock::new(|| Mutex::new(PdfCache::new(0)));

// 请求取消：追踪最新请求 ID（按项目隔离）
static ACTIVE_REQUESTS: LazyLock<Mutex<HashMap<String, u64>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

#[allow(dead_code)]
/// 获取当前活跃请求 ID（用于取消检测）
/// 返回 (project_path, request_id)
fn get_active_request_id(project_path: &str) -> u64 {
    let requests = ACTIVE_REQUESTS.lock().unwrap();
    *requests.get(project_path).unwrap_or(&0)
}

#[allow(dead_code)]
/// 设置新请求 ID（递增）
fn set_new_request_id(project_path: &str) -> u64 {
    let mut requests = ACTIVE_REQUESTS.lock().unwrap();
    let next = *requests.get(project_path).unwrap_or(&0) + 1;
    requests.insert(project_path.to_string(), next);
    next
}

/// 清除项目的请求 ID
fn clear_request_id(project_path: &str) {
    let mut requests = ACTIVE_REQUESTS.lock().unwrap();
    requests.remove(project_path);
}

/// 初始化 PDF 缓存（在 app 启动时调用一次）
pub fn init_pdf_cache(max_pages_per_project: usize) {
    let mut cache = PDF_CACHE.lock().unwrap();
    *cache = PdfCache::new(max_pages_per_project);
}

/// 请求渲染 PDF 页面（从当前打开的项目自动获取 PDF）
#[command]
pub async fn render_pdf_pages(
    state: tauri::State<'_, ProjectState>,
    page_numbers: Option<Vec<u32>>,
    dpi: Option<u32>,
) -> Result<Vec<PdfPageImage>, String> {
    let pdf_path = get_project_pdf_path(&state)?;
    let project_path_str = pdf_path.clone();
    let dpi = dpi.unwrap_or(150);
    let pages_to_render = page_numbers.unwrap_or(vec![1]);

    // 先检查缓存
    let mut result = Vec::new();
    let mut pages_to_render_now: Vec<u32> = Vec::new();

    {
        let cache = PDF_CACHE.lock().map_err(|e| format!("Cache lock error: {}", e))?;
        for &pn in &pages_to_render {
            if let Some(cached) = cache.get(&project_path_str, pn) {
                result.push(cached.clone());
            } else {
                pages_to_render_now.push(pn);
            }
        }
    }

    // 渲染缓存未命中的页面
    if !pages_to_render_now.is_empty() {
        // 创建 LiteParse 配置
        let mut config = LiteParseConfig::default();
        config.dpi = dpi as f32;
        config.quiet = true;

        let parser = LiteParse::new(config);

        // 使用 liteparse 渲染指定页面，设置 60 秒超时（降低超时时间）
        let screenshots_result = timeout(
            Duration::from_secs(60),
            parser.screenshot(&pdf_path, Some(pages_to_render_now.clone()))
        ).await.map_err(|_| "渲染超时（60秒），该 PDF 页面尺寸过大，建议拆分文档或使用其他 PDF 查看器")?;

        let screenshots = screenshots_result.map_err(|e| format!("LiteParse screenshot error: {}", e))?;

        // 写入缓存 + 合并结果
        {
            let mut cache = PDF_CACHE.lock().map_err(|e| format!("Cache lock error: {}", e))?;
            for s in screenshots {
                let image_b64 = base64_encode(&s.image_bytes);
                let page = PdfPageImage {
                    page_num: s.page_num,
                    width: s.width,
                    height: s.height,
                    image_base64: image_b64.clone(),
                };
                cache.insert(project_path_str.clone(), page);
                result.push(PdfPageImage {
                    page_num: s.page_num,
                    width: s.width,
                    height: s.height,
                    image_base64: image_b64,
                });
            }
        }
    }

    // 按请求顺序返回
    result.sort_by_key(|p| pages_to_render.iter().position(|&x| x == p.page_num).unwrap_or(usize::MAX));

    Ok(result)
}

/// 清除指定项目的 PDF 缓存（切换项目时调用）
pub fn clear_project_cache(project_path: &str) {
    let mut cache = PDF_CACHE.lock().unwrap();
    cache.clear_project(project_path);
    clear_request_id(project_path);
}

/// 获取 PDF 总页数（从当前打开的项目自动获取 PDF，使用 PDFium 直接获取）
#[command]
pub async fn get_pdf_page_count(state: tauri::State<'_, ProjectState>) -> Result<u32, String> {
    let pdf_path = get_project_pdf_path(&state)?;
    
    use liteparse_pdfium::Library;
    
    // 使用 PDFium 库直接打开 PDF 获取页数
    let lib = Library::init();
    let document = lib.load_document(&pdf_path, None)
        .map_err(|e| format!("Failed to open PDF: {}", e))?;
    
    Ok(document.page_count() as u32)
}

// =========================================================================
// PDF 文件查找：根据 OCR 数据源类型自动选择正确的 PDF
// =========================================================================

/// 获取当前打开的项目中的 PDF 文件路径
/// 
/// 根据 OCR 数据源类型自动选择正确的 PDF：
/// - MinerU: 搜索 *_origin.pdf（未添加标注的原始 PDF，避免与标注层重叠）
/// - OpenDataLoader: 搜索原始输入 PDF（非 _annotated.pdf、非 _tagged.pdf 的 .pdf 文件）
/// - 未知/无 OCR: 先尝试 *_origin.pdf（向后兼容），再尝试任意 PDF
fn get_project_pdf_path(state: &tauri::State<'_, ProjectState>) -> Result<String, String> {
    let project_path = {
        let guard = state.project_path.lock().map_err(|e| e.to_string())?;
        guard.as_ref().ok_or("没有打开的项目")?.display().to_string()
    };
    
    let assets_dir = Path::new(&project_path).join("assets");
    if !assets_dir.is_dir() {
        return Err("项目中未找到 assets 目录".to_string());
    }
    
    // 检测 OCR 数据源类型，根据数据源选择正确的 PDF 查找策略
    let source = crate::ocr_adapter::detect_ocr_source(&assets_dir);
    
    match source {
        Some(crate::ocr_adapter::OcrSource::MinerU) => {
            // MinerU: 加载 *_origin.pdf（未添加标注的原始 PDF）
            // MinerU 输出 _origin.pdf（原始未标注）和 _annotated.pdf（带标注），
            // 我们选择原始 PDF 避免与自绘标注层重叠。
            find_pdf_for_mineru(&assets_dir)
                .ok_or_else(|| "项目中未找到 PDF 文件（MinerU: 搜索 *_origin.pdf）".to_string())
        }
        Some(crate::ocr_adapter::OcrSource::OpenDataLoader) => {
            // OpenDataLoader: 加载原始输入 PDF（排除 _annotated.pdf 和 _tagged.pdf）
            // OpenDataLoader 不输出 _origin.pdf，原始 PDF 以原始文件名保存在 assets 中。
            find_pdf_for_opendataloader(&assets_dir)
                .ok_or_else(|| "项目中未找到原始 PDF 文件（OpenDataLoader: 搜索输入 PDF）".to_string())
        }
        None => {
            // 未知数据源：先尝试 _origin.pdf（向后兼容），再尝试任意 PDF
            find_pdf_for_mineru(&assets_dir)
                .or_else(|| find_pdf_for_opendataloader(&assets_dir))
                .or_else(|| find_any_pdf(&assets_dir))
                .ok_or_else(|| "项目中未找到 PDF 文件（搜索 *_origin.pdf 或任意 .pdf）".to_string())
        }
    }
}

/// 查找 MinerU 的原始 PDF（*_origin.pdf）
fn find_pdf_for_mineru(assets_dir: &Path) -> Option<String> {
    search_for_pdf(assets_dir, |name| {
        name.to_lowercase().contains("_origin.pdf")
    })
}

/// 查找 OpenDataLoader 的原始输入 PDF
/// 排除 _annotated.pdf 和 _tagged.pdf（这些是处理后的产物）。
fn find_pdf_for_opendataloader(assets_dir: &Path) -> Option<String> {
    search_for_pdf(assets_dir, |name| {
        let lower = name.to_lowercase();
        lower.ends_with(".pdf") 
            && !lower.contains("_annotated.pdf")
            && !lower.contains("_tagged.pdf")
            && !lower.contains("_origin.pdf")
    })
}

/// 查找任意 PDF 文件（最后手段的 fallback）
fn find_any_pdf(assets_dir: &Path) -> Option<String> {
    search_for_pdf(assets_dir, |name| {
        name.to_lowercase().ends_with(".pdf")
    })
}

/// 在 assets 目录中递归搜索匹配谓词的 PDF 文件
fn search_for_pdf(assets_dir: &Path, predicate: fn(&str) -> bool) -> Option<String> {
    if !assets_dir.is_dir() {
        return None;
    }
    
    for entry in std::fs::read_dir(assets_dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if predicate(name) {
                    return Some(path.display().to_string());
                }
            }
        } else if path.is_dir() {
            if let Some(found) = search_for_pdf(&path, predicate) {
                return Some(found);
            }
        }
    }
    None
}

/// 简单 base64 编码
fn base64_encode(bytes: &[u8]) -> String {
    let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    let padding = 3 - (bytes.len() % 3);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).map_or(0, |b| *b as u32);
        let b2 = chunk.get(2).map_or(0, |b| *b as u32);
        let n = (b0 << 16) | (b1 << 8) | b2;
        result.push(chars[((n >> 18) & 0x3F) as usize] as char);
        result.push(chars[((n >> 12) & 0x3F) as usize] as char);
        result.push(chars[((n >> 6) & 0x3F) as usize] as char);
        result.push(chars[(n & 0x3F) as usize] as char);
    }
    for _ in 0..padding {
        result.pop();
        result.push('=');
    }
    result
}