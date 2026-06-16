use lopdf::Document as LodpdfDocument;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
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

/// 内存中的渲染结果缓存（热页面，最近访问的）
struct RenderedPageCache {
    maps: HashMap<String, HashMap<u32, PdfPageImage>>,
    max_pages_per_project: usize,
}

impl RenderedPageCache {
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
        let map = self.maps.entry(project_path).or_insert_with(|| HashMap::new());
        if map.len() >= self.max_pages_per_project {
            map.clear();
        }
        map.insert(page.page_num, page);
    }

    fn clear_project(&mut self, project_path: &str) {
        self.maps.remove(project_path);
    }
}

/// 全局缓存
static RENDERED_CACHE: LazyLock<Mutex<RenderedPageCache>> = LazyLock::new(|| Mutex::new(RenderedPageCache::new(0)));

/// 初始化 PDF 缓存
pub fn init_pdf_cache(max_pages_per_project: usize) {
    let mut cache = RENDERED_CACHE.lock().unwrap();
    *cache = RenderedPageCache::new(max_pages_per_project);
}

/// 使用 lodpdf 快速获取 PDF 页数（毫秒级）
fn get_page_count_lopdf(pdf_path: &str) -> Result<u32, String> {
    let document = LodpdfDocument::load(pdf_path)
        .map_err(|e| format!("Failed to load PDF '{}': {}", pdf_path, e))?;
    Ok(document.get_pages().len() as u32)
}

/// 获取项目的缩略图缓存目录
fn get_thumbnails_dir(project_path: &str) -> PathBuf {
    Path::new(project_path).join(".pdf_thumbnails")
}

/// 获取指定页面的缩略图文件路径
fn get_thumbnail_path(project_path: &str, page_num: u32) -> PathBuf {
    get_thumbnails_dir(project_path)
        .join(format!("{:05}.png", page_num))
}

/// 从磁盘缓存加载已渲染的页面（如果存在）
fn load_cached_page(project_path: &str, page_num: u32) -> Option<PdfPageImage> {
    let path = get_thumbnail_path(project_path, page_num);
    if !path.exists() {
        return None;
    }
    let data = std::fs::read(&path).ok()?;
    // 文件前 12 字节存储元数据: 4 bytes width + 4 bytes height + 4 bytes page_num
    if data.len() < 12 {
        return None;
    }
    let width = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let height = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let stored_page = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    if stored_page != page_num {
        return None;
    }
    let png_data = &data[12..];
    Some(PdfPageImage {
        page_num,
        width: width as u32,
        height: height as u32,
        image_base64: base64_encode(png_data),
    })
}

/// 保存渲染的页面到磁盘缓存
fn save_page_to_cache(project_path: &str, page: &PdfPageImage, png_bytes: &[u8]) {
    let path = get_thumbnail_path(project_path, page.page_num);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // 存储元数据头 + PNG 数据
    let mut data = Vec::with_capacity(12 + png_bytes.len());
    data.extend_from_slice(&page.width.to_le_bytes());
    data.extend_from_slice(&page.height.to_le_bytes());
    data.extend_from_slice(&page.page_num.to_le_bytes());
    data.extend_from_slice(png_bytes);
    let _ = std::fs::write(&path, data);
}

/// 使用 PDFium 从内存数据渲染指定页面
fn render_pages_from_bytes(pdf_bytes: &[u8], page_numbers: &[u32], dpi: f32) -> Result<Vec<(PdfPageImage, Vec<u8>)>, String> {
    use liteparse_pdfium::{Library, Bitmap};
    
    eprintln!("[PDF] render_pages_from_bytes: pages={:?}, dpi={}, size={}MB", 
              page_numbers, dpi, pdf_bytes.len() / 1024 / 1024);
    let start = std::time::Instant::now();
    
    let lib = Library::init();
    let document = lib.load_document_from_bytes(pdf_bytes, None)
        .map_err(|e| format!("Failed to load PDF from bytes: {:?}", e))?;
    
    eprintln!("[PDF] Document loaded from bytes in {:?}, page_count={}", 
              start.elapsed(), document.page_count());
    
    let mut results = Vec::new();
    
    for &page_num in page_numbers {
        let page = document.page((page_num - 1) as i32)
            .map_err(|e| format!("Failed to get page {}: {:?}", page_num, e))?;
        
        let bitmap = page.render(dpi)
            .map_err(|e| format!("Failed to render page {}: {:?}", page_num, e))?;
        
        let width = bitmap.width() as u32;
        let height = bitmap.height() as u32;
        
        let bgra = bitmap.buffer();
        let png_bytes = bgra_to_png(bgra, width, height)
            .map_err(|e| format!("PNG encode failed: {}", e))?;
        
        let elapsed = start.elapsed();
        eprintln!("[PDF] Page {} rendered in {:?} ({}x{}) -> PNG {} bytes", 
                  page_num, elapsed, width, height, png_bytes.len());
        
        results.push((PdfPageImage {
            page_num,
            width,
            height,
            image_base64: base64_encode(&png_bytes),
        }, png_bytes));
    }
    
    eprintln!("[PDF] Total render time for {} pages: {:?}", page_numbers.len(), start.elapsed());
    Ok(results)
}

/// BGRA 转 PNG
fn bgra_to_png(bgra: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    use png::{Encoder, ColorType};
    use std::io::{Cursor, Write};
    
    let mut rgba = Vec::with_capacity(bgra.len());
    for chunk in bgra.chunks_exact(4) {
        rgba.push(chunk[2]);
        rgba.push(chunk[1]);
        rgba.push(chunk[0]);
        rgba.push(chunk[3]);
    }
    
    let mut buffer = Vec::new();
    {
        let cursor = Cursor::new(&mut buffer);
        let mut encoder = Encoder::new(cursor, width, height);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()
            .map_err(|e| format!("PNG header error: {}", e))?;
        writer.write_image_data(&rgba)
            .map_err(|e| format!("PNG write error: {}", e))?;
        writer.finish().map_err(|e| format!("PNG finish error: {}", e))?;
    }
    
    Ok(buffer)
}

/// 请求渲染 PDF 页面
#[command]
pub async fn render_pdf_pages(
    state: tauri::State<'_, ProjectState>,
    page_numbers: Option<Vec<u32>>,
    dpi: Option<u32>,
) -> Result<Vec<PdfPageImage>, String> {
    let pdf_path = get_project_pdf_path(&state)?;
    let project_path_str = pdf_path.clone();
    let pages_to_render = page_numbers.unwrap_or(vec![1]);
    let dpi_val: f32 = dpi.map(|d| d as f32).unwrap_or(150.0);
    
    eprintln!("[PDF] render_pdf_pages: path={}, pages={:?}", pdf_path, pages_to_render);

    let mut result = Vec::new();
    let mut pages_need_render: Vec<u32> = Vec::new();

    // 1. 检查内存缓存
    {
        let cache = RENDERED_CACHE.lock().map_err(|e| e.to_string())?;
        for &pn in &pages_to_render {
            if let Some(cached) = cache.get(&project_path_str, pn) {
                eprintln!("[PDF] Memory cache HIT page {}", pn);
                result.push(cached.clone());
            } else {
                // 2. 检查磁盘缓存
                if let Some(disk_cached) = load_cached_page(&project_path_str, pn) {
                    eprintln!("[PDF] Disk cache HIT page {}", pn);
                    result.push(disk_cached);
                } else {
                    eprintln!("[PDF] Cache MISS page {}", pn);
                    pages_need_render.push(pn);
                }
            }
        }
    }

    // 3. 渲染未命中的页面
    if !pages_need_render.is_empty() {
        eprintln!("[PDF] Rendering {} pages...", pages_need_render.len());
        
        // 读取 PDF 文件（只读一次）
        let pdf_bytes = std::fs::read(&pdf_path)
            .map_err(|e| format!("Failed to read PDF: {}", e))?;
        
        let render_result = timeout(
            Duration::from_secs(120),
            async {
                let pages = pages_need_render.clone();
                let bytes = pdf_bytes;
                let dpi = dpi_val;
                
                tokio::task::spawn_blocking(move || {
                    render_pages_from_bytes(&bytes, &pages, dpi)
                }).await.map_err(|e| format!("Task error: {}", e))?
            }
        ).await.map_err(|_| {
            "渲染超时（120秒）".to_string()
        })?;

        let rendered = render_result?;
        
        // 写入磁盘缓存 + 内存缓存
        {
            let mut cache = RENDERED_CACHE.lock().map_err(|e| e.to_string())?;
            for (img, png_bytes) in rendered {
                save_page_to_cache(&project_path_str, &img, &png_bytes);
                cache.insert(project_path_str.clone(), img.clone());
                result.push(img);
            }
        }
    }

    // 按请求顺序返回
    result.sort_by_key(|p| pages_to_render.iter().position(|&x| x == p.page_num).unwrap_or(usize::MAX));

    eprintln!("[PDF] Returning {} pages", result.len());
    Ok(result)
}

/// 清除指定项目的 PDF 缓存（切换项目时调用）
pub fn clear_project_cache(project_path: &str) {
    eprintln!("[PDF] Clearing caches for: {}", project_path);
    let mut cache = RENDERED_CACHE.lock().unwrap();
    cache.clear_project(project_path);
    // 删除磁盘缩略图目录
    let dir = get_thumbnails_dir(project_path);
    if dir.exists() {
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// 获取 PDF 总页数
#[command]
pub async fn get_pdf_page_count(state: tauri::State<'_, ProjectState>) -> Result<u32, String> {
    let pdf_path = get_project_pdf_path(&state)?;
    eprintln!("[PDF] get_pdf_page_count: {}", pdf_path);

    let start = std::time::Instant::now();
    let count = get_page_count_lopdf(&pdf_path)?;
    eprintln!("[PDF] Page count: {} (took {:?})", count, start.elapsed());

    Ok(count)
}

// =========================================================================
// PDF 文件查找
// =========================================================================

fn get_project_pdf_path(state: &tauri::State<'_, ProjectState>) -> Result<String, String> {
    let project_path = {
        let guard = state.project_path.lock().map_err(|e| e.to_string())?;
        guard.as_ref().ok_or("没有打开的项目")?.display().to_string()
    };
    
    let assets_dir = Path::new(&project_path).join("assets");
    if !assets_dir.is_dir() {
        return Err("项目中未找到 assets 目录".to_string());
    }
    
    let source = crate::ocr_adapter::detect_ocr_source(&assets_dir);
    
    match source {
        Some(crate::ocr_adapter::OcrSource::MinerU) => {
            find_pdf_for_mineru(&assets_dir)
                .ok_or_else(|| "项目中未找到 PDF 文件（MinerU）".to_string())
        }
        Some(crate::ocr_adapter::OcrSource::OpenDataLoader) => {
            find_pdf_for_opendataloader(&assets_dir)
                .ok_or_else(|| "项目中未找到原始 PDF 文件（OpenDataLoader）".to_string())
        }
        None => {
            find_pdf_for_mineru(&assets_dir)
                .or_else(|| find_pdf_for_opendataloader(&assets_dir))
                .or_else(|| find_any_pdf(&assets_dir))
                .ok_or_else(|| "项目中未找到 PDF 文件".to_string())
        }
    }
}

fn find_pdf_for_mineru(assets_dir: &Path) -> Option<String> {
    search_for_pdf(assets_dir, |name| name.to_lowercase().contains("_origin.pdf"))
}

fn find_pdf_for_opendataloader(assets_dir: &Path) -> Option<String> {
    search_for_pdf(assets_dir, |name| {
        let lower = name.to_lowercase();
        lower.ends_with(".pdf") 
            && !lower.contains("_annotated.pdf")
            && !lower.contains("_tagged.pdf")
            && !lower.contains("_origin.pdf")
    })
}

fn find_any_pdf(assets_dir: &Path) -> Option<String> {
    search_for_pdf(assets_dir, |name| name.to_lowercase().ends_with(".pdf"))
}

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