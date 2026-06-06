use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use crate::project_manager::ProjectState;

// ---------------------------------------------------------------------------
// 数据模型
// ---------------------------------------------------------------------------

/// 语义块 — 文档的最小操作单元
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub id: String,
    pub parent_id: Option<String>,
    pub order_idx: i32,
    pub level: i32,
    pub block_type: String,
    pub content: String,
    pub original_content: String,
    pub metadata: String,   // JSON string
    pub version: i32,
    pub created_at: String,
    pub updated_at: String,
}

/// 目录树节点 (轻量版，用于 TOC 渲染)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TocNode {
    pub id: String,
    pub parent_id: Option<String>,
    pub order_idx: i32,
    pub level: i32,
    pub block_type: String,
    pub content_preview: String,  // 前 80 字符
    pub children: Vec<TocNode>,
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 获取数据库连接
fn get_conn<'a>(state: &'a tauri::State<'a, ProjectState>) -> Result<std::sync::MutexGuard<'a, Option<Connection>>, String> {
    state.db_conn.lock().map_err(|e| e.to_string())
}

/// 内部获取单个块（供 get_block_chunk 等复用）
fn get_block_inner(state: &tauri::State<'_, ProjectState>, id: &str) -> Result<Block, String> {
    let conn_guard = state.db_conn.lock().map_err(|e| e.to_string())?;
    let conn = conn_guard.as_ref().ok_or("没有打开的项目")?;
    conn.query_row(
        "SELECT id, parent_id, order_idx, level, block_type, content, original_content, metadata,
                version, created_at, updated_at
         FROM blocks WHERE id = ?1",
        rusqlite::params![id],
        |row| {
            Ok(Block {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                order_idx: row.get(2)?,
                level: row.get(3)?,
                block_type: row.get(4)?,
                content: row.get(5)?,
                original_content: row.get(6)?,
                metadata: row.get(7)?,
                version: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        },
    )
    .map_err(|e| format!("块不存在: {}", e))
}

// ---------------------------------------------------------------------------
// Tauri Commands
// ---------------------------------------------------------------------------

/// 获取目录树 (所有 section，按层级构建树)
#[tauri::command]
pub fn get_toc(state: tauri::State<'_, ProjectState>) -> Result<Vec<TocNode>, String> {
    let conn_guard = get_conn(&state)?;
    let conn = conn_guard.as_ref().ok_or("没有打开的项目")?;

    let mut stmt = conn
        .prepare(
            "SELECT id, parent_id, order_idx, level, block_type,
                    substr(content, 1, 80) as content_preview
             FROM blocks
             WHERE block_type = 'heading'
             ORDER BY level, order_idx",
        )
        .map_err(|e| e.to_string())?;

    let nodes: Vec<TocNode> = stmt
        .query_map([], |row| {
            Ok(TocNode {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                order_idx: row.get(2)?,
                level: row.get(3)?,
                block_type: row.get(4)?,
                content_preview: row.get(5)?,
                children: vec![],
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    // 构建树形结构
    Ok(build_toc_tree(nodes))
}

/// 递归构建 TOC 树
fn build_toc_tree(flat_nodes: Vec<TocNode>) -> Vec<TocNode> {
    let mut roots: Vec<TocNode> = vec![];
    let mut children_map: std::collections::HashMap<String, Vec<TocNode>> =
        std::collections::HashMap::new();

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

    fn attach_children(
        nodes: &mut Vec<TocNode>,
        children_map: &std::collections::HashMap<String, Vec<TocNode>>,
    ) {
        for node in nodes.iter_mut() {
            if let Some(children) = children_map.get(&node.id) {
                let mut child_nodes = children.clone();
                attach_children(&mut child_nodes, children_map);
                node.children = child_nodes;
            }
        }
    }

    attach_children(&mut roots, &children_map);
    roots
}

/// 获取指定父块下的子块列表（分页）
#[tauri::command]
pub fn get_blocks(
    state: tauri::State<'_, ProjectState>,
    parent_id: Option<String>,
    limit: i32,
    offset: i32,
) -> Result<Vec<Block>, String> {
    let conn_guard = get_conn(&state)?;
    let conn = conn_guard.as_ref().ok_or("没有打开的项目")?;

    let query = match &parent_id {
        Some(_) => {
            "SELECT id, parent_id, order_idx, level, block_type, content, original_content, metadata,
                    version, created_at, updated_at
             FROM blocks
             WHERE parent_id = ?1
             ORDER BY order_idx
             LIMIT ?2 OFFSET ?3"
        }
        None => {
            "SELECT id, parent_id, order_idx, level, block_type, content, original_content, metadata,
                    version, created_at, updated_at
             FROM blocks
             WHERE parent_id IS NULL
             ORDER BY order_idx
             LIMIT ?2 OFFSET ?3"
        }
    };

    let mut stmt = conn.prepare(query).map_err(|e| e.to_string())?;

    let blocks: Vec<Block> = stmt
        .query_map(
            params![parent_id.as_deref().unwrap_or(""), limit, offset],
            |row| {
                Ok(Block {
                    id: row.get(0)?,
                    parent_id: row.get(1)?,
                    order_idx: row.get(2)?,
                    level: row.get(3)?,
                    block_type: row.get(4)?,
                    content: row.get(5)?,
                    original_content: row.get(6)?,
                    metadata: row.get(7)?,
                    version: row.get(8)?,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            },
        )
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(blocks)
}

/// 更新块内容（带乐观锁）
#[tauri::command]
pub fn update_block(
    state: tauri::State<'_, ProjectState>,
    id: String,
    content: String,
    expected_version: i32,
) -> Result<Block, String> {
    let conn_guard = get_conn(&state)?;
    let conn = conn_guard.as_ref().ok_or("没有打开的项目")?;

    let affected = conn
        .execute(
            "UPDATE blocks
             SET content = ?1,
                 version = version + 1,
                 updated_at = CURRENT_TIMESTAMP
             WHERE id = ?2 AND version = ?3",
            params![content, id, expected_version],
        )
        .map_err(|e| e.to_string())?;

    if affected == 0 {
        return Err("版本冲突: 该块已被其他操作修改，请刷新后重试".to_string());
    }

    // 返回更新后的块
    conn.query_row(
        "SELECT id, parent_id, order_idx, level, block_type, content, original_content, metadata,
                version, created_at, updated_at
         FROM blocks WHERE id = ?1",
        params![id],
        |row| {
            Ok(Block {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                order_idx: row.get(2)?,
                level: row.get(3)?,
                block_type: row.get(4)?,
                content: row.get(5)?,
                original_content: row.get(6)?,
                metadata: row.get(7)?,
                version: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

/// 全文搜索
#[tauri::command]
pub fn search_blocks(
    state: tauri::State<'_, ProjectState>,
    query: String,
    limit: i32,
) -> Result<Vec<Block>, String> {
    let conn_guard = get_conn(&state)?;
    let conn = conn_guard.as_ref().ok_or("没有打开的项目")?;

    let mut stmt = conn
        .prepare(
            "SELECT b.id, b.parent_id, b.order_idx, b.level, b.block_type,
                    b.content, b.original_content, b.metadata, b.version, b.created_at, b.updated_at
             FROM blocks b
             INNER JOIN blocks_fts fts ON b.rowid = fts.rowid
             WHERE blocks_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )
        .map_err(|e| e.to_string())?;

    let blocks: Vec<Block> = stmt
        .query_map(params![query, limit], |row| {
            Ok(Block {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                order_idx: row.get(2)?,
                level: row.get(3)?,
                block_type: row.get(4)?,
                content: row.get(5)?,
                original_content: row.get(6)?,
                metadata: row.get(7)?,
                version: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(blocks)
}

/// 按 PDF 页码范围加载块
#[tauri::command]
pub fn get_blocks_by_page(
    state: tauri::State<'_, ProjectState>,
    page_start: i32,
    page_end: i32,
) -> Result<Vec<Block>, String> {
    let conn_guard = get_conn(&state)?;
    let conn = conn_guard.as_ref().ok_or("没有打开的项目")?;

    let mut stmt = conn
        .prepare(
            "SELECT id, parent_id, order_idx, level, block_type, content, original_content, metadata,
                    version, created_at, updated_at
             FROM blocks
             WHERE CAST(json_extract(metadata, '$.page') AS INTEGER) BETWEEN ?1 AND ?2
             ORDER BY order_idx",
        )
        .map_err(|e| e.to_string())?;

    let blocks: Vec<Block> = stmt
        .query_map(params![page_start, page_end], |row| {
            Ok(Block {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                order_idx: row.get(2)?,
                level: row.get(3)?,
                block_type: row.get(4)?,
                content: row.get(5)?,
                original_content: row.get(6)?,
                metadata: row.get(7)?,
                version: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(blocks)
}

/// 获取所有块（按 order_idx 排序，分页）
#[tauri::command]
pub fn get_blocks_paginated(
    state: tauri::State<'_, ProjectState>,
    limit: i32,
    offset: i32,
) -> Result<Vec<Block>, String> {
    let conn_guard = get_conn(&state)?;
    let conn = conn_guard.as_ref().ok_or("没有打开的项目")?;

    let mut stmt = conn
        .prepare(
            "SELECT id, parent_id, order_idx, level, block_type, content, original_content, metadata,
                    version, created_at, updated_at
             FROM blocks
             ORDER BY order_idx
             LIMIT ?1 OFFSET ?2",
        )
        .map_err(|e| e.to_string())?;

    let blocks: Vec<Block> = stmt
        .query_map(params![limit, offset], |row| {
            Ok(Block {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                order_idx: row.get(2)?,
                level: row.get(3)?,
                block_type: row.get(4)?,
                content: row.get(5)?,
                original_content: row.get(6)?,
                metadata: row.get(7)?,
                version: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(blocks)
}

/// 获取单个块
#[tauri::command]
pub fn get_block(
    state: tauri::State<'_, ProjectState>,
    id: String,
) -> Result<Block, String> {
    let conn_guard = get_conn(&state)?;
    let conn = conn_guard.as_ref().ok_or("没有打开的项目")?;

    conn.query_row(
        "SELECT id, parent_id, order_idx, level, block_type, content, original_content, metadata,
                version, created_at, updated_at
         FROM blocks WHERE id = ?1",
        params![id],
        |row| {
            Ok(Block {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                order_idx: row.get(2)?,
                level: row.get(3)?,
                block_type: row.get(4)?,
                content: row.get(5)?,
                original_content: row.get(6)?,
                metadata: row.get(7)?,
                version: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        },
    )
    .map_err(|e| format!("块不存在: {}", e))
}

/// 获取块及其所有子孙块（扁平列表）
/// 注：section 级分块后，每个 block 已是完整章节
#[tauri::command]
pub fn get_block_chunk(
    state: tauri::State<'_, ProjectState>,
    id: String,
) -> Result<Vec<Block>, String> {
    let block = get_block_inner(&state, &id)?;
    Ok(vec![block])
}

/// 获取块的子块数量
#[tauri::command]
pub fn get_child_count(
    state: tauri::State<'_, ProjectState>,
    parent_id: Option<String>,
) -> Result<i32, String> {
    let conn_guard = get_conn(&state)?;
    let conn = conn_guard.as_ref().ok_or("没有打开的项目")?;

    let count: i32 = match &parent_id {
        Some(pid) => conn
            .query_row(
                "SELECT COUNT(*) FROM blocks WHERE parent_id = ?1",
                params![pid],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?,
        None => conn
            .query_row(
                "SELECT COUNT(*) FROM blocks WHERE parent_id IS NULL",
                [],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?,
    };

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_toc_tree_empty() {
        let result = build_toc_tree(vec![]);
        assert!(result.is_empty());
    }
}
