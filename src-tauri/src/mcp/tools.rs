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
        // --- PDF 翻页 ---
        json!({
            "name": "get_total_pages",
            "description": "获取 PDF 文档的总页数",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        json!({
            "name": "get_page_content",
            "description": "获取指定 PDF 页的完整内容（该页所有语义块的文字内容拼接）",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "page": {
                        "type": "integer",
                        "description": "PDF 页码（从 1 开始）"
                    }
                },
                "required": ["page"]
            }
        }),
        json!({
            "name": "get_page_preview",
            "description": "获取指定 PDF 页的内容预览（每个块只显示前 200 字符）",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "page": {
                        "type": "integer",
                        "description": "PDF 页码（从 1 开始）"
                    }
                },
                "required": ["page"]
            }
        }),
        json!({
            "name": "navigate_page",
            "description": "翻页导航。返回指定页的内容预览，以及上一页/下一页的页码信息，方便外部智能体进行翻页浏览",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "page": {
                        "type": "integer",
                        "description": "当前页码（从 1 开始）"
                    },
                    "direction": {
                        "type": "string",
                        "description": "翻页方向：'next' 下一页, 'prev' 上一页, 'current' 只看当前页",
                        "enum": ["next", "prev", "current"],
                        "default": "current"
                    }
                },
                "required": ["page"]
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
        // --- 系统工具 ---
        json!({
            "name": "screenshot",
            "description": "截取当前应用界面。MCP 独立进程无法直接访问前端 GUI，请通过 eval_queue 调用 Tauri 的 capture_window 命令: echo 'window.__TAURI__.core.invoke(\"capture_window\")' > /tmp/narrative-eval-queue.txt",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        // --- 前端诊断工具 ---
        // 这些工具通过 eval_queue 文件队列向运行中的 Tauri 应用注入 JavaScript
        // 使用方法: echo 'JS_CODE' > /tmp/narrative-eval-queue.txt
        json!({
            "name": "get_page_text",
            "description": "获取当前页面所有可见文本内容。通过 eval_queue 注入 JS: echo 'document.body.innerText.substring(0,5000)' > /tmp/narrative-eval-queue.txt。用于快速了解当前显示了什么内容、哪个项目被打开等。需要 App 正在运行。",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        json!({
            "name": "get_page_html",
            "description": "获取当前页面 HTML 结构。通过 eval_queue 注入 JS: echo 'document.body.outerHTML.substring(0,20000)' > /tmp/narrative-eval-queue.txt。用于详细分析 DOM 结构、组件状态等。需要 App 正在运行。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "可选：CSS 选择器，只获取特定区域"
                    },
                    "max_length": {
                        "type": "integer",
                        "description": "最大返回字符数（默认 20000）",
                        "default": 20000
                    }
                }
            }
        }),
        json!({
            "name": "get_console_logs",
            "description": "获取前端控制台日志。通过 eval_queue 执行诊断脚本: echo 'window.pageControllerBridge?.getState()' > /tmp/narrative-eval-queue.txt。需要 App 正在运行。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "type": {
                        "type": "string",
                        "description": "日志类型：all, error, warning, log, info, debug",
                        "default": "all"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "返回数量上限（默认 50）",
                        "default": 50
                    }
                }
            }
        }),
        json!({
            "name": "evaluate_js",
            "description": "通过 eval_queue 在 Tauri 应用中执行 JavaScript。使用方法: echo 'JS_CODE' > /tmp/narrative-eval-queue.txt。用于获取任意前端状态、检查变量值等。需要 App 正在运行 (npm run tauri dev)。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "script": {
                        "type": "string",
                        "description": "要执行的 JavaScript 代码，通过 echo '...' > /tmp/narrative-eval-queue.txt 注入"
                    }
                },
                "required": ["script"]
            }
        }),
        // --- Page Agent 集成：GUI 驱动 ---
        json!({
            "name": "page_get_state",
            "description": "获取当前页面的简化 DOM 状态（由 Page Agent 的 PageController 提供，不需要 LLM）。返回可交互元素的索引化列表，外部 Agent 可用这些索引调用 page_do_action 进行操作。返回格式: {url, title, header, content, footer} 其中 content 是简化 HTML，如 '[0]<button>Open</button>[1]<input placeholder=\"path\"/>'",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        json!({
            "name": "page_do_action",
            "description": "对当前页面执行操作（由 Page Agent 的 PageController 提供，不需要 LLM）。操作类型: click(点击元素), fill(填写输入框), scroll(滚动), select(选择下拉选项), execute_js(执行 JS)。target 为元素索引（从 page_get_state 获取，如 \"0\", \"1\"）。例如: {type:\"click\", target:\"0\"} 点击索引0的元素",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "type": {
                        "type": "string",
                        "description": "操作类型: click, fill, scroll, select, execute_js"
                    },
                    "target": {
                        "type": "string",
                        "description": "元素索引（从 page_get_state 获取，如 \"0\", \"1\"）"
                    },
                    "value": {
                        "type": "string",
                        "description": "fill 时填写的文本, select 时选择的选项, execute_js 时执行的脚本"
                    },
                    "scroll_down": {
                        "type": "boolean",
                        "description": "scroll 时是否向下滚动（默认 true）"
                    },
                    "pixels": {
                        "type": "integer",
                        "description": "scroll 时滚动的像素数"
                    }
                },
                "required": ["type"]
            }
        }),
        json!({
            "name": "page_screenshot",
            "description": "截取当前页面截图，返回 base64 编码的 PNG 图片。用于确认 page_do_action 操作后的页面状态。",
            "inputSchema": {
                "type": "object",
                "properties": {}
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

        // --- PDF 翻页 ---
        "get_total_pages" => tool_get_total_pages(arguments, state),
        "get_page_content" => tool_get_page_content(arguments, state),
        "get_page_preview" => tool_get_page_preview(arguments, state),
        "navigate_page" => tool_navigate_page(arguments, state),

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

        // --- 系统工具 ---
        "screenshot" => tool_screenshot(arguments, state),

        // --- Page Agent 集成 ---
        "page_get_state" => tool_page_get_state(arguments, state),
        "page_do_action" => tool_page_do_action(arguments, state),
        "page_screenshot" => tool_page_screenshot(arguments, state),

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
// 工具实现：PDF 翻页
// ---------------------------------------------------------------------------

/// 获取 PDF 总页数
fn tool_get_total_pages(_args: &Value, state: &McpState) -> Result<Vec<Value>, String> {
    let conn = get_db_conn(state)?;
    let total_pages: i32 = conn.query_row(
        "SELECT MAX(CAST(json_extract(metadata, '$.page') AS INTEGER)) FROM blocks",
        [],
        |r| r.get(0)
    ).unwrap_or(0);

    Ok(vec![json!({
        "type": "text",
        "text": format!("Total pages: {}", total_pages)
    })])
}

/// 获取指定页的完整内容
fn tool_get_page_content(args: &Value, state: &McpState) -> Result<Vec<Value>, String> {
    let page: i32 = args.get("page")
        .and_then(|v| v.as_i64()).map(|v| v as i32)
        .ok_or("Missing required parameter: page")?;

    let conn = get_db_conn(state)?;
    
    // 获取该页所有块的内容，按 order_idx 排序拼接
    let mut stmt = conn.prepare(
        "SELECT content, block_type FROM blocks \
         WHERE CAST(json_extract(metadata, '$.page') AS INTEGER) = ?1 \
         ORDER BY order_idx"
    ).map_err(|e| e.to_string())?;

    let contents: Vec<(String, String)> = stmt.query_map(rusqlite::params![page], |row| {
        Ok((row.get(0)?, row.get(1)?))
    }).map_err(|e| e.to_string())?
    .filter_map(|r| r.ok())
    .collect();

    let mut result = String::new();
    for (content, block_type) in &contents {
        match block_type.as_str() {
            "heading" => {
                result.push_str(&format!("\n# {}\n", content));
            }
            "paragraph" | "text" => {
                result.push_str(&format!("{}\n", content));
            }
            "list_item" => {
                result.push_str(&format!("- {}\n", content));
            }
            "table" => {
                result.push_str(&format!("\n{}\n", content));
            }
            _ => {
                result.push_str(&format!("{}\n", content));
            }
        }
    }

    if result.is_empty() {
        return Ok(vec![json!({
            "type": "text",
            "text": format!("Page {} has no content blocks", page)
        })]);
    }

    Ok(vec![json!({
        "type": "text",
        "text": format!("=== Page {} ===\n{}", page, result.trim())
    })])
}

/// 获取指定页的内容预览（每个块只显示前 200 字符）
fn tool_get_page_preview(args: &Value, state: &McpState) -> Result<Vec<Value>, String> {
    let page: i32 = args.get("page")
        .and_then(|v| v.as_i64()).map(|v| v as i32)
        .ok_or("Missing required parameter: page")?;

    let conn = get_db_conn(state)?;

    let mut stmt = conn.prepare(
        "SELECT block_type, substr(content, 1, 200) as preview FROM blocks \
         WHERE CAST(json_extract(metadata, '$.page') AS INTEGER) = ?1 \
         ORDER BY order_idx"
    ).map_err(|e| e.to_string())?;

    let previews: Vec<(String, String)> = stmt.query_map(rusqlite::params![page], |row| {
        Ok((row.get(0)?, row.get(1)?))
    }).map_err(|e| e.to_string())?
    .filter_map(|r| r.ok())
    .collect();

    let mut result = String::new();
    for (block_type, preview) in &previews {
        let marker = match block_type.as_str() {
            "heading" => "📑",
            "paragraph" | "text" => "📝",
            "list_item" => "🔹",
            "table" => "📊",
            "image" => "🖼️",
            _ => "📄",
        };
        result.push_str(&format!("  {} {}\n", marker, preview));
    }

    if result.is_empty() {
        return Ok(vec![json!({
            "type": "text",
            "text": format!("Page {} has no content blocks", page)
        })]);
    }

    Ok(vec![json!({
        "type": "text",
        "text": format!("=== Page {} (preview) ===\n{}", page, result.trim())
    })])
}

/// 翻页导航 - 返回当前页预览 + 上下页导航信息
fn tool_navigate_page(args: &Value, state: &McpState) -> Result<Vec<Value>, String> {
    let page: i32 = args.get("page")
        .and_then(|v| v.as_i64()).map(|v| v as i32)
        .ok_or("Missing required parameter: page")?;
    let direction: String = args.get("direction")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "current".to_string());

    // 获取总页数
    let conn = get_db_conn(state)?;
    let total_pages: i32 = conn.query_row(
        "SELECT MAX(CAST(json_extract(metadata, '$.page') AS INTEGER)) FROM blocks",
        [],
        |r| r.get(0)
    ).unwrap_or(0);

    // 获取该页的预览
    let mut stmt = conn.prepare(
        "SELECT block_type, substr(content, 1, 200) as preview FROM blocks \
         WHERE CAST(json_extract(metadata, '$.page') AS INTEGER) = ?1 \
         ORDER BY order_idx"
    ).map_err(|e| e.to_string())?;

    let previews: Vec<(String, String)> = stmt.query_map(rusqlite::params![page], |row| {
        Ok((row.get(0)?, row.get(1)?))
    }).map_err(|e| e.to_string())?
    .filter_map(|r| r.ok())
    .collect();

    let mut page_content = String::new();
    for (block_type, preview) in &previews {
        let marker = match block_type.as_str() {
            "heading" => "📑",
            "paragraph" | "text" => "📝",
            "list_item" => "🔹",
            "table" => "📊",
            "image" => "🖼️",
            _ => "📄",
        };
        page_content.push_str(&format!("  {} {}\n", marker, preview));
    }

    // 构建导航信息
    let mut result = format!(
        "📖 Page {}/{}\n",
        page, total_pages
    );

    // 当前页内容
    if page_content.is_empty() {
        result.push_str("  (empty page)\n");
    } else {
        result.push_str(&page_content);
    }

    // 导航按钮
    result.push('\n');
    if page > 1 {
        result.push_str(&format!("⬅️  Previous: Page {}\n", page - 1));
    }
    if page < total_pages {
        result.push_str(&format!("➡️  Next: Page {}\n", page + 1));
    }

    // 如果是 next/prev 方向，也显示目标页的简要信息
    match direction.as_str() {
        "next" if page < total_pages => {
            let next_page = page + 1;
            let next_previews: Vec<(String, String)> = conn.prepare(
                "SELECT block_type, substr(content, 1, 100) as preview FROM blocks \
                 WHERE CAST(json_extract(metadata, '$.page') AS INTEGER) = ?1 \
                 ORDER BY order_idx LIMIT 3"
            ).map_err(|e| e.to_string())?
            .query_map(rusqlite::params![next_page], |row| {
                Ok((row.get(0)?, row.get(1)?))
            }).map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
            result.push_str(&format!("\n--- Preview of Page {} ---\n", next_page));
            for (bt, pv) in &next_previews {
                result.push_str(&format!("  [{}] {}\n", bt, pv));
            }
        }
        "prev" if page > 1 => {
            let prev_page = page - 1;
            let prev_previews: Vec<(String, String)> = conn.prepare(
                "SELECT block_type, substr(content, 1, 100) as preview FROM blocks \
                 WHERE CAST(json_extract(metadata, '$.page') AS INTEGER) = ?1 \
                 ORDER BY order_idx LIMIT 3"
            ).map_err(|e| e.to_string())?
            .query_map(rusqlite::params![prev_page], |row| {
                Ok((row.get(0)?, row.get(1)?))
            }).map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
            result.push_str(&format!("\n--- Preview of Page {} ---\n", prev_page));
            for (bt, pv) in &prev_previews {
                result.push_str(&format!("  [{}] {}\n", bt, pv));
            }
        }
        _ => {}
    }

    Ok(vec![json!({
        "type": "text",
        "text": result.trim().to_string()
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

// ---------------------------------------------------------------------------
// 工具实现：系统工具
// ---------------------------------------------------------------------------

/// 截图工具 - 使用前端 html2canvas，不依赖系统命令
/// MCP 独立进程无法直接访问前端窗口，返回使用说明
fn tool_screenshot(_args: &Value, _state: &McpState) -> Result<Vec<Value>, String> {
    Ok(vec![json!({
        "type": "text",
        "text": "Screenshot via MCP standalone is not directly supported.\
                 \n\nTo take a screenshot:\
                 \n  1. In the Tauri app, call: window.screenshot() \
                 \n  2. Or use the Tauri command: invoke('save_screenshot', { base64 })\
                 \n\nThis uses html2canvas (pure JavaScript) and does not depend on any OS commands."
    })])
}

// ---------------------------------------------------------------------------
// 工具实现：Page Agent 集成
// ---------------------------------------------------------------------------

/// 通过 eval-queue 在前端执行 JS，等待结果文件返回
/// 
/// 机制：
/// 1. MCP 后端将 JS 写入 /tmp/narrative-eval-queue.txt
/// 2. 前端 useEvalQueue 轮询 eval_js_queue → <script> 标签注入执行
/// 3. 注入脚本通过 dispatchEvent('eval-result') 发送结果
/// 4. 前端监听 eval-result 事件，调用 Tauri command 写结果文件
/// 5. MCP 后端轮询读取结果文件
/// 
/// 注意：注入脚本不能直接调用 window.__TAURI__（Tauri v2 安全限制），
/// 必须通过 CustomEvent 让前端处理。
fn eval_js_and_wait(script: &str) -> Result<String, String> {
    use std::fs;
    use std::thread;
    
    let queue_path = "/tmp/narrative-eval-queue.txt";
    let result_path = "/tmp/narrative-eval-result.txt";
    
    // Step 1: 清空旧结果
    fs::write(result_path, "").ok();
    
    // Step 2: 包装脚本：执行后 dispatchEvent 发送结果
    // 注入脚本通过 dispatchEvent 发送结果，前端监听并调用 Tauri command
    let full_script = format!(
        r#"(async()=>{{try{{const r=await({});window.dispatchEvent(new CustomEvent('eval-result',{{detail:r}}));}}catch(e){{window.dispatchEvent(new CustomEvent('eval-result',{{detail:{{error:e.message}}}}));}}}})()"#,
        script
    );
    fs::write(queue_path, &full_script)
        .map_err(|e| format!("Failed to write eval queue: {}", e))?;
    
    // Step 3: 等待前端消费（useEvalQueue 每 500ms 轮询）
    for _ in 0..10 {
        thread::sleep(std::time::Duration::from_millis(500));
        if fs::metadata(queue_path).map(|m| m.len() == 0).unwrap_or(false) {
            break; // 已被消费
        }
    }
    
    // Step 4: 等待结果文件（最多 5 秒）
    for _ in 0..50 {
        thread::sleep(std::time::Duration::from_millis(100));
        if let Ok(content) = fs::read_to_string(result_path) {
            if !content.trim().is_empty() {
                return Ok(content.trim().to_string());
            }
        }
    }
    
    Err("Timeout: frontend did not return Page Agent result. Make sure the Tauri app is running.".to_string())
}

/// page_get_state — 获取当前页面的简化 DOM 状态
fn tool_page_get_state(_args: &Value, _state: &McpState) -> Result<Vec<Value>, String> {
    let result = eval_js_and_wait(
        r#"window.pageControllerBridge.getState()"#
    )?;
    
    Ok(vec![json!({ "type": "text", "text": result })])
}

/// page_do_action — 对当前页面执行操作
fn tool_page_do_action(args: &Value, _state: &McpState) -> Result<Vec<Value>, String> {
    let action_type = args.get("type")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: type")?;
    
    let target = args.get("target")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let value = args.get("value")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let scroll_down = args.get("scroll_down")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let pixels = args.get("pixels")
        .and_then(|v| v.as_i64())
        .map(|v| v as i32);
    
    // 构建 JSON 参数
    let mut json_parts = vec![format!("type:{}", action_type)];
    if let Some(t) = &target {
        json_parts.push(format!("target:{}", t));
    }
    if let Some(v) = &value {
        json_parts.push(format!("value:{}", v));
    }
    if let Some(p) = &pixels {
        json_parts.push(format!("pixels:{}", p));
    }
    
    let action_json = format!(
        r#"{{type:"{}",target:{},value:{},scrollDown:{},pixels:{}}}"#,
        action_type,
        target.map(|t| format!("\"{}\"", t)).unwrap_or("null".to_string()),
        value.map(|v| format!("\"{}\"", v)).unwrap_or("null".to_string()),
        scroll_down,
        pixels.map(|p| p.to_string()).unwrap_or("null".to_string())
    );
    
    let result = eval_js_and_wait(
        &format!(r#"window.pageControllerBridge.doAction({})"#, action_json)
    )?;
    
    Ok(vec![json!({ "type": "text", "text": result })])
}

/// page_screenshot — 截取当前页面截图
fn tool_page_screenshot(_args: &Value, _state: &McpState) -> Result<Vec<Value>, String> {
    // 使用前端 window.screenshot() 函数（html2canvas）
    // 通过 dispatchEvent 发送结果
    let result = eval_js_and_wait(
        r#"window.screenshot()"#
    )?;
    
    Ok(vec![json!({ "type": "text", "text": result })])
}

/// 简单的 base64 编码（不依赖外部 crate）
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    let chunks = data.chunks(3);
    for chunk in chunks {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
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