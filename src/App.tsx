import { useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import TOC from "./components/TOC";
import BlockEditor from "./components/Editor";
import MarkdownPreview from "./components/MarkdownPreview";
import FileExplorer from "./components/FileExplorer";
import PdfViewer from "./components/PdfViewer";
import AgentConsole from "./components/AgentConsole";
import PipelineStatus from "./components/PipelineStatus";
import LogPanel from "./components/LogPanel";
import Workspace from "./components/Workspace";
import LinesLayer, { LineDef } from "./components/LinesLayer";
import { useResizable } from "./hooks/useResizable";
import "./App.css";

export interface TocNode {
  id: string;
  parent_id: string | null;
  order_idx: number;
  level: number;
  block_type: string;
  content_preview: string;
  children: TocNode[];
}

export interface Block {
  id: string;
  parent_id: string | null;
  order_idx: number;
  level: number;
  block_type: string;
  content: string;
  original_content: string;
  metadata: string;
  version: number;
  created_at: string;
  updated_at: string;
}

function countNodes(node: TocNode): number {
  return 1 + node.children.reduce((s, c) => s + countNodes(c), 0);
}

// ---- 最近项目持久化 ----
interface ImportProgress { stage: string; percent: number; detail: string; }
interface RecentProject { name: string; path: string; time: number }
const RECENT_KEY = "narrative-structure-recent";

function loadRecent(): RecentProject[] {
  try { return JSON.parse(localStorage.getItem(RECENT_KEY) || "[]"); } catch { return []; }
}
function saveRecent(list: RecentProject[]) {
  localStorage.setItem(RECENT_KEY, JSON.stringify(list.slice(0, 10)));
}
function addRecent(name: string, path: string) {
  const list = loadRecent().filter(r => r.path !== path);
  list.unshift({ name, path, time: Date.now() });
  saveRecent(list);
}

function fmtTime(ts: number): string {
  const d = new Date(ts);
  return `${d.getMonth()+1}/${d.getDate()} ${d.getHours()}:${String(d.getMinutes()).padStart(2,"0")}`;
}

function App() {
  const [projectPath, setProjectPath] = useState<string | null>(null);
  const [projectName, setProjectName] = useState("");
  const [tocTree, setTocTree] = useState<TocNode[]>([]);
  const [activeBlock, setActiveBlock] = useState<Block | null>(null);
  const [pageBlocks, setPageBlocks] = useState<Block[] | null>(null);
  const [statusMsg, setStatusMsg] = useState("");
  const [recentProjects, setRecentProjects] = useState<RecentProject[]>(loadRecent);
  const refreshRecent = useCallback(() => setRecentProjects(loadRecent()), []);
  const [projectKey, setProjectKey] = useState(0);
  const pdfIframeRef = useRef<HTMLIFrameElement>(null);
  const [lines, _setLines] = useState<LineDef[]>([]);
  const [importProgress, setImportProgress] = useState<ImportProgress | null>(null);
  const [importLogs, setImportLogs] = useState<string[]>([]);

  // 可拖拽面板尺寸
  const [leftW, bindLeft] = useResizable(240, 160, 500);
  const [rightW, bindRight] = useResizable(220, 160, 400);

  const [, bindBottom] = useResizable(140, 60, 400, "y");

  // =========================================================================
  // 加载项目
  // =========================================================================
  const loadProject = useCallback(async (path: string, name?: string) => {
    try {
      const msg = await invoke<string>("open_project", { path });
      const pName = name || path.split("/").pop() || path;
      setProjectPath(path);
      setProjectName(pName);
      setProjectKey(k => k + 1);
      setStatusMsg(msg);
      addRecent(pName, path);
      const toc = await invoke<TocNode[]>("get_toc");
      setTocTree(toc);
    } catch (err) {
      setStatusMsg(`错误: ${err}`);
    }
  }, []);

  // =========================================================================
  // 导入文档 = 新建项目
  // =========================================================================
  const handleImportNewProject = useCallback(async () => {
    try {
      setStatusMsg("正在打开文件选择器...");
      const selected = await open({
        filters: [{ name: "ZIP 压缩包", extensions: ["zip"] }],
        multiple: false,
        title: "选择 MinerU 输出 zip 包",
      });

      if (!selected) {
        setStatusMsg("已取消选择");
        return;
      }

      const zipPath = typeof selected === "string" ? selected : String(selected);
      setStatusMsg(`正在导入: ${zipPath} ...`);
      setImportProgress({ stage: "准备中", percent: 0, detail: "正在初始化..." });
      setImportLogs([]);

      // 短暂延迟，让 React 先渲染初始进度条，再注册监听和调用导入
      await new Promise(r => setTimeout(r, 30));

      const unlistenProgress = await listen<ImportProgress>("import-progress", (e) => {
        console.log("[import]", e.payload.stage, e.payload.percent + "%", e.payload.detail);
        setImportProgress(e.payload);
      });
      const unlistenLog = await listen<string>("import-log", (e) => {
        setImportLogs(prev => [...prev.slice(-19), e.payload]);
      });
      console.log("[import] 监听已注册，开始导入...");

      const msg = await invoke<string>("import_new_project", {
        zipPath: zipPath,
      });

      // 保持进度条至少显示 600ms（防止一闪而过）
      await new Promise(r => setTimeout(r, 600));
      unlistenProgress();
      unlistenLog();
      setImportProgress(null);

      const parts = msg.split(" | ");
      const name = parts[0] || "";
      const pathPart = parts[2] || "";

      setProjectName(name);
      setProjectPath(pathPart.trim());
      setProjectKey(k => k + 1);
      setStatusMsg(msg);
      addRecent(name, pathPart.trim());

      const toc = await invoke<TocNode[]>("get_toc");
      setTocTree(toc);
    } catch (err) {
      setStatusMsg(`导入失败: ${err}`);
      setImportProgress(null);
      setImportLogs([]);
      setImportLogs([]);
    }
  }, []);

  // =========================================================================
  // 打开已有项目（原生文件夹选择）
  // =========================================================================
  const handleOpenProject = useCallback(async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "选择项目文件夹（包含 narrative.db）",
      });

      if (selected) {
        await loadProject(selected as string);
      }
    } catch (err) {
      setStatusMsg(`打开失败: ${err}`);
    }
  }, [loadProject]);

  // =========================================================================
  // 关闭当前项目 → 回到欢迎页
  // =========================================================================
  const handleCloseProject = useCallback(async () => {
    try { await invoke<string>("close_project"); } catch {}
    setProjectPath(null);
    setTocTree([]);
    setActiveBlock(null);
    setStatusMsg("");
    refreshRecent();
  }, [refreshRecent]);

  // =========================================================================
  // 导入 MinerU 输出 zip
  // =========================================================================
  const handleImportDocument = useCallback(async () => {
    try {
      const selected = await open({
        filters: [{ name: "ZIP 压缩包", extensions: ["zip"] }],
        multiple: false,
        title: "选择 MinerU 输出 zip 包",
      });

      if (!selected) return;

      setStatusMsg("正在导入...");
      setImportProgress({ stage: "准备中", percent: 0, detail: "正在初始化..." });
      setImportLogs([]);

      await new Promise(r => setTimeout(r, 30));

      const unlistenProgress = await listen<ImportProgress>("import-progress", (e) => {
        console.log("[import]", e.payload.stage, e.payload.percent + "%", e.payload.detail);
        setImportProgress(e.payload);
      });
      const unlistenLog = await listen<string>("import-log", (e) => {
        setImportLogs(prev => [...prev.slice(-19), e.payload]);
      });
      console.log("[import] 监听已注册，开始导入...");

      const msg = await invoke<string>("import_document", {
        zipPath: selected as string,
      });

      await new Promise(r => setTimeout(r, 600));
      unlistenProgress();
      unlistenLog();
      setImportProgress(null);

      setStatusMsg(msg);

      const toc = await invoke<TocNode[]>("get_toc");
      setTocTree(toc);
    } catch (err) {
      setStatusMsg(`导入失败: ${err}`);
      setImportProgress(null);
    }
  }, []);

  // PDF 翻页 → 加载当前页所有行块
  const handlePageChange = useCallback(async (page: number) => {
    try {
      const blocks = await invoke<Block[]>("get_blocks_by_page", {
        pageStart: page,
        pageEnd: page,
      });
      if (blocks.length > 0) {
        setPageBlocks(blocks);
        setActiveBlock(null); // 清除单块选中，进入页面模式
      }
    } catch {
      // 回退：用 order_idx 估算
      try {
        const blocks = await invoke<Block[]>("get_blocks_paginated", { limit: 1, offset: page - 1 });
        if (blocks.length > 0) setPageBlocks(blocks);
      } catch {}
    }
  }, []);
  const handleSelectBlock = useCallback(async (nodeId: string) => {
    try {
      const blocks = await invoke<Block[]>("get_block_chunk", { id: nodeId });
      if (blocks.length === 0) return;
      const head = blocks[0];
      setActiveBlock(head);
      setPageBlocks(null); // 切换到单块模式

      // 用 metadata.page 精确跳转 PDF
      let targetPage = 0;
      try {
        const meta = JSON.parse(head.metadata || "{}");
        if (meta.page) targetPage = meta.page as number;
      } catch {}
      if (targetPage > 0) {
        pdfIframeRef.current?.contentWindow?.postMessage({ type: "navigate", page: targetPage }, "*");
      }
    } catch (err) {
      setStatusMsg(`加载块失败: ${err}`);
    }
  }, []);

  const handleContentChange = useCallback(
    async (blockId: string, newContent: string, version: number) => {
      try {
        const updated = await invoke<Block>("update_block", {
          id: blockId,
          content: newContent,
          expectedVersion: version,
        });
        setActiveBlock(updated);
        setStatusMsg("已保存");
      } catch (err) {
        setStatusMsg(`保存失败: ${err}`);
      }
    },
    [],
  );

  // =========================================================================
  // 欢迎页 (无项目打开时)
  // =========================================================================
  if (!projectPath) {
    return (
      <div className="welcome-screen">
        <div className="welcome-card">
          <div className="welcome-icon">🧱</div>
          <h1 className="welcome-title">NarrativeStructure</h1>
          <p className="welcome-subtitle">文档智能化重构工作台</p>
          <p className="welcome-desc">
            导入 MinerU 输出的 zip 压缩包，自动解析为语义块并构建可编辑的文档树
          </p>

          <div className="welcome-actions">
            <button
              className="btn btn-primary"
              onClick={() => handleImportNewProject()}
            >
              📥 导入文档开始
            </button>
            <button className="btn btn-secondary" onClick={handleOpenProject}>
              📂 打开已有项目
            </button>
          </div>

          {statusMsg && <p className="welcome-status">{statusMsg}</p>}

          <div className="welcome-recent">
            <h4>最近打开</h4>
            {recentProjects.length === 0 ? (
              <p className="welcome-recent-empty">（暂无历史记录）</p>
            ) : (
              <div className="recent-list">
                {recentProjects.map((r) => (
                  <div
                    key={r.path}
                    className="recent-item"
                    onClick={() => loadProject(r.path, r.name)}
                    title={r.path}
                  >
                    <span className="recent-icon">📁</span>
                    <span className="recent-name">{r.name}</span>
                    <span className="recent-time">{fmtTime(r.time)}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>

        {importProgress && (
          <div className="import-overlay">
            <div className="import-progress-card">
              <div className="import-progress-stage">{importProgress.stage}</div>
              <div className="import-progress-bar-wrap">
                <div className="import-progress-bar" style={{ width: `${importProgress.percent}%` }} />
              </div>
              <div className="import-progress-detail">{importProgress.detail}</div>
              <div className="import-progress-pct">{importProgress.percent}%</div>
              {importLogs.length > 0 && (
                <div className="import-logs">
                  {importLogs.map((log, i) => (
                    <div key={i} className="import-log-line">{log}</div>
                  ))}
                </div>
              )}
            </div>
          </div>
        )}
      </div>
    );
  }

  // =========================================================================
  // 主界面
  // =========================================================================
  return (
    <div className="app-grid" style={{
      gridTemplateColumns: "1fr",
      gridTemplateRows: "40px 1fr 4px 140px",
      gridTemplateAreas: `"toolbar" "main" "hbot" "bottom"`,
    }}>
      <header className="toolbar">
        <h1 className="app-title">NarrativeStructure</h1>
        <div className="toolbar-project">
          <span className="project-path" title={projectPath}>📁 {projectName}</span>
          <button className="btn-close" onClick={handleCloseProject} title="关闭项目">✕</button>
        </div>
        <div className="toolbar-actions">
          <button className="btn-import" onClick={handleImportDocument} title="追加导入">📥</button>
          <span className="status-msg">{statusMsg}</span>
        </div>
      </header>

      <div style={{ gridArea: "main", overflow: "hidden" }}>
        <Workspace>
          {/* 左侧：TOC + 文件资产 */}
          <div className="workspace-left" style={{ width: leftW }}>
            <div className="toc-section">
              <h3>📑 语义目录 ({tocTree.reduce((s, n) => s + countNodes(n), 0)})</h3>
              <TOC nodes={tocTree} onSelect={handleSelectBlock} />
            </div>
            <div className="files-section">
              <FileExplorer projectPath={projectPath} />
            </div>
          </div>

          <div className="workspace-resize-h" {...bindLeft()} />

          {/* 中间三列：PDF | Block列表 | Markdown编辑器 */}
          <div className="workspace-center">
            <div className="workbench-split" id="workbench-split">
              <div className="wb-col" style={{ flex: 1 }}>
                <PdfViewer ref={pdfIframeRef} key={projectKey} projectPath={projectPath} onPageChange={handlePageChange} />
              </div>
              <div className="workspace-resize-h" />
              <div className="wb-col" style={{ flex: 1 }}>
                <BlockEditor block={activeBlock} pageBlocks={pageBlocks} onChange={handleContentChange} />
              </div>
              <div className="workspace-resize-h" />
              <div className="wb-col" style={{ flex: 1.5 }}>
                <div className="md-preview-panel">
                  <div className="md-preview-header">📝 Markdown 预览</div>
                  <div className="md-preview-body">
                    <MarkdownPreview blocks={pageBlocks} activeBlock={activeBlock} />
                  </div>
                </div>
              </div>
            </div>
          </div>

          <div className="workspace-resize-h" {...bindRight({ reversed: true })} />

          {/* 右侧：Pipeline + Console */}
          <div className="workspace-right" style={{ width: rightW }}>
            <div className="pr-section">
              <h3>⚙️ 流程管线</h3>
              <PipelineStatus blocksTotal={tocTree.reduce((s, n) => s + countNodes(n), 0)} />
            </div>
            <div className="pr-section pr-console">
              <h3>💬 智能对话</h3>
              <AgentConsole />
            </div>
          </div>

          {/* 连线信息层（绝对定位，覆盖 workspace 全区域） */}
          <LinesLayer lines={lines} />
        </Workspace>
      </div>

      <div className="resize-handle resize-v" style={{ gridArea: "hbot" }} {...bindBottom({ reversed: true })} />

      <footer className="panel-bottom">
        <LogPanel externalLogs={importLogs} />
      </footer>

      {importProgress && (
        <div className="import-overlay">
          <div className="import-progress-card">
            <div className="import-progress-stage">{importProgress.stage}</div>
            <div className="import-progress-bar-wrap">
              <div className="import-progress-bar" style={{ width: `${importProgress.percent}%` }} />
            </div>
            <div className="import-progress-detail">{importProgress.detail}</div>
            <div className="import-progress-pct">{importProgress.percent}%</div>
            {importLogs.length > 0 && (
              <div className="import-logs">
                {importLogs.map((log, i) => (
                  <div key={i} className="import-log-line">{log}</div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
