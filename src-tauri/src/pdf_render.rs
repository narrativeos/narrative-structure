use liteparse::{LiteParse, LiteParseConfig};
use serde::Serialize;
use std::path::Path;
use tauri::command;
use tokio::time::{timeout, Duration};

use crate::project_manager::ProjectState;

/// PDF 页面渲染结果
#[derive(Serialize, Debug)]
pub struct PdfPageImage {
    pub page_num: u32,
    pub width: u32,
    pub height: u32,
    /// Base64 编码的 PNG 图片
    pub image_base64: String,
}

/// 请求渲染 PDF 页面（从当前打开的项目自动获取 PDF）
#[command]
pub async fn render_pdf_pages(
    state: tauri::State<'_, ProjectState>,
    page_numbers: Option<Vec<u32>>,
    dpi: Option<u32>,
) -> Result<Vec<PdfPageImage>, String> {
    let pdf_path = get_project_pdf_path(&state)?;
    let dpi = dpi.unwrap_or(150);
    
    // 创建 LiteParse 配置
    let mut config = LiteParseConfig::default();
    config.dpi = dpi as f32;
    config.quiet = true;
    
    let parser = LiteParse::new(config);
    
    // 使用 liteparse 渲染指定页面，设置 120 秒超时（大 PDF 可能需要更长时间）
    let screenshots_result = timeout(
        Duration::from_secs(120),
        parser.screenshot(&pdf_path, page_numbers)
    ).await.map_err(|_| "渲染超时（120秒），该 PDF 页面尺寸过大，建议拆分文档或使用其他 PDF 查看器")?;
    
    let screenshots = screenshots_result.map_err(|e| format!("LiteParse screenshot error: {}", e))?;
    
    // 转换为 base64
    let mut result = Vec::new();
    for s in screenshots {
        let image_b64 = base64_encode(&s.image_bytes);
        result.push(PdfPageImage {
            page_num: s.page_num,
            width: s.width,
            height: s.height,
            image_base64: image_b64,
        });
    }
    
    Ok(result)
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