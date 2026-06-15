use liteparse::{LiteParse, LiteParseConfig};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
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
            max_pages_per_project,
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

// 全局缓存实例
static PDF_CACHE: Mutex<PdfCache> = Mutex::new(PdfCache::new(0));

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
        let mut cache = PDF_CACHE.lock().map_err(|e| format!("Cache lock error: {}", e))?;
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
                    image_base64,
                });
            }
        }
    }

    // 按请求顺序返回
    result.sort_by_key(|p| pages_to_render.iter().position(|&x| x == p.page_num).unwrap_or(u32::MAX));

    Ok(result)
}

/// 清除指定项目的 PDF 缓存（切换项目时调用）
pub fn clear_project_cache(project_path: &str) {
    let mut cache = PDF_CACHE.lock().unwrap();
    cache.clear_project(project_path);
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

/// 从当前打开的项目中查找默认 PDF 文件（_origin.pdf）
fn get_project_pdf_path(state: &tauri::State<'_, ProjectState>) -> Result<String, String> {
    let project_path = {
        let guard = state.project_path.lock().map_err(|e| e.to_string())?;
        guard.as_ref().ok_or("没有打开的项目")?.display().to_string()
    };
    
    find_pdf_in_assets(&project_path).ok_or("项目中未找到 PDF 文件（搜索 *_origin.pdf）".to_string())
}

/// 在项目的 assets 目录中递归搜索 _origin.pdf
fn find_pdf_in_assets(project_path: &str) -> Option<String> {
    let assets_dir = Path::new(project_path).join("assets");
    if !assets_dir.is_dir() { return None; }
    search_for_origin_pdf(&assets_dir)
}

/// 递归搜索包含 "_origin.pdf" 的文件
fn search_for_origin_pdf(dir: &Path) -> Option<String> {
    if dir.is_file() {
        if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
            if name.contains("_origin.pdf") {
                return Some(dir.display().to_string());
            }
        }
        return None;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Some(found) = search_for_origin_pdf(&entry.path()) {
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