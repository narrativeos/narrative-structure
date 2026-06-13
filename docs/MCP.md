# NarrativeStructure MCP Server

> Model Context Protocol (MCP) Server for NarrativeStructure — 让任何 AI 智能体可以结构化地操作 NarrativeStructure。

## 概述

NarrativeStructure 内置了一个 MCP Server（`narrative-structure-mcp`），通过 **stdio** 协议与外部 AI 智能体通信。这允许任何支持 MCP 协议的 AI 框架（如 Claude Desktop、Cursor、Cline 等）直接调用 NarrativeStructure 的功能。

## 架构

```
┌──────────────┐     stdio      ┌──────────────────────┐
│  AI Agent    │ ◄── JSON-RPC ──► │  narrative-structure-mcp │
│  (外部智能体) │                 │  (MCP Server)          │
└──────────────┘                 └────────┬─────────────┘
                                          │
                                          ▼
                                    ┌──────────────┐
                                    │ narrative.db  │
                                    │ (SQLite)      │
                                    └──────────────┘
```

## 快速开始

### 编译

```bash
cd narrative-structure/src-tauri
cargo build --bin narrative-structure-mcp --release
```

生成的二进制文件位于 `target/release/narrative-structure-mcp`。

### 基本使用

```bash
# 启动 MCP Server（交互式，等待 stdin 输入）
./target/release/narrative-structure-mcp

# 指定项目路径启动
./target/release/narrative-structure-mcp -p ~/.narrativeos/narrative-structure/Projects/MyDoc

# 查看帮助
./target/release/narrative-structure-mcp --help
```

### 手动测试

```bash
# 测试 initialize
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | \
  ./target/release/narrative-structure-mcp -p /path/to/project

# 列出所有可用工具
echo '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' | \
  ./target/release/narrative-structure-mcp -p /path/to/project

# 调用工具：获取目录
echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_toc","arguments":{}}}' | \
  ./target/release/narrative-structure-mcp -p /path/to/project
```

## 可用工具

### 项目管理

| 工具名 | 描述 | 参数 |
|--------|------|------|
| `open_project` | 打开已有项目 | `path: string` |
| `close_project` | 关闭当前项目 | 无 |
| `get_project_info` | 获取项目信息（路径、块数量等） | 无 |
| `import_document` | 导入 MinerU zip | `zip_path: string` |

### 文档结构

| 工具名 | 描述 | 参数 |
|--------|------|------|
| `get_toc` | 获取文档目录树 | 无 |
| `get_page_stats` | 统计每页的语义块数量 | 无 |

### 语义块操作

| 工具名 | 描述 | 参数 |
|--------|------|------|
| `get_blocks` | 获取语义块列表（支持分页） | `parent_id?`, `limit`, `offset` |
| `get_block` | 获取单个语义块 | `id: string` |
| `get_blocks_by_page` | 按 PDF 页码获取语义块 | `page_start`, `page_end` |
| `update_block` | 更新语义块内容（乐观锁） | `id`, `content`, `version` |

### 搜索

| 工具名 | 描述 | 参数 |
|--------|------|------|
| `search_blocks` | FTS5 全文搜索 | `query`, `limit?` |

### 资源文件

| 工具名 | 描述 | 参数 |
|--------|------|------|
| `list_assets` | 列出所有资源文件 | 无 |
| `find_asset` | 搜索匹配的资源文件 | `pattern: string` |

## 集成到 AI 框架

### Claude Desktop

在 `claude_desktop_config.json` 中添加：

```json
{
  "mcpServers": {
    "narrative-structure": {
      "command": "/path/to/narrative-structure-mcp",
      "args": ["-p", "/path/to/project"]
    }
  }
}
```

### Cursor

在 `.cursor/mcp.json` 中添加：

```json
{
  "mcpServers": {
    "narrative-structure": {
      "command": "/path/to/narrative-structure-mcp",
      "args": ["-p", "/path/to/project"]
    }
  }
}
```

## 协议规范

本 Server 遵循 [Model Context Protocol](https://modelcontextprotocol.io/specification) 规范：

- **传输层**: stdio（标准输入/输出）
- **消息格式**: JSON-RPC 2.0
- **支持的方法**:
  - `initialize` — 握手，返回 Server 信息
  - `tools/list` — 列出所有可用工具及其 schema
  - `tools/call` — 调用指定工具
  - `notifications/initialized` — 客户端初始化完成通知

## 安全考虑

- MCP Server 以独立进程运行，与主 GUI 进程隔离
- 数据库连接是只读的（除 `update_block` 外），遵循最小权限原则
- 所有操作通过 JSON-RPC 进行，可以审计和记录
- 不支持远程网络访问，仅支持本地 stdio 通信

## 开发

### 目录结构

```
src-tauri/src/
├── mcp/
│   ├── mod.rs          # 模块入口
│   ├── server.rs       # MCP Server 核心（JSON-RPC over stdio）
│   └── tools.rs        # 工具描述和执行
└── bin/
    └── narrative-structure-mcp.rs # 独立二进制入口
```

### 添加新工具

1. 在 `mcp/tools.rs` 的 `list_tools()` 中添加工具描述
2. 在 `call_tool()` 的 match 中添加路由
3. 实现工具函数（遵循 `fn tool_xxx(args, state) -> Result<Vec<Value>, String>` 签名）

## GUI 集成

### AgentConsole 面板

NarrativeStructure 的 GUI 右栏集成了 **AgentConsole** 面板，提供：

1. **MCP 工具列表**: 展示所有 13 个可用工具，按类别分组（项目管理、文档结构、语义块、搜索、资源文件）
2. **实时状态指示器**: 每 5 秒心跳检测 MCP Server 连接状态，显示延迟
3. **上下文管理**: 查看当前项目信息（名称、路径、语义块数量、MCP 模式、延迟）
4. **最近项目**: 快速打开最近使用的项目
5. **无障碍支持**: ARIA labels、键盘导航（Tab/Enter/Space）、焦点指示器

### 使用方式

在主界面的右侧栏中，点击"工具列表"或"上下文"标签切换视图。在"上下文"标签中，可以展开"当前项目"查看详情，或从"最近打开"列表快速切换项目。

## 后续计划

- **Phase 4**: 本地推理引擎（向量检索）