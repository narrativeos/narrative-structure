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
  const [statusMsg, setStatusMsg] = useState("请打开一个项目以开始");

  // 通过 Tauri invoke 打开项目
  const handleOpenProject = useCallback(async () => {
    try {
      // 使用 Tauri dialog (简化版：直接使用已知路径)
      const { invoke } = await import("@tauri-apps/api/core");
      const demoPath = "Projects/MyDocument_A";
      const msg = await invoke<string>("open_project", { path: demoPath });
      setProjectPath(demoPath);
      setStatusMsg(msg);

      // 加载目录树
      const toc = await invoke<TocNode[]>("get_toc");
      setTocTree(toc);
    } catch (err) {
      setStatusMsg(`错误: ${err}`);
    }
  }, []);

  // 点击目录项 → 加载该块及子块
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

  // 编辑器内容变更 → DEBOUNCE 后更新数据库
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

  return (
    <div className="app-container">
      {/* 顶部工具栏 */}
      <header className="toolbar">
        <h1 className="app-title">NarrativeStructure</h1>
        <div className="toolbar-actions">
          <button onClick={handleOpenProject} disabled={!!projectPath}>
            {projectPath ? `📂 ${projectPath}` : "📂 打开项目"}
          </button>
          <span className="status-msg">{statusMsg}</span>
        </div>
      </header>

      {/* 主体区域 */}
      <div className="main-area">
        {/* 左侧目录树 */}
        <aside className="sidebar">
          <h3>📑 文档目录</h3>
          <TOC nodes={tocTree} onSelect={handleSelectBlock} />
        </aside>

        {/* 右侧编辑器 */}
        <main className="editor-area">
          <Editor block={activeBlock} onChange={handleContentChange} />
        </main>
      </div>
    </div>
  );
}

export default App;
