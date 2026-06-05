use rusqlite::Connection;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::markdown_parser::parse_markdown;

/// 数据库建表 SQL（与 scripts/init_db.py 保持一致）
const DB_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS blocks (
    id          TEXT PRIMARY KEY,
    parent_id   TEXT,
    order_idx   INTEGER NOT NULL DEFAULT 0,
    level       INTEGER NOT NULL DEFAULT 0,
    block_type  TEXT NOT NULL DEFAULT 'text',
    content     TEXT DEFAULT '',
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

impl ProjectState {
    pub fn new() -> Self {
        Self {
            db_conn: Mutex::new(None),
            project_path: Mutex::new(None),
        }
    }
}

/// 新建项目: 创建目录结构 → 初始化 narrative.db → 自动打开
#[tauri::command]
pub fn create_project(
    state: tauri::State<'_, ProjectState>,
    parent_dir: String,
    project_name: String,
) -> Result<String, String> {
    let project_path = PathBuf::from(&parent_dir).join(&project_name);

    // 1. 验证不重名
    if project_path.exists() {
        return Err(format!("项目已存在: {}", project_path.display()));
    }

    // 2. 创建目录结构
    fs::create_dir_all(project_path.join("assets"))
        .map_err(|e| format!("无法创建 assets 目录: {}", e))?;
    fs::create_dir_all(project_path.join("nodes"))
        .map_err(|e| format!("无法创建 nodes 目录: {}", e))?;
    fs::create_dir_all(project_path.join("prompts"))
        .map_err(|e| format!("无法创建 prompts 目录: {}", e))?;

    // 3. 初始化 narrative.db
    let db_path = project_path.join("narrative.db");
    {
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
    } // conn 在此 drop，后续 open_project 会重新打开

    // 4. 自动打开新项目
    open_project_inner(state, project_path.clone())?;

    Ok(format!("项目已创建并打开: {}", project_path.display()))
}

/// 打开一个项目: 验证路径 → 连接数据库 → 应用 PRAGMA
#[tauri::command]
pub fn open_project(
    state: tauri::State<'_, ProjectState>,
    path: String,
) -> Result<String, String> {
    let project_path = PathBuf::from(&path);
    open_project_inner(state, project_path)
}

/// 内部打开项目逻辑（供 create_project 复用）
fn open_project_inner(
    state: tauri::State<'_, ProjectState>,
    project_path: PathBuf,
) -> Result<String, String> {
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

    // 3. 关闭旧连接 (drop 自动处理)
    {
        let mut conn_guard = state.db_conn.lock().map_err(|e| e.to_string())?;
        *conn_guard = None;
    }

    // 4. 打开新连接
    let conn = Connection::open(&db_path).map_err(|e| format!("无法打开数据库: {}", e))?;

    // 5. 应用 PRAGMA
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;",
    )
    .map_err(|e| format!("PRAGMA 设置失败: {}", e))?;

    // 6. 更新全局状态
    {
        let mut conn_guard = state.db_conn.lock().map_err(|e| e.to_string())?;
        *conn_guard = Some(conn);
    }
    {
        let mut path_guard = state.project_path.lock().map_err(|e| e.to_string())?;
        *path_guard = Some(project_path.clone());
    }

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
pub fn import_document(
    state: tauri::State<'_, ProjectState>,
    zip_path: String,
) -> Result<String, String> {
    let conn_guard = state.db_conn.lock().map_err(|e| e.to_string())?;
    let conn = conn_guard.as_ref().ok_or("没有打开的项目")?;

    let project_path = {
        let path_guard = state.project_path.lock().map_err(|e| e.to_string())?;
        path_guard.clone().ok_or("项目路径未设置")?
    };

    let zip_file = PathBuf::from(&zip_path);
    if !zip_file.exists() {
        return Err(format!("文件不存在: {}", zip_path));
    }

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

    let assets_dir = project_path.join("assets").join(doc_name);
    fs::create_dir_all(&assets_dir)
        .map_err(|e| format!("无法创建资源目录: {}", e))?;

    // 3. 解压所有文件到 assets/<doc_name>/
    let mut md_content: Option<String> = None;

    for i in 0..archive.len() {
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
    }

    // 也解压 images/ 子目录
    // (上面已按文件名处理，flat 结构也可以)

    // 4. 解析 Markdown → SemanticBlock
    let md_text = md_content.ok_or("zip 中未找到 .md 文件")?;
    let parsed_blocks = parse_markdown(&md_text);

    if parsed_blocks.is_empty() {
        return Ok(format!(
            "已导入资源文件到 {}，但 Markdown 中未解析出语义块",
            assets_dir.display()
        ));
    }

    // 5. 批量写入数据库（事务）
    conn.execute("BEGIN", [])
        .map_err(|e| format!("事务开始失败: {}", e))?;

    let insert_sql = "INSERT INTO blocks (id, parent_id, order_idx, level, block_type, content, metadata)
                      VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)";

    let block_count = parsed_blocks.len();
    for block in &parsed_blocks {
        conn.execute(
            insert_sql,
            rusqlite::params![
                block.id,
                block.parent_id,
                block.order_idx,
                block.level,
                block.block_type,
                block.content,
                block.metadata,
            ],
        )
        .map_err(|e| format!("插入块失败: {}", e))?;
    }

    conn.execute("COMMIT", [])
        .map_err(|e| format!("事务提交失败: {}", e))?;

    Ok(format!(
        "导入成功: {} 个语义块 → assets/{}/",
        block_count, doc_name
    ))
}

/// 获取当前项目路径
#[tauri::command]
pub fn get_project_path(state: tauri::State<'_, ProjectState>) -> Result<Option<String>, String> {
    let path_guard = state.project_path.lock().map_err(|e| e.to_string())?;
    Ok(path_guard.as_ref().map(|p| p.display().to_string()))
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
