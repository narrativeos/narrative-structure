//! MCP Tools — 工具描述与执行
//!
//! 每个工具都有结构化的描述（名称、说明、参数 schema），
//! 外部智能体通过阅读这些描述来自动调用对应功能。

use serde_json::{json, Value};

use crate::db_engine::{Block, TocNode};
use crate::mcp::server::McpState;

// ---------------------------------------------------------------------------
// 工具描述
// ---------------------------------------------------------------------------

/// 列出所有可用的 MCP 工具（符合 MCP spec 格式）
pub fn list_tools() -> Vec<Value> {
    vec![
        // --- 项目管理 ---
        json!({
            "name": "open_project",
            "description": "打开一个已有的 NarrativeStructure 项目（包含 narrative.db 的目录）",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "项目目录的绝对路径"
                    }
                },
                "required": ["path"]
            }
        }),
        json!({
            "name": "close_project",
            "description": "关闭当前打开的项目，释放数据库连接",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        json!({
            "name": "get_project_info",
            "description": "获取当前项目的路径和基本信息",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        json!({
            "name": "import_document",
            "description": "导入 MinerU 输出的 zip 压缩包到当前项目（追加模式）",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "zip_path": {
                        "type": "string",
                        "description": "zip 文件的绝对路径"
                    }
                },
                "required": ["zip_path"]
            }
        }),
        // --- 目录与结构 ---
        json!({
            "name": "get_toc",
            "description": "获取文档的目录树（TOC），返回层级化的标题结构",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        json!({
            "name": "get_page_stats",
            "description": "统计每个 PDF 页码包含的语义块数量",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        // --- 语义块操作 ---
        json!({
            "name": "get_blocks",
            "description": "获取语义块列表（支持分页和按父节点过滤）",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "parent_id": {
                        "type": "string",
                        "description": "可选：只获取指定父块下的子块"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "返回数量上限（默认 100）",
                        "default": 100
                    },
                    "offset": {
                        "type": "integer",
                        "description": "偏移量（默认 0）",
                        "default": 0
                    }
                }
            }
        }),
        json!({
            "name": "get_block",
            "description": "根据 ID 获取单个语义块的完整信息",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "语义块的唯一 ID"
                    }
                },
                "required": ["id"]
            }
        }),
        json!({
            "name": "get_blocks_by_page",
            "description": "根据 PDF 页码范围获取该页上的所有语义块",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "page_start": {
                        "type": "integer",
                        "description": "起始页码（包含）"
                    },
                    "page_end": {
                        "type": "integer",
                        "description": "结束页码（包含）"
                    }
                },
                "required": ["page_start", "page_end"]
            }
        }),
        json!({
            "name": "update_block",
            "description": "更新语义块的内容（带乐观锁版本控制）",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "语义块的唯一 ID"
                    },
                    "content": {
                        "type": "string",
                        "description": "新的内容"
                    },
                    "version": {
                        "type": "integer",
                        "description": "期望的当前版本号（用于乐观锁）"
                    }
                },
                "required": ["id", "content", "version"]
            }
        }),
        // --- 搜索 ---
        json!({
            "name": "search_blocks",
            "description": "使用 FTS5 全文搜索引擎搜索语义块",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "搜索关键词（支持 FTS5 语法）"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "返回结果数量上限（默认 20）",
                        "default": 20
                    }
                },
                "required": ["query"]
            }
        }),
        // --- 资源文件 ---
        json!({
            "name": "list_assets",
            "description": "列出项目 assets 目录下的所有资源文件",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        json!({
            "name": "find_asset",
            "description": "在 assets 目录中搜索匹配模式的文件",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "文件名匹配模式（子串匹配）"
                    }
                },
                "required": ["pattern"]
            }
        }),
    ]
}

/// 返回 tools/list 的完整响应（用于 CLI 模式）
pub fn list_tools_response() -> Value {
    json!({ "tools": list_tools() })
}

// ---------------------------------------------------------------------------
// 工具执行
// ---------------------------------------------------------------------------

/// 调用指定工具，返回结果或错误
pub fn call_tool(name: &str, arguments: &Value, state: &McpState) -> Result<Vec<Value>, String> {
    match name {
        // --- 项目管理 ---
        "open_project" => tool_open_project(arguments, state),
        "close_project" => tool_close_project(arguments, state),
        "get_project_info" => tool_get_project_info(arguments, state),
        "import_document" => tool_import_document(arguments, state),

        // --- 目录与结构 ---
        "get_toc" => tool_get_toc(arguments, state),
        "get_page_stats" => tool_get_page_stats(arguments, state),

        // --- 语义块操作 ---
        "get_blocks" => tool_get_blocks(arguments, state),
        "get_block" => tool_get_block(arguments, state),
        "get_blocks_by_page" => tool_get_blocks_by_page(arguments, state),
        "update_block" => tool_update_block(arguments, state),

        // --- 搜索 ---
        "search_blocks" => tool_search_blocks(arguments, state),

        // --- 资源文件 ---
        "list_assets" => tool_list_assets(arguments, state),
        "find_asset" => tool_find_asset(arguments, state),

        _ => Err(format!("Unknown tool: {}", name)),
    }
}

// ---------------------------------------------------------------------------
// 工具实现：项目管理
// ---------------------------------------------------------------------------

fn get_db_conn(state: &McpState) -> Result<rusqlite::Connection, String> {
    // MCP standalone 模式下，直接从 project_path 打开数据库
    let path = {
        let guard = state.project_path.lock().map_err(|e| e.to_string())?;
        guard.clone().ok_or("No project is currently open. Use open_project first.")?
    };
    rusqlite::Connection::open(format!("{}/narrative.db", path))
        .map_err(|e| format!("Cannot open database: {}", e))
}

fn tool_open_project(args: &Value, state: &McpState) -> Result<Vec<Value>, String> {
    let path = args.get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: path")?;

    // 验证目录和数据库文件
    let db_path = format!("{}/narrative.db", path);
    if !std::path::Path::new(&db_path).exists() {
        return Err(format!("Database file not found: {}. Please use a valid project directory.", db_path));
    }

    let mut p = state.project_path.lock().map_err(|e| e.to_string())?;
    *p = Some(path.to_string());

    Ok(vec![json!({ "type": "text", "text": format!("Project opened: {}", path) })])
}

fn tool_close_project(_args: &Value, state: &McpState) -> Result<Vec<Value>, String> {
    let mut p = state.project_path.lock().map_err(|e| e.to_string())?;
    let prev = p.take();
    match prev {
        Some(path) => Ok(vec![json!({ "type": "text", "text": format!("Project closed: {}", path) })]),
        None => Ok(vec![json!({ "type": "text", "text": "No project was open".to_string() })]),
    }
}

fn tool_get_project_info(_args: &Value, state: &McpState) -> Result<Vec<Value>, String> {
    let path = {
        let guard = state.project_path.lock().map_err(|e| e.to_string())?;
        guard.clone().ok_or("No project is currently open")?
    };

    let conn = get_db_conn(state)?;
    let block_count: i64 = conn.query_row("SELECT COUNT(*) FROM blocks", [], |r| r.get(0))
        .unwrap_or(0);
    let heading_count: i64 = conn.query_row("SELECT COUNT(*) FROM blocks WHERE block_type='heading'", [], |r| r.get(0))
        .unwrap_or(0);

    Ok(vec![json!({
        "type": "text",
        "text": format!(
            "Project: {}\nTotal blocks: {}\nHeadings: {}",
            path, block_count, heading_count
        )
    })])
}

fn tool_import_document(args: &Value, _state: &McpState) -> Result<Vec<Value>, String> {
    let zip_path = args.get("zip_path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: zip_path")?;

    // MCP standalone 模式下不支持 import（需要 Tauri AppHandle 发送进度事件）
    // 建议用户使用 GUI 或 CLI 进行导入操作
    Ok(vec![json!({
        "type": "text",
        "text": format!(
            "Import via MCP is not directly supported. \
             Please use the GUI 'Import' button or CLI 'narrative-cli import {}' to import this document.",
            zip_path
        )
    })])
}

// ---------------------------------------------------------------------------
// 工具实现：目录与结构
// ---------------------------------------------------------------------------

fn tool_get_toc(_args: &Value, state: &McpState) -> Result<Vec<Value>, String> {
    let conn = get_db_conn(state)?;

    let mut stmt = conn.prepare(
        "SELECT id, parent_id, order_idx, level, block_type, \
         substr(content, 1, 80) as content_preview \
         FROM blocks WHERE block_type = 'heading' \
         ORDER BY order_idx"
    ).map_err(|e| e.to_string())?;

    let flat: Vec<TocNode> = stmt.query_map([], |row| {
        Ok(TocNode {
            id: row.get(0)?,
            parent_id: row.get(1)?,
            order_idx: row.get(2)?,
            level: row.get(3)?,
            block_type: row.get(4)?,
            content_preview: row.get(5)?,
            children: vec![],
        })
    }).map_err(|e| e.to_string())?
    .filter_map(|r| r.ok())
    .collect();

    let tree = build_toc_tree(flat);
    let tree_json = serde_json::to_value(tree).unwrap_or(json!([]));

    Ok(vec![json!({ "type": "text", "text": format!("TOC tree ({} root nodes):\n{}", tree_json.as_array().map(|a| a.len()).unwrap_or(0), tree_json) })])
}

/// 递归构建 TOC 树（复用 db_engine 的逻辑）
fn build_toc_tree(flat_nodes: Vec<TocNode>) -> Vec<TocNode> {
    let mut roots: Vec<TocNode> = vec![];
    let mut children_map: std::collections::HashMap<String, Vec<TocNode>> = std::collections::HashMap::new();

    for node in flat_nodes {
        match &node.parent_id {
            Some(pid) => {
                children_map.entry(pid.clone()).or_default().push(node);
            }
            None => {
                roots.push(node);
            }
        }
    }

    fn attach_children(nodes: &mut Vec<TocNode>, map: &std::collections::HashMap<String, Vec<TocNode>>) {
        for node in nodes.iter_mut() {
            if let Some(children) = map.get(&node.id) {
                let mut child_nodes = children.clone();
                attach_children(&mut child_nodes, map);
                node.children = child_nodes;
            }
        }
    }

    attach_children(&mut roots, &children_map);
    roots
}

fn tool_get_page_stats(_args: &Value, state: &McpState) -> Result<Vec<Value>, String> {
    let conn = get_db_conn(state)?;

    let mut stmt = conn.prepare(
        "SELECT CAST(json_extract(metadata, '$.page') AS INTEGER) as page, COUNT(*) as cnt \
         FROM blocks WHERE CAST(json_extract(metadata, '$.page') AS INTEGER) > 0 \
         GROUP BY page ORDER BY page"
    ).map_err(|e| e.to_string())?;

    let stats: Vec<(i32, i32)> = stmt.query_map([], |row| {
        Ok((row.get(0)?, row.get(1)?))
    }).map_err(|e| e.to_string())?
    .filter_map(|r| r.ok())
    .collect();

    let total_pages = stats.len();
    let total_blocks: i32 = stats.iter().map(|(_, c)| c).sum();

    Ok(vec![json!({
        "type": "text",
        "text": format!("Page statistics: {} pages, {} total blocks\nDetails: {:?}", total_pages, total_blocks, stats)
    })])
}

// ---------------------------------------------------------------------------
// 工具实现：语义块操作
// ---------------------------------------------------------------------------

fn tool_get_blocks(args: &Value, state: &McpState) -> Result<Vec<Value>, String> {
    let conn = get_db_conn(state)?;
    let parent_id: Option<String> = args.get("parent_id").and_then(|v| v.as_str()).map(|s| s.to_string());
    let limit: i32 = args.get("limit").and_then(|v| v.as_i64()).map(|v| v as i32).unwrap_or(100);
    let offset: i32 = args.get("offset").and_then(|v| v.as_i64()).map(|v| v as i32).unwrap_or(0);

    let query = match &parent_id {
        Some(_) => "SELECT id, parent_id, order_idx, level, block_type, content, original_content, \
                    metadata, version, created_at, updated_at \
                    FROM blocks WHERE parent_id = ?1 ORDER BY order_idx LIMIT ?2 OFFSET ?3",
        None => "SELECT id, parent_id, order_idx, level, block_type, content, original_content, \
                 metadata, version, created_at, updated_at \
                 FROM blocks WHERE parent_id IS NULL ORDER BY order_idx LIMIT ?2 OFFSET ?3",
    };

    let blocks: Vec<Block> = conn.prepare(query).map_err(|e| e.to_string())?
        .query_map(rusqlite::params![parent_id.as_deref().unwrap_or(""), limit, offset], |row| {
            Ok(Block {
                id: row.get(0)?, parent_id: row.get(1)?, order_idx: row.get(2)?,
                level: row.get(3)?, block_type: row.get(4)?, content: row.get(5)?,
                original_content: row.get(6)?, metadata: row.get(7)?, version: row.get(8)?,
                created_at: row.get(9)?, updated_at: row.get(10)?,
            })
        }).map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(vec![json!({ "type": "text", "text": format!("{} blocks found:\n{}", blocks.len(), serde_json::to_string_pretty(&blocks).unwrap_or_default()) })])
}

fn tool_get_block(args: &Value, state: &McpState) -> Result<Vec<Value>, String> {
    let id = args.get("id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: id")?;

    let conn = get_db_conn(state)?;
    let block: Block = conn.query_row(
        "SELECT id, parent_id, order_idx, level, block_type, content, original_content, \
                metadata, version, created_at, updated_at \
         FROM blocks WHERE id = ?1",
        rusqlite::params![id],
        |row| {
            Ok(Block {
                id: row.get(0)?, parent_id: row.get(1)?, order_idx: row.get(2)?,
                level: row.get(3)?, block_type: row.get(4)?, content: row.get(5)?,
                original_content: row.get(6)?, metadata: row.get(7)?, version: row.get(8)?,
                created_at: row.get(9)?, updated_at: row.get(10)?,
            })
        }
    ).map_err(|e| format!("Block not found: {}", e))?;

    Ok(vec![json!({ "type": "text", "text": serde_json::to_string_pretty(&block).unwrap_or_default() })])
}

fn tool_get_blocks_by_page(args: &Value, state: &McpState) -> Result<Vec<Value>, String> {
    let page_start: i32 = args.get("page_start")
        .and_then(|v| v.as_i64()).map(|v| v as i32)
        .ok_or("Missing required parameter: page_start")?;
    let page_end: i32 = args.get("page_end")
        .and_then(|v| v.as_i64()).map(|v| v as i32)
        .ok_or("Missing required parameter: page_end")?;

    let conn = get_db_conn(state)?;
    let blocks: Vec<Block> = conn.prepare(
        "SELECT id, parent_id, order_idx, level, block_type, content, original_content, \
                metadata, version, created_at, updated_at \
         FROM blocks \
         WHERE CAST(json_extract(metadata, '$.page') AS INTEGER) BETWEEN ?1 AND ?2 \
         ORDER BY order_idx"
    ).map_err(|e| e.to_string())?
    .query_map(rusqlite::params![page_start, page_end], |row| {
        Ok(Block {
            id: row.get(0)?, parent_id: row.get(1)?, order_idx: row.get(2)?,
            level: row.get(3)?, block_type: row.get(4)?, content: row.get(5)?,
            original_content: row.get(6)?, metadata: row.get(7)?, version: row.get(8)?,
            created_at: row.get(9)?, updated_at: row.get(10)?,
        })
    }).map_err(|e| e.to_string())?
    .filter_map(|r| r.ok())
    .collect();

    Ok(vec![json!({ "type": "text", "text": format!("{} blocks on pages {}-{}:\n{}", blocks.len(), page_start, page_end, serde_json::to_string_pretty(&blocks).unwrap_or_default()) })])
}

fn tool_update_block(args: &Value, state: &McpState) -> Result<Vec<Value>, String> {
    let id = args.get("id").and_then(|v| v.as_str()).ok_or("Missing required parameter: id")?;
    let content = args.get("content").and_then(|v| v.as_str()).ok_or("Missing required parameter: content")?;
    let version: i32 = args.get("version").and_then(|v| v.as_i64()).map(|v| v as i32)
        .ok_or("Missing required parameter: version")?;

    let conn = get_db_conn(state)?;
    let affected = conn.execute(
        "UPDATE blocks SET content = ?1, version = version + 1, updated_at = CURRENT_TIMESTAMP \
         WHERE id = ?2 AND version = ?3",
        rusqlite::params![content, id, version]
    ).map_err(|e| e.to_string())?;

    if affected == 0 {
        return Err("Version conflict: the block has been modified by another operation. Please refresh and try again.".to_string());
    }

    Ok(vec![json!({ "type": "text", "text": format!("Block '{}' updated successfully (version {} -> {})", id, version, version + 1) })])
}

// ---------------------------------------------------------------------------
// 工具实现：搜索
// ---------------------------------------------------------------------------

fn tool_search_blocks(args: &Value, state: &McpState) -> Result<Vec<Value>, String> {
    let query = args.get("query")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: query")?;
    let limit: i32 = args.get("limit").and_then(|v| v.as_i64()).map(|v| v as i32).unwrap_or(20);

    let conn = get_db_conn(state)?;
    let blocks: Vec<Block> = conn.prepare(
        "SELECT b.id, b.parent_id, b.order_idx, b.level, b.block_type, \
                b.content, b.original_content, b.metadata, b.version, b.created_at, b.updated_at \
         FROM blocks b \
         INNER JOIN blocks_fts fts ON b.rowid = fts.rowid \
         WHERE blocks_fts MATCH ?1 \
         ORDER BY rank LIMIT ?2"
    ).map_err(|e| e.to_string())?
    .query_map(rusqlite::params![query, limit], |row| {
        Ok(Block {
            id: row.get(0)?, parent_id: row.get(1)?, order_idx: row.get(2)?,
            level: row.get(3)?, block_type: row.get(4)?, content: row.get(5)?,
            original_content: row.get(6)?, metadata: row.get(7)?, version: row.get(8)?,
            created_at: row.get(9)?, updated_at: row.get(10)?,
        })
    }).map_err(|e| e.to_string())?
    .filter_map(|r| r.ok())
    .collect();

    Ok(vec![json!({ "type": "text", "text": format!("Search '{}' returned {} results:\n{}", query, blocks.len(), serde_json::to_string_pretty(&blocks).unwrap_or_default()) })])
}

// ---------------------------------------------------------------------------
// 工具实现：资源文件
// ---------------------------------------------------------------------------

fn tool_list_assets(_args: &Value, state: &McpState) -> Result<Vec<Value>, String> {
    let path = {
        let guard = state.project_path.lock().map_err(|e| e.to_string())?;
        guard.clone().ok_or("No project is currently open")?
    };

    let assets_dir = std::path::Path::new(&path).join("assets");
    if !assets_dir.exists() {
        return Ok(vec![json!({ "type": "text", "text": "No assets directory found".to_string() })]);
    }

    let mut files: Vec<String> = Vec::new();
    collect_files(&assets_dir, &assets_dir, &mut files);
    files.sort();

    Ok(vec![json!({ "type": "text", "text": format!("{} files in assets:\n{}", files.len(), files.join("\n")) })])
}

fn collect_files(base: &std::path::Path, dir: &std::path::Path, out: &mut Vec<String>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_files(base, &path, out);
            } else {
                if let Ok(rel) = path.strip_prefix(base) {
                    out.push(rel.display().to_string());
                }
            }
        }
    }
}

fn tool_find_asset(args: &Value, state: &McpState) -> Result<Vec<Value>, String> {
    let pattern = args.get("pattern")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: pattern")?;

    let path = {
        let guard = state.project_path.lock().map_err(|e| e.to_string())?;
        guard.clone().ok_or("No project is currently open")?
    };

    let assets_dir = std::path::Path::new(&path).join("assets");
    let mut result: Option<String> = None;
    find_matching_file(&assets_dir, pattern, &mut result);

    match result {
        Some(f) => Ok(vec![json!({ "type": "text", "text": format!("Found: {}", f) })]),
        None => Ok(vec![json!({ "type": "text", "text": format!("No file matching '{}' found in assets", pattern) })]),
    }
}

fn find_matching_file(dir: &std::path::Path, pattern: &str, out: &mut Option<String>) {
    if out.is_some() { return; }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                find_matching_file(&path, pattern, out);
            } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.contains(pattern) {
                    *out = Some(path.display().to_string());
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_tools_returns_tools() {
        let tools = list_tools();
        assert!(tools.len() >= 10);
        let names: Vec<&str> = tools.iter()
            .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
            .collect();
        assert!(names.contains(&"open_project"));
        assert!(names.contains(&"get_toc"));
        assert!(names.contains(&"search_blocks"));
    }

    #[test]
    fn test_unknown_tool_returns_error() {
        let state = McpState::new();
        let result = call_tool("nonexistent", &json!({}), &state);
        assert!(result.is_err());
    }
}