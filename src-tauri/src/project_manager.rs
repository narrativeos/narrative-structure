use rusqlite::Connection;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;



use crate::markdown_parser::parse_markdown;
use crate::page_mapper;


/// 数据库建表 SQL（与 scripts/init_db.py 保持一致）
const DB_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS blocks (
    id          TEXT PRIMARY KEY,
    parent_id   TEXT,
    order_idx   INTEGER NOT NULL DEFAULT 0,
    level       INTEGER NOT NULL DEFAULT 0,
    block_type  TEXT NOT NULL DEFAULT 'text',
    content     TEXT DEFAULT '',
    original_content TEXT DEFAULT '',
    metadata    TEXT DEFAULT '{}',
    version     INTEGER NOT NULL DEFAULT 1,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (parent_id) REFERENCES blocks(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_blocks_tree ON blocks(level, order_idx);
CREATE INDEX IF NOT EXISTS idx_blocks_parent ON blocks(parent_id);
CREATE INDEX IF NOT EXISTS idx_blocks_type ON blocks(block_type);

CREATE VIRTUAL TABLE IF NOT EXISTS blocks_fts
    USING fts5(content, content='blocks', content_rowid='rowid');

CREATE TRIGGER IF NOT EXISTS blocks_ai AFTER INSERT ON blocks BEGIN
    INSERT INTO blocks_fts(rowid, content) VALUES (new.rowid, new.content);
END;

CREATE TRIGGER IF NOT EXISTS blocks_ad AFTER DELETE ON blocks BEGIN
    INSERT INTO blocks_fts(blocks_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
END;

CREATE TRIGGER IF NOT EXISTS blocks_au AFTER UPDATE ON blocks BEGIN
    INSERT INTO blocks_fts(blocks_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
    INSERT INTO blocks_fts(rowid, content) VALUES (new.rowid, new.content);
END;
";

/// 全局项目状态 — 管理当前打开的项目
pub struct ProjectState {
    pub db_conn: Mutex<Option<Connection>>,
    pub project_path: Mutex<Option<PathBuf>>,
}

impl Default for ProjectState {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectState {
    pub fn new() -> Self {
        Self {
            db_conn: Mutex::new(None),
            project_path: Mutex::new(None),
        }
    }
}

/// 获取项目根目录：~/.narrativeos/narrative-structure/
fn project_root_dir() -> PathBuf {
    let home = dirs_next().unwrap_or_else(|| PathBuf::from("."));
    home.join(".narrativeos").join("narrative-structure")
}

/// 获取用户主目录
fn dirs_next() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

/// 生成时间序列 ID: YYYYMMDD_HHMMSS_<4位随机hex>
fn timestamp_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    // 简单 hash 取后 4 位 hex
    let suffix = format!("{:08x}", now);
    let suffix = &suffix[suffix.len().saturating_sub(4)..];

    // 格式化时间
    use std::time::SystemTime;
    #[allow(deprecated)]
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let days = secs / 86400;
    let day_secs = secs % 86400;
    let hours = day_secs / 3600;
    let mins = (day_secs % 3600) / 60;
    let secs_rem = day_secs % 60;

    // 计算年月日 (简化)
    let (y, m, d) = days_to_ymd(days as i64);

    format!("{:04}{:02}{:02}_{:02}{:02}{:02}_{}", y, m, d, hours, mins, secs_rem, suffix)
}

/// 简化的 unix epoch days → YMD
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

/// 递归搜索目录中匹配的文件
fn find_file_in_dir(dir: &Path, predicate: fn(&str) -> bool) -> Option<PathBuf> {
    if !dir.is_dir() {
        return None;
    }
    for entry in fs::read_dir(dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if predicate(name) {
                    return Some(path);
                }
            }
        } else if path.is_dir() {
            if let found @ Some(_) = find_file_in_dir(&path, predicate) {
                return found;
            }
        }
    }
    None
}

fn emit_stage_progress(app_handle: &tauri::AppHandle, stage: &str, index: usize, total: usize, min_percent: u8, max_percent: u8, detail: &str) {
    let percent = if total == 0 {
        max_percent
    } else {
        let ratio = ((index + 1) as f64 / total as f64).min(1.0);
        let value = min_percent as f64 + ratio * (max_percent.saturating_sub(min_percent)) as f64;
        value.round().clamp(min_percent as f64, max_percent as f64) as u8
    };
    page_mapper::emit_progress(app_handle, stage, percent, detail);
}

/// 发送进度事件到前端
/// 流程: 解压 → 以时间戳创建项目目录 → 初始化 DB → 解析 MD → 打开项目
#[tauri::command]
pub async fn import_new_project(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, ProjectState>,
    zip_path: String,
) -> Result<String, String> {
    let app_handle = app_handle.clone();
    let state = state.clone();
    let app_handle_for_thread = app_handle.clone();
    let (result_text, project_dir) = tauri::async_runtime::spawn_blocking(move || import_new_project_blocking(app_handle_for_thread, zip_path))
        .await
        .map_err(|e| format!("导入线程失败: {}", e))??;

    page_mapper::emit_progress(&app_handle, "项目准备", 99, "加载项目中...");
    let log_path = project_dir.join("import.log");
    page_mapper::emit_log(&app_handle, "[import] 项目准备: 开始打开项目并连接数据库", Some(&log_path));
    open_project_inner(&state, project_dir.clone())?;
    page_mapper::emit_log(&app_handle, "[import] 项目已打开，导入完成", Some(&log_path));
    page_mapper::emit_progress(&app_handle, "完成", 100, "项目就绪");

    Ok(result_text)
}

fn import_new_project_blocking(
    app_handle: tauri::AppHandle,
    zip_path: String,
) -> Result<(String, PathBuf), String> {
    let zip_file = PathBuf::from(&zip_path);
    if !zip_file.exists() {
        return Err(format!("文件不存在: {}", zip_path));
    }

    // 项目名称 = zip 文件名（不含扩展名）
    let project_name = zip_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled");

    let zip_size = fs::metadata(&zip_file).map(|m| m.len()).unwrap_or(0);
    page_mapper::emit_log(&app_handle, &format!("[import] 开始导入: {} ({:.1} MB)", project_name, zip_size as f64 / 1_048_576.0), None);

    // 项目文件夹 = <project_root>/Projects/<timestamp_id>/
    let project_id = timestamp_id();
    let project_dir = project_root_dir().join("Projects").join(&project_id);
    let log_path = project_dir.join("import.log");
    page_mapper::emit_log(&app_handle, &format!("[import] 项目目录: {}", project_dir.display()), Some(&log_path));

    fs::create_dir_all(project_dir.join("assets"))
        .map_err(|e| format!("无法创建目录: {}", e))?;
    fs::create_dir_all(project_dir.join("nodes"))
        .map_err(|e| format!("无法创建目录: {}", e))?;
    fs::create_dir_all(project_dir.join("prompts"))
        .map_err(|e| format!("无法创建目录: {}", e))?;

    // 解压到 assets/<project_name>/
    let assets_subdir = project_dir.join("assets").join(project_name);
    fs::create_dir_all(&assets_subdir)
        .map_err(|e| format!("无法创建资源目录: {}", e))?;

    let file = fs::File::open(&zip_file)
        .map_err(|e| format!("无法打开 zip: {}", e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("无法读取 zip: {}", e))?;

    let mut md_content: Option<String> = None;
    let total_entries = archive.len();
    page_mapper::emit_log(&app_handle, &format!("[import] ZIP 包含 {} 个条目，开始解压...", total_entries), Some(&log_path));
    page_mapper::emit_progress(&app_handle, "解压 ZIP", 1, &format!("共 {} 个文件", total_entries));

    // 先扫描所有条目，检测是否有公共前缀目录（如 GitHub zip 的 project-main/）
    let mut common_prefix = String::new();
    for i in 0..total_entries {
        if let Ok(entry) = archive.by_index(i) {
            let name = entry.name().to_string();
            if !name.ends_with('/') && !name.starts_with('.') {
                let first = name.split('/').next().unwrap_or("");
                if common_prefix.is_empty() {
                    common_prefix = first.to_string();
                } else if common_prefix != first {
                    common_prefix.clear();
                    break;
                }
            }
        }
    }
    if !common_prefix.is_empty() {
        page_mapper::emit_log(&app_handle, &format!("[import] 检测到公共前缀目录: {}/", common_prefix), Some(&log_path));
    }

    let mut pdf_count = 0u32;
    let mut img_count = 0u32;
    let mut json_count = 0u32;
    let mut other_count = 0u32;
    let extract_progress_interval = (total_entries / 20).max(25);

    for i in 0..total_entries {
        let mut entry = archive.by_index(i)
            .map_err(|e| format!("读取 zip 条目失败: {}", e))?;
        let entry_name = entry.name().to_string();
        if entry_name.ends_with('/') {
            continue;
        }

        // 去掉公共前缀，保留相对路径结构
        let rel_path = if !common_prefix.is_empty() && entry_name.starts_with(&format!("{}/", common_prefix)) {
            entry_name[common_prefix.len() + 1..].to_string()
        } else {
            entry_name.clone()
        };

        let dest_path = assets_subdir.join(&rel_path);
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).ok();
        }

        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)
            .map_err(|e| format!("读取 zip 条目失败: {}", e))?;
        fs::write(&dest_path, &buf)
            .map_err(|e| format!("写入文件失败: {}", e))?;

        // 统计文件类型
        let lower = rel_path.to_lowercase();
        if lower.ends_with(".pdf") { pdf_count += 1; }
        else if lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") || lower.ends_with(".gif") || lower.ends_with(".svg") || lower.ends_with(".webp") { img_count += 1; }
        else if lower.ends_with(".json") { json_count += 1; }
        else { other_count += 1; }

        // 捕获第一个 .md 作为正文
        if lower.ends_with(".md") && md_content.is_none() {
            md_content = Some(String::from_utf8_lossy(&buf).to_string());
        }

        if i % extract_progress_interval == 0 || i + 1 == total_entries {
            let progress = 10 + ((i as f64 / total_entries as f64) * 20.0).round() as u8;
            page_mapper::emit_progress(&app_handle, "解压 ZIP", progress.min(29), &format!("解压文件 {}/{}", i + 1, total_entries));
        }
    }

    page_mapper::emit_log(&app_handle, &format!(
        "[import] 解压完成: PDF×{} 图片×{} JSON×{} 其他×{} → {}",
        pdf_count, img_count, json_count, other_count, assets_subdir.display()
    ), Some(&log_path));

    // 初始化 narrative.db
    page_mapper::emit_log(&app_handle, "[import] 初始化数据库...", Some(&log_path));
    page_mapper::emit_progress(&app_handle, "初始化数据库", 4, "创建表结构...");
    let db_path = project_dir.join("narrative.db");
    let conn = Connection::open(&db_path)
        .map_err(|e| format!("无法创建数据库: {}", e))?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;
             PRAGMA busy_timeout=5000;",
    )
    .map_err(|e| format!("PRAGMA 设置失败: {}", e))?;
    conn.execute_batch(DB_SCHEMA)
        .map_err(|e| format!("建表失败: {}", e))?;

    // Markdown 行级分块 → 基于 _middle.json bbox 文本匹配页码 → 写入 DB
    if let Some(ref md) = md_content {
            page_mapper::emit_log(&app_handle, &format!("[import] MD 大小: {} bytes", md.len()), Some(&log_path));
            page_mapper::emit_progress(&app_handle, "解析 Markdown", 5, "行级分块...");
            let mut parsed = parse_markdown(md);
            let line_count = parsed.len();
            page_mapper::emit_log(&app_handle, &format!("[import] 解析完成: {} 行 (headings: {})", line_count,
                parsed.iter().filter(|b| b.block_type == "heading").count()), Some(&log_path));

            if !parsed.is_empty() {
                // 页码映射: bbox span 文本 → MD 行匹配
                let assets_dir = project_dir.join("assets");
                if let Some(middle_path) = find_file_in_dir(&assets_dir, |n| n.ends_with("_middle.json")) {
                    page_mapper::emit_progress(&app_handle, "加载信息层", 6, "展开 _middle.json bbox...");
                    page_mapper::apply_bbox_page_mapping(&app_handle, &middle_path, &mut parsed);
                    page_mapper::emit_log(&app_handle, "[import] 信息层加载完成", Some(&log_path));
                }

                page_mapper::emit_log(&app_handle, "[import] 写入数据库阶段开始", Some(&log_path));
                emit_stage_progress(&app_handle, "写入数据库", 0, line_count, 90, 94, &format!("共 {} 行", line_count));
                conn.execute("BEGIN", []).map_err(|e| format!("事务失败: {}", e))?;
                for (idx, block) in parsed.iter().enumerate() {
                    if idx % 100 == 0 || idx + 1 == line_count {
                        emit_stage_progress(&app_handle, "写入数据库", idx, line_count, 90, 94, "");
                    }
                    conn.execute(
                        "INSERT INTO blocks (id, parent_id, order_idx, level, block_type, content, original_content, metadata)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        rusqlite::params![
                            block.id, block.parent_id, block.order_idx,
                            block.level, block.block_type, block.content, block.content, block.metadata,
                        ],
                    )
                    .map_err(|e| format!("插入块失败: {}", e))?;
                }
                conn.execute("COMMIT", []).map_err(|e| format!("提交失败: {}", e))?;
                page_mapper::emit_log(&app_handle, "[import] 写入数据库阶段完成", Some(&log_path));
                page_mapper::emit_log(&app_handle, &format!("[import] DB 写入完成: {} 行", line_count), Some(&log_path));
            }
        }

    let block_count: i32 = conn.query_row("SELECT COUNT(*) FROM blocks", [], |r| r.get(0))
        .unwrap_or(0);

    Ok((format!(
        "{} | {} 个语义块 | {}",
        project_name, block_count, project_dir.display()
    ), project_dir))
}

/// 打开一个项目: 验证路径 → 连接数据库 → 应用 PRAGMA
#[tauri::command]
pub fn open_project(
    state: tauri::State<'_, ProjectState>,
    path: String,
) -> Result<String, String> {
    let project_path = PathBuf::from(&path);
    open_project_inner(&state, project_path)
}

/// 内部打开项目逻辑（供 create_project 复用）
fn open_project_inner(
    state: &tauri::State<'_, ProjectState>,
    project_path: PathBuf,
) -> Result<String, String> {
    let start = std::time::Instant::now();
    
    // 1. 验证目录存在
    if !project_path.is_dir() {
        return Err(format!("目录不存在: {}", project_path.display()));
    }

    let db_path = project_path.join("narrative.db");

    // 2. 检查 narrative.db 是否存在
    if !db_path.exists() {
        return Err(format!(
            "数据库文件不存在: {}\n请先运行 scripts/init_db.py 初始化项目",
            db_path.display()
        ));
    }
    
    let validate_time = start.elapsed();
    eprintln!("[PERF] open_project: 路径验证 = {:.3}ms", validate_time.as_secs_f64() * 1000.0);

    // 3. 关闭旧连接 (drop 自动处理)
    {
        let mut conn_guard = state.db_conn.lock().map_err(|e| e.to_string())?;
        *conn_guard = None;
    }

    // 4. 打开新连接
    let db_open_start = start.elapsed();
    let conn = Connection::open(&db_path).map_err(|e| format!("无法打开数据库: {}", e))?;
    let db_open_time = start.elapsed() - db_open_start;
    eprintln!("[PERF] open_project: 数据库打开 = {:.3}ms", db_open_time.as_secs_f64() * 1000.0);

    // 5. 应用 PRAGMA
    let pragma_start = start.elapsed();
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;",
    )
    .map_err(|e| format!("PRAGMA 设置失败: {}", e))?;
    let pragma_time = start.elapsed() - pragma_start;
    eprintln!("[PERF] open_project: PRAGMA 设置 = {:.3}ms", pragma_time.as_secs_f64() * 1000.0);

    // 迁移：为旧数据库添加 original_content 列
    let migrate_start = start.elapsed();
    let _ = conn.execute_batch("ALTER TABLE blocks ADD COLUMN original_content TEXT DEFAULT ''");
    let migrate_time = start.elapsed() - migrate_start;
    eprintln!("[PERF] open_project: 迁移检查 = {:.3}ms", migrate_time.as_secs_f64() * 1000.0);

    // 6. 更新全局状态
    {
        let mut conn_guard = state.db_conn.lock().map_err(|e| e.to_string())?;
        *conn_guard = Some(conn);
    }
    {
        let mut path_guard = state.project_path.lock().map_err(|e| e.to_string())?;
        *path_guard = Some(project_path.clone());
    }
    
    let total_time = start.elapsed();
    eprintln!("[PERF] open_project: 总耗时 = {:.3}ms", total_time.as_secs_f64() * 1000.0);

    Ok(format!("项目已打开: {}", project_path.display()))
}

/// 关闭当前项目，释放数据库连接
#[tauri::command]
pub fn close_project(state: tauri::State<'_, ProjectState>) -> Result<String, String> {
    let mut conn_guard = state.db_conn.lock().map_err(|e| e.to_string())?;
    let mut path_guard = state.project_path.lock().map_err(|e| e.to_string())?;

    let prev_path = path_guard.take();
    *conn_guard = None;

    match prev_path {
        Some(p) => Ok(format!("项目已关闭: {}", p.display())),
        None => Ok("没有打开的项目".to_string()),
    }
}

/// 导入 MinerU 输出 zip 包: 解压 → 复制到 assets/ → 解析 .md → 写入 blocks
#[tauri::command]
pub async fn import_document(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, ProjectState>,
    zip_path: String,
) -> Result<String, String> {
    let app_handle = app_handle.clone();
    let project_path = {
        let path_guard = state.project_path.lock().map_err(|e| e.to_string())?;
        path_guard.clone().ok_or_else(|| "项目路径未设置".to_string())?
    };

    tauri::async_runtime::spawn_blocking(move || import_document_blocking(app_handle, project_path, zip_path))
        .await
        .map_err(|e| format!("导入线程失败: {}", e))?
}

fn import_document_blocking(
    app_handle: tauri::AppHandle,
    project_path: PathBuf,
    zip_path: String,
) -> Result<String, String> {
    let conn = Connection::open(project_path.join("narrative.db"))
        .map_err(|e| format!("无法打开数据库: {}", e))?;

    let zip_file = PathBuf::from(&zip_path);
    if !zip_file.exists() {
        return Err(format!("文件不存在: {}", zip_path));
    }

    let zip_size = fs::metadata(&zip_file).map(|m| m.len()).unwrap_or(0);
    let log_path = project_path.join("import.log");

    page_mapper::emit_progress(&app_handle, "解压 ZIP", 1, "读取压缩包...");

    // 1. 读取 zip
    let file = fs::File::open(&zip_file)
        .map_err(|e| format!("无法打开 zip: {}", e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("无法读取 zip: {}", e))?;

    // 2. 确定文档 ID（取 zip 文件名去除扩展名）
    let doc_name = zip_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("imported_doc");

    page_mapper::emit_log(&app_handle, &format!("[import-doc] 开始追加导入: {} ({:.1} MB)", doc_name, zip_size as f64 / 1_048_576.0), Some(&log_path));

    let assets_dir = project_path.join("assets").join(doc_name);
    page_mapper::emit_log(&app_handle, &format!("[import-doc] 资源目录: {}", assets_dir.display()), Some(&log_path));
    fs::create_dir_all(&assets_dir)
        .map_err(|e| format!("无法创建资源目录: {}", e))?;

    // 3. 解压所有文件到 assets/<doc_name>/
    let mut md_content: Option<String> = None;
    let total_entries = archive.len();
    let extract_progress_interval = (total_entries / 20).max(25);
    page_mapper::emit_log(&app_handle, &format!("[import-doc] ZIP 包含 {} 个条目，解压中...", total_entries), Some(&log_path));
    page_mapper::emit_progress(&app_handle, "解压 ZIP", 1, &format!("共 {} 个文件", total_entries));

    for i in 0..total_entries {
        let mut entry = archive.by_index(i)
            .map_err(|e| format!("读取 zip 条目失败: {}", e))?;
        let entry_name = entry.name().to_string();

        // 跳过目录条目
        if entry_name.ends_with('/') {
            continue;
        }

        // 提取文件名（去掉路径前缀）
        let file_name = Path::new(&entry_name)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&entry_name);

        let dest_path = assets_dir.join(file_name);

        // 读取内容
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)
            .map_err(|e| format!("读取 zip 条目失败: {}", e))?;

        // 写入目标
        fs::write(&dest_path, &buf)
            .map_err(|e| format!("写入文件失败: {}", e))?;

        // 抓取 .md 文件内容
        if file_name.ends_with(".md") && md_content.is_none() {
            md_content = Some(String::from_utf8_lossy(&buf).to_string());
        }

        if i % extract_progress_interval == 0 || i + 1 == total_entries {
            let progress = 1 + ((i as f64 / total_entries as f64) * 2.0).round() as u8;
            page_mapper::emit_progress(&app_handle, "解压 ZIP", progress.min(3), &format!("解压文件 {}/{}", i + 1, total_entries));
        }
    }

    page_mapper::emit_progress(&app_handle, "解压 ZIP", 3, "解压完成，继续解析 Markdown...");
    page_mapper::emit_log(&app_handle, "[import-doc] 解压完成，开始解析 Markdown", Some(&log_path));
    page_mapper::emit_progress(&app_handle, "解析 Markdown", 5, "行级分块...");

    // 4. Markdown 行级分块 → 页码映射
    let md_text = md_content.ok_or("zip 中未找到 .md 文件")?;
    let mut parsed_blocks = parse_markdown(&md_text);

    if parsed_blocks.is_empty() {
        return Ok(format!(
            "已导入资源文件到 {}，但 Markdown 中未解析出语义块",
            assets_dir.display()
        ));
    }

    // 基于 _middle.json bbox span 文本 → MD 行匹配页码
    if let Some(middle_path) = find_file_in_dir(&assets_dir, |n| n.ends_with("_middle.json")) {
        page_mapper::emit_progress(&app_handle, "加载信息层", 6, "展开 _middle.json bbox...");
        page_mapper::emit_log(&app_handle, "[import-doc] 开始加载信息层", Some(&log_path));
        page_mapper::apply_bbox_page_mapping(&app_handle, &middle_path, &mut parsed_blocks);
        page_mapper::emit_log(&app_handle, "[import-doc] 信息层加载完成", Some(&log_path));
    }

    let block_count = parsed_blocks.len();
    emit_stage_progress(&app_handle, "写入数据库", 0, block_count as usize, 90, 94, &format!("共 {} 行", block_count));

    // 5. 批量写入数据库（事务）
    conn.execute("BEGIN", [])
        .map_err(|e| format!("事务开始失败: {}", e))?;

    let insert_sql = "INSERT INTO blocks (id, parent_id, order_idx, level, block_type, content, original_content, metadata)
                      VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)";

    for (idx, block) in parsed_blocks.iter().enumerate() {
        if idx % 100 == 0 || idx + 1 == block_count {
            emit_stage_progress(&app_handle, "写入数据库", idx, block_count as usize, 90, 94, "");
        }
        conn.execute(
            insert_sql,
            rusqlite::params![
                block.id,
                block.parent_id,
                block.order_idx,
                block.level,
                block.block_type,
                block.content,
                block.content,
                block.metadata,
            ],
        )
        .map_err(|e| format!("插入块失败: {}", e))?;
    }

    conn.execute("COMMIT", [])
        .map_err(|e| format!("事务提交失败: {}", e))?;

    page_mapper::emit_log(&app_handle, "[import-doc] 写入数据库阶段完成", Some(&log_path));
    page_mapper::emit_log(&app_handle, &format!("[import-doc] 导入完成: {} 个语义块", block_count), Some(&log_path));
    page_mapper::emit_progress(&app_handle, "项目准备", 99, "写入完成，正在收尾...");
    page_mapper::emit_progress(&app_handle, "完成", 100, &format!("{} 个语义块就绪", block_count));

    Ok(format!(

        "导入成功: {} 个语义块 → assets/{}/",
        block_count, doc_name
    ))
}

/// 查找 assets 目录下匹配模式的文件
#[tauri::command]
pub fn find_asset_file(
    state: tauri::State<'_, ProjectState>,
    pattern: String,
) -> Result<Option<String>, String> {
    let path_guard = state.project_path.lock().map_err(|e| e.to_string())?;
    let project_path = path_guard.as_ref().ok_or("没有打开的项目")?;
    let assets_dir = project_path.join("assets");
    if !assets_dir.exists() { return Ok(None); }

    let mut result = None;
    collect_matching_files(&assets_dir, &assets_dir, &pattern, &mut result);
    Ok(result)
}

/// 读取文件并返回字节数组
#[tauri::command]
pub fn read_file_bytes(path: String) -> Result<Vec<u8>, String> {
    let mut file = fs::File::open(&path).map_err(|e| format!("无法打开: {}", e))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).map_err(|e| format!("读取失败: {}", e))?;
    Ok(buf)
}

fn collect_matching_files(_base: &Path, dir: &Path, pattern: &str, out: &mut Option<String>) {
    if out.is_some() { return; }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_matching_files(_base, &path, pattern, out);
            } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.contains(pattern) {
                    *out = Some(path.display().to_string());
                    return;
                }
            }
        }
    }
}

/// 获取当前项目路径
#[tauri::command]
pub fn get_project_path(state: tauri::State<'_, ProjectState>) -> Result<Option<String>, String> {
    let path_guard = state.project_path.lock().map_err(|e| e.to_string())?;
    Ok(path_guard.as_ref().map(|p| p.display().to_string()))
}

/// 列出项目 assets 目录下的所有文件
#[tauri::command]
pub fn list_project_files(
    state: tauri::State<'_, ProjectState>,
) -> Result<Vec<String>, String> {
    let path_guard = state.project_path.lock().map_err(|e| e.to_string())?;
    let project_path = path_guard.as_ref().ok_or("没有打开的项目")?;

    let assets_dir = project_path.join("assets");
    if !assets_dir.exists() {
        return Ok(vec![]);
    }

    let mut files = Vec::new();
    collect_files(&assets_dir, &assets_dir, &mut files);
    files.sort();
    Ok(files)
}

fn collect_files(_base: &Path, dir: &Path, out: &mut Vec<String>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_files(_base, &path, out);
            } else {
                // 返回绝对路径，确保自定义协议能直接访问
                out.push(path.display().to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_state_new() {
        let state = ProjectState::new();
        assert!(state.db_conn.lock().unwrap().is_none());
        assert!(state.project_path.lock().unwrap().is_none());
    }
}
