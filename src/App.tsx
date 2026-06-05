import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import TOC from "./components/TOC";
import BlockEditor from "./components/Editor";
import FileExplorer from "./components/FileExplorer";
import PdfViewer from "./components/PdfViewer";
import AgentConsole from "./components/AgentConsole";
import PipelineStatus from "./components/PipelineStatus";
import LogPanel from "./components/LogPanel";
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
  metadata: string;
  version: number;
  created_at: string;
  updated_at: string;
}

function countNodes(node: TocNode): number {
  return 1 + node.children.reduce((s, c) => s + countNodes(c), 0);
}

function App() {
  const [projectPath, setProjectPath] = useState<string | null>(null);
  const [projectName, setProjectName] = useState<string>("");
  const [tocTree, setTocTree] = useState<TocNode[]>([]);
  const [activeBlock, setActiveBlock] = useState<Block | null>(null);
  const [statusMsg, setStatusMsg] = useState("");
  // 可拖拽面板尺寸
  const [leftW, bindLeft] = useResizable(240, 160, 500);
  const [rightW, bindRight] = useResizable(220, 160, 400);
  const [splitPct, bindSplit] = useResizable(40, 20, 80);
  const [bottomH, bindBottom] = useResizable(140, 60, 400, "y");
  // =========================================================================
  // 加载项目（打开已有项目后调用）
  // =========================================================================
  const loadProject = useCallback(async (path: string) => {
    try {
      const msg = await invoke<string>("open_project", { path });
      setProjectPath(path);
      setProjectName(path.split("/").pop() || path);
      setStatusMsg(msg);

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

      const msg = await invoke<string>("import_new_project", {
        zipPath: zipPath,
      });

      const parts = msg.split(" | ");
      const name = parts[0] || "";
      const pathPart = parts[2] || "";

      setProjectName(name);
      setProjectPath(pathPart.trim());
      setStatusMsg(msg);

      const toc = await invoke<TocNode[]>("get_toc");
      setTocTree(toc);
    } catch (err) {
      setStatusMsg(`导入失败: ${err}`);
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
    try {
      await invoke<string>("close_project");
    } catch { /* ignore */ }
    setProjectPath(null);
    setTocTree([]);
    setActiveBlock(null);
    setStatusMsg("");
  }, []);

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
      const msg = await invoke<string>("import_document", {
        zipPath: selected as string,
      });
      setStatusMsg(msg);

      const toc = await invoke<TocNode[]>("get_toc");
      setTocTree(toc);
    } catch (err) {
      setStatusMsg(`导入失败: ${err}`);
    }
  }, []);

  // =========================================================================
  // 目录树 / 编辑器交互
  // =========================================================================
  const handleSelectBlock = useCallback(async (nodeId: string) => {
    try {
      const block = await invoke<Block>("get_block", { id: nodeId });
      setActiveBlock(block);
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
            <p className="welcome-recent-empty">（暂无历史记录）</p>
          </div>
        </div>
      </div>
    );
  }

  // =========================================================================
  // 主界面：三区布局
  // =========================================================================
  return (
    <div className="app-grid" style={{
      gridTemplateColumns: `${leftW}px 4px 1fr 4px ${rightW}px`,
      gridTemplateRows: `var(--toolbar-height) 1fr 4px ${bottomH}px`,
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

      <aside className="panel-left">
        <div className="panel-section toc-section">
          <h3>📑 语义目录 ({tocTree.reduce((s, n) => s + countNodes(n), 0)})</h3>
          <TOC nodes={tocTree} onSelect={handleSelectBlock} />
        </div>
        <div className="panel-section files-section">
          <FileExplorer projectPath={projectPath} />
        </div>
      </aside>

      <div className="resize-handle resize-h" {...bindLeft()} />

      <div className="panel-center">
        <div className="workbench-split">
          <div className="wb-left" style={{ width: `${splitPct}%` }}>
            <PdfViewer projectPath={projectPath} docName={projectName} />
          </div>
          <div className="resize-handle resize-h" {...bindSplit()} />
          <div className="wb-right" style={{ flex: 1 }}>
            <BlockEditor block={activeBlock} onChange={handleContentChange} />
          </div>
        </div>
      </div>

      <div className="resize-handle resize-h" {...bindRight(true)} />

      <aside className="panel-right">
        <div className="pr-section">
          <h3>⚙️ 流程管线</h3>
          <PipelineStatus blocksTotal={tocTree.reduce((s, n) => s + countNodes(n), 0)} />
        </div>
        <div className="pr-section pr-console">
          <h3>💬 智能对话</h3>
          <AgentConsole />
        </div>
      </aside>

      <div className="resize-handle resize-v" {...bindBottom()} />

      <footer className="panel-bottom">
        <LogPanel />
      </footer>
    </div>
  );
}

export default App;
