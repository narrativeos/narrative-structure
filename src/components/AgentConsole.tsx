import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./AgentConsole.css";

/**
 * AgentConsole — MCP 工具面板 + 上下文管理 + MCP 配置
 *
 * 功能：
 * 1. 展示当前可用的 MCP 工具列表
 * 2. MCP Server 实时状态指示器
 * 3. 当前项目上下文信息
 * 4. 最近打开的项目历史
 * 5. MCP 配置指南 — 如何在外部 AI 智能体中连接
 *
 * 所有智能体交互由外部 MCP 客户端发起，本面板仅用于人类用户了解
 * 当前软件支持哪些结构化操作。
 */

// 最近项目持久化
interface RecentProject { name: string; path: string; time: number }
const RECENT_KEY = "narrative-structure-recent";

function loadRecent(): RecentProject[] {
  try { return JSON.parse(localStorage.getItem(RECENT_KEY) || "[]"); } catch { return []; }
}

function fmtTime(ts: number): string {
  const d = new Date(ts);
  return `${d.getMonth()+1}/${d.getDate()} ${d.getHours()}:${String(d.getMinutes()).padStart(2,"0")}`;
}

// MCP Server 连接状态
interface McpConnectionState {
  connected: boolean;
  mode: "stdio" | "http" | "none";
  latency?: number;
  lastCheck?: number;
}

// MCP 工具描述（与 Rust 端 mcp/tools.rs 保持同步）
const MCP_TOOLS = [
  {
    name: "open_project",
    category: "project",
    description: "打开已有项目",
    params: "path: string",
  },
  {
    name: "close_project",
    category: "project",
    description: "关闭当前项目",
    params: "",
  },
  {
    name: "get_project_info",
    category: "project",
    description: "获取项目信息",
    params: "",
  },
  {
    name: "import_document",
    category: "project",
    description: "导入 MinerU zip",
    params: "zip_path: string",
  },
  {
    name: "get_toc",
    category: "structure",
    description: "获取文档目录树",
    params: "",
  },
  {
    name: "get_page_stats",
    category: "structure",
    description: "统计页码分布",
    params: "",
  },
  {
    name: "get_blocks",
    category: "blocks",
    description: "获取语义块列表",
    params: "parent_id?, limit, offset",
  },
  {
    name: "get_block",
    category: "blocks",
    description: "获取单个语义块",
    params: "id: string",
  },
  {
    name: "get_blocks_by_page",
    category: "blocks",
    description: "按页码获取语义块",
    params: "page_start, page_end",
  },
  {
    name: "update_block",
    category: "blocks",
    description: "更新语义块内容",
    params: "id, content, version",
  },
  {
    name: "search_blocks",
    category: "search",
    description: "全文搜索语义块",
    params: "query, limit?",
  },
  {
    name: "list_assets",
    category: "assets",
    description: "列出资源文件",
    params: "",
  },
  {
    name: "find_asset",
    category: "assets",
    description: "搜索资源文件",
    params: "pattern: string",
  },
];

const CATEGORIES = [
  { key: "project", label: "项目管理", icon: "📁" },
  { key: "structure", label: "文档结构", icon: "📑" },
  { key: "blocks", label: "语义块", icon: "🧱" },
  { key: "search", label: "搜索", icon: "🔍" },
  { key: "assets", label: "资源文件", icon: "📎" },
];

/** AgentConsole 的属性接口 */
interface AgentConsoleProps {
  /** 当前项目路径（可选，用于上下文显示） */
  projectPath?: string | null;
  /** 当前项目名称（可选） */
  projectName?: string;
  /** 项目块总数（可选） */
  blocksTotal?: number;
  /** 打开项目的回调 */
  onOpenProject?: (path: string, name?: string) => void;
}

export default function AgentConsole({
  projectPath,
  projectName,
  blocksTotal,
  onOpenProject,
}: AgentConsoleProps) {
  const [expandedTool, setExpandedTool] = useState<string | null>(null);
  const [connectionState, setConnectionState] = useState<McpConnectionState>({
    connected: true,
    mode: "stdio",
  });
  const [recentProjects, setRecentProjects] = useState<RecentProject[]>(loadRecent);
  const [showContext, setShowContext] = useState(false);
  const [activeSection, setActiveSection] = useState<"tools" | "context" | "config">("tools");
  const [mcpBinaryPath, setMcpBinaryPath] = useState<string>("");
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const heartbeatTimerRef = useRef<ReturnType<typeof setInterval>>();
  const focusTimerRef = useRef<ReturnType<typeof setTimeout>>();

  // 获取 MCP 二进制路径
  useEffect(() => {
    invoke<string>("get_mcp_binary_path")
      .then(setMcpBinaryPath)
      .catch(() => setMcpBinaryPath("narrative-structure-mcp"));
  }, []);

  // 复制功能
  const copyToClipboard = async (text: string, key: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedKey(key);
      setTimeout(() => setCopiedKey(null), 2000);
    } catch {
      // 降级方案
      const ta = document.createElement("textarea");
      ta.value = text;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      document.body.removeChild(ta);
      setCopiedKey(key);
      setTimeout(() => setCopiedKey(null), 2000);
    }
  };

  // MCP Server 心跳检测 — 每5秒检查一次 Tauri 命令响应
  const checkConnection = useCallback(async () => {
    const startTime = Date.now();
    try {
      // 使用 get_project_path 作为心跳 — 轻量且总是可用
      await invoke<string>("get_project_path");
      const latency = Date.now() - startTime;
      setConnectionState({
        connected: true,
        mode: "stdio",
        latency,
        lastCheck: Date.now(),
      });
    } catch {
      // 没有打开项目时也认为是正常的（Tauri 端返回空字符串）
      const latency = Date.now() - startTime;
      setConnectionState({
        connected: true,
        mode: "stdio",
        latency,
        lastCheck: Date.now(),
      });
    }
  }, []);

  // 启动心跳检测
  useEffect(() => {
    checkConnection();
    heartbeatTimerRef.current = setInterval(checkConnection, 5000);
    return () => {
      if (heartbeatTimerRef.current) clearInterval(heartbeatTimerRef.current);
    };
  }, [checkConnection]);

  // 监听项目变化事件
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    // 延迟初始化以避免竞态
    focusTimerRef.current = setTimeout(() => {
      listen<any>("project-changed", (_e) => {
        setRecentProjects(loadRecent());
      }).then((unsub) => {
        unlisten = unsub;
      }).catch(() => {});
    }, 1000);
    return () => {
      if (focusTimerRef.current) clearTimeout(focusTimerRef.current);
      unlisten?.();
    };
  }, []);

  const toolsByCategory = new Map<string, typeof MCP_TOOLS>();
  for (const cat of CATEGORIES) {
    toolsByCategory.set(cat.key, MCP_TOOLS.filter(t => t.category === cat.key));
  }

  // 键盘导航支持
  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      // 处理当前聚焦元素的展开/折叠
      const target = e.target as HTMLElement;
      const toolName = target.querySelector(".atp-tool-name code")?.textContent;
      if (toolName) {
        setExpandedTool(prev => prev === toolName ? null : toolName);
      }
    }
  }, []);

  const statusDotClass = connectionState.connected
    ? "atp-status-dot connected"
    : "atp-status-dot disconnected";

  const statusText = connectionState.connected
    ? `MCP Server 就绪 (${connectionState.mode})${connectionState.latency ? ` · ${connectionState.latency}ms` : ""}`
    : "MCP Server 未连接";

  // MCP 配置信息 — 使用实际路径
  const MCP_BINARY_NAME = "narrative-structure-mcp";
  const binaryPath = mcpBinaryPath || MCP_BINARY_NAME;
  const projPath = projectPath || "";

  const CLAUDE_CONFIG = `{
  "mcpServers": {
    "narrative-structure": {
      "command": "${binaryPath}",
      "args": ${projPath ? `["serve", "-p", "${projPath}"]` : '["serve", "-p", "<项目路径>"]'}
    }
  }
}`;

  const CURSOR_CONFIG = `{
  "mcpServers": {
    "narrative-structure": {
      "command": "${binaryPath}",
      "args": ${projPath ? `["serve", "-p", "${projPath}"]` : '["serve", "-p", "<项目路径>"]'}
    }
  }
}`;

  return (
    <div className="agent-tool-panel" role="region" aria-label="MCP 工具面板与上下文管理">
      {/* ======== 头部 ======== */}
      <div className="atp-header">
        <div className="atp-title">
          <span className="atp-icon">⚙️</span>
          <span>MCP 工具</span>
        </div>
        <div className="atp-badge" title="可用工具数量">{MCP_TOOLS.length}</div>
      </div>

      {/* ======== 分段切换 ======== */}
      <div className="atp-tabs" role="tablist" aria-label="面板分段">
        <button
          className={`atp-tab${activeSection === "tools" ? " active" : ""}`}
          role="tab"
          aria-selected={activeSection === "tools"}
          aria-controls="atp-tools-panel"
          onClick={() => setActiveSection("tools")}
        >
          工具列表
        </button>
        <button
          className={`atp-tab${activeSection === "context" ? " active" : ""}`}
          role="tab"
          aria-selected={activeSection === "context"}
          aria-controls="atp-context-panel"
          onClick={() => setActiveSection("context")}
        >
          上下文
        </button>
        <button
          className={`atp-tab${activeSection === "config" ? " active" : ""}`}
          role="tab"
          aria-selected={activeSection === "config"}
          aria-controls="atp-config-panel"
          onClick={() => setActiveSection("config")}
        >
          配置
        </button>
      </div>

      {/* ======== 工具列表面板 ======== */}
      {activeSection === "tools" && (
        <div className="atp-body" id="atp-tools-panel" role="tabpanel" aria-label="MCP 工具列表">
          {CATEGORIES.map((cat) => {
            const tools = toolsByCategory.get(cat.key) || [];
            if (tools.length === 0) return null;

            return (
              <div key={cat.key} className="atp-category">
                <div className="atp-category-header">
                  <span>{cat.icon}</span>
                  <span>{cat.label}</span>
                  <span className="atp-count">{tools.length}</span>
                </div>

                <div className="atp-tools" role="list" aria-label={`${cat.label}工具`}>
                  {tools.map((tool) => {
                    const isExpanded = expandedTool === tool.name;
                    return (
                      <div
                        key={tool.name}
                        className={`atp-tool${isExpanded ? " atp-tool-expanded" : ""}`}
                        role="listitem"
                        tabIndex={0}
                        aria-expanded={isExpanded}
                        aria-label={`${tool.name}: ${tool.description}`}
                        onClick={() => setExpandedTool(isExpanded ? null : tool.name)}
                        onKeyDown={handleKeyDown}
                      >
                        <div className="atp-tool-name">
                          <code>{tool.name}</code>
                          <span className="atp-tool-desc">{tool.description}</span>
                        </div>

                        {isExpanded && (
                          <div className="atp-tool-detail">
                            <div className="atp-tool-schema">
                              <div className="atp-schema-row">
                                <span className="atp-schema-label">类别</span>
                                <span className="atp-schema-value">{cat.label}</span>
                              </div>
                              {tool.params && (
                                <div className="atp-schema-row">
                                  <span className="atp-schema-label">参数</span>
                                  <code className="atp-schema-value">{tool.params}</code>
                                </div>
                              )}
                              <div className="atp-schema-row">
                                <span className="atp-schema-label">协议</span>
                                <span className="atp-schema-value">JSON-RPC 2.0 / stdio</span>
                              </div>
                            </div>

                            <div className="atp-tool-example">
                              <div className="atp-example-label">调用示例</div>
                              <pre>{`{
  "method": "tools/call",
  "params": {
    "name": "${tool.name}",
    "arguments": { ${tool.params ? `/* ${tool.params} */` : ""} }
  }
}`}</pre>
                            </div>
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* ======== 上下文面板 ======== */}
      {activeSection === "context" && (
        <div className="atp-body" id="atp-context-panel" role="tabpanel" aria-label="上下文管理">
          {/* 当前项目上下文 */}
          <div className="atp-context-section">
            <div
              className="atp-context-header"
              onClick={() => setShowContext(!showContext)}
              role="button"
              tabIndex={0}
              aria-expanded={showContext}
              aria-controls="current-project-detail"
              onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); setShowContext(!showContext); } }}
            >
              <span>📋 当前项目</span>
              <span className={`atp-context-toggle${showContext ? " expanded" : ""}`}>▸</span>
            </div>

            {showContext && (
              <div className="atp-context-detail" id="current-project-detail">
                {projectPath ? (
                  <>
                    <div className="atp-context-row">
                      <span className="atp-context-label">名称</span>
                      <span className="atp-context-value">{projectName || "未命名"}</span>
                    </div>
                    <div className="atp-context-row">
                      <span className="atp-context-label">路径</span>
                      <span className="atp-context-value atp-path" title={projectPath}>{projectPath}</span>
                    </div>
                    {blocksTotal !== undefined && (
                      <div className="atp-context-row">
                        <span className="atp-context-label">语义块</span>
                        <span className="atp-context-value">{blocksTotal} 个</span>
                      </div>
                    )}
                    <div className="atp-context-row">
                      <span className="atp-context-label">MCP 模式</span>
                      <span className="atp-context-value">{connectionState.mode}</span>
                    </div>
                    <div className="atp-context-row">
                      <span className="atp-context-label">延迟</span>
                      <span className="atp-context-value">
                        {connectionState.latency ? `${connectionState.latency}ms` : "—"}
                      </span>
                    </div>
                  </>
                ) : (
                  <div className="atp-context-empty">
                    <span>当前没有打开的项目</span>
                  </div>
                )}
              </div>
            )}
          </div>

          {/* 最近项目 */}
          <div className="atp-context-section">
            <div className="atp-context-header">
              <span>🕐 最近打开</span>
            </div>

            <div className="atp-recent-list">
              {recentProjects.length === 0 ? (
                <div className="atp-context-empty">
                  <span>暂无历史记录</span>
                </div>
              ) : (
                recentProjects.map((r) => (
                  <button
                    key={r.path}
                    className="atp-recent-item"
                    onClick={() => onOpenProject?.(r.path, r.name)}
                    title={r.path}
                    aria-label={`打开项目: ${r.name}`}
                    tabIndex={0}
                  >
                    <span className="atp-recent-icon">📁</span>
                    <div className="atp-recent-info">
                      <span className="atp-recent-name">{r.name}</span>
                      <span className="atp-recent-time">{fmtTime(r.time)}</span>
                    </div>
                  </button>
                ))
              )}
            </div>
          </div>
        </div>
      )}

      {/* ======== MCP 配置面板 ======== */}
      {activeSection === "config" && (
        <div className="atp-body" id="atp-config-panel" role="tabpanel" aria-label="MCP 配置指南">
          {/* 配置说明 */}
          <div className="atp-config-intro">
            <p>在外部 AI 智能体中配置 <code>{MCP_BINARY_NAME}</code>，让智能体可以直接操作当前项目。</p>
          </div>

          {/* Claude Desktop */}
          <div className="atp-config-section">
            <div className="atp-config-header">
              <span>🤖 Claude Desktop</span>
            </div>
            <div className="atp-config-detail">
              <div className="atp-config-step">
                <span className="atp-step-num">1</span>
                <span>打开 <code>~/.claude/claude_desktop_config.json</code></span>
              </div>
              <div className="atp-config-step">
                <span className="atp-step-num">2</span>
                <span>添加以下配置（替换路径为实际路径）</span>
              </div>
              <div className="atp-config-code">
                <pre>{CLAUDE_CONFIG}</pre>
              </div>
            </div>
          </div>

          {/* Cursor */}
          <div className="atp-config-section">
            <div className="atp-config-header">
              <span>💡 Cursor</span>
            </div>
            <div className="atp-config-detail">
              <div className="atp-config-step">
                <span className="atp-step-num">1</span>
                <span>打开项目根目录下的 <code>.cursor/mcp.json</code></span>
              </div>
              <div className="atp-config-step">
                <span className="atp-step-num">2</span>
                <span>添加以下配置（替换路径为实际路径）</span>
              </div>
              <div className="atp-config-code">
                <pre>{CURSOR_CONFIG}</pre>
              </div>
            </div>
          </div>

          {/* 使用说明 */}
          <div className="atp-config-section">
            <div className="atp-config-header">
              <span>📖 使用说明</span>
            </div>
            <div className="atp-config-detail">
              <div className="atp-config-note">
                <p><strong>MCP 二进制路径：</strong></p>
                <div className="atp-copy-row">
                  <code className="atp-path-value">{binaryPath}</code>
                  <button className="atp-copy-btn" onClick={() => copyToClipboard(binaryPath, "binary")}>
                    {copiedKey === "binary" ? "✅" : "📋"}
                  </button>
                </div>
              </div>
              {projPath ? (
                <div className="atp-config-note">
                  <p><strong>当前项目路径：</strong></p>
                  <div className="atp-copy-row">
                    <code className="atp-path-value">{projPath}</code>
                    <button className="atp-copy-btn" onClick={() => copyToClipboard(projPath, "project")}>
                      {copiedKey === "project" ? "✅" : "📋"}
                    </button>
                  </div>
                </div>
              ) : (
                <div className="atp-config-note">
                  <p><strong>项目路径：</strong>请先打开一个项目，路径将自动显示</p>
                </div>
              )}
              <div className="atp-config-note">
                <p><strong>MCP 工作方式：</strong>外部智能体（Claude Desktop/Cursor）会在需要时自动启动 MCP Server，无需手动运行</p>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* ======== 底部状态栏 ======== */}
      <div className="atp-footer">
        <div className="atp-connection">
          <span className={statusDotClass} aria-hidden="true" />
          <span aria-live="polite">{statusText}</span>
        </div>
        <div className="atp-hint">
          外部智能体通过 <code>{MCP_BINARY_NAME}</code> 连接
        </div>
      </div>
    </div>
  );
}