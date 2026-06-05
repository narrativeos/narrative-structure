use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

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
