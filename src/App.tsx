import { useState, useCallback } from "react";
import TOC from "./components/TOC";
import Editor from "./components/Editor";
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

function App() {
  const [projectPath, setProjectPath] = useState<string | null>(null);
  const [tocTree, setTocTree] = useState<TocNode[]>([]);
  const [activeBlock, setActiveBlock] = useState<Block | null>(null);
  const [statusMsg, setStatusMsg] = useState("");

  // =========================================================================
  // 加载项目（打开或新建后调用）
  // =========================================================================
  const loadProject = useCallback(async (path: string) => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");

      const msg = await invoke<string>("open_project", { path });
      setProjectPath(path);
      setStatusMsg(msg);

      const toc = await invoke<TocNode[]>("get_toc");
      setTocTree(toc);
    } catch (err) {
      setStatusMsg(`错误: ${err}`);
    }
  }, []);

  // =========================================================================
  // 新建项目
  // =========================================================================
  const handleNewProject = useCallback(async () => {
    const name = window.prompt("输入项目名称:");
    if (!name?.trim()) return;

    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const projectsDir = "Projects"; // 默认创建在 Projects/ 下
      const msg = await invoke<string>("create_project", {
        parentDir: projectsDir,
        projectName: name.trim(),
      });
      // create_project 内部已自动打开，只需同步前端状态
      const fullPath = `${projectsDir}/${name.trim()}`;
      setProjectPath(fullPath);
      setStatusMsg(msg);

      const toc = await invoke<TocNode[]>("get_toc");
      setTocTree(toc);
    } catch (err) {
      setStatusMsg(`创建失败: ${err}`);
    }
  }, []);

  // =========================================================================
  // 打开已有项目（原生文件夹选择）
  // =========================================================================
  const handleOpenProject = useCallback(async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
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
      const { invoke } = await import("@tauri-apps/api/core");
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
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        filters: [{ name: "ZIP 压缩包", extensions: ["zip"] }],
        multiple: false,
        title: "选择 MinerU 输出 zip 包",
      });

      if (!selected) return;

      setStatusMsg("正在导入...");
      const { invoke } = await import("@tauri-apps/api/core");
      const msg = await invoke<string>("import_document", {
        zipPath: selected as string,
      });
      setStatusMsg(msg);

      // 刷新目录树
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
      const { invoke } = await import("@tauri-apps/api/core");
      const blocks = await invoke<Block[]>("get_blocks", {
        parentId: nodeId,
        limit: 50,
        offset: 0,
      });
      if (blocks.length > 0) {
        setActiveBlock(blocks[0]);
      }
    } catch (err) {
      setStatusMsg(`加载块失败: ${err}`);
    }
  }, []);

  const handleContentChange = useCallback(
    async (blockId: string, newContent: string, version: number) => {
      try {
        const { invoke } = await import("@tauri-apps/api/core");
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
            以语义块为最小单元，将超大型文档离散化为可操作的知识资产
          </p>

          <div className="welcome-actions">
            <button className="btn btn-primary" onClick={handleNewProject}>
              ＋ 新建项目
            </button>
            <button className="btn btn-secondary" onClick={handleOpenProject}>
              📂 打开项目
            </button>
          </div>

          <div className="welcome-recent">
            <h4>最近打开</h4>
            <p className="welcome-recent-empty">（暂无历史记录）</p>
          </div>
        </div>
      </div>
    );
  }

  // =========================================================================
  // 工作台 (项目已打开)
  // =========================================================================
  return (
    <div className="app-container">
      <header className="toolbar">
        <h1 className="app-title">NarrativeStructure</h1>
        <div className="toolbar-project">
          <span className="project-path" title={projectPath}>
            📁 {projectPath}
          </span>
          <button className="btn-close" onClick={handleCloseProject} title="关闭项目">
            ✕
          </button>
        </div>
        <div className="toolbar-actions">
          <button className="btn-import" onClick={handleImportDocument} title="导入 MinerU 输出 zip">
            📥 导入文档
          </button>
          <span className="status-msg">{statusMsg}</span>
        </div>
      </header>

      <div className="main-area">
        <aside className="sidebar">
          <h3>📑 文档目录</h3>
          <TOC nodes={tocTree} onSelect={handleSelectBlock} />
        </aside>

        <main className="editor-area">
          <Editor block={activeBlock} onChange={handleContentChange} />
        </main>
      </div>
    </div>
  );
}

export default App;
