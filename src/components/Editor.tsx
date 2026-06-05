import { useState, useEffect, useRef, useCallback } from "react";
import type { Block } from "../App";
import "./Editor.css";

interface EditorProps {
  block: Block | null;
  onChange: (blockId: string, content: string, version: number) => void;
}

const DEBOUNCE_MS = 800;

/** 编辑器组件：展示当前选中块的内容，支持实时编辑 + debounce 自动保存 */
export default function Editor({ block, onChange }: EditorProps) {
  const [localContent, setLocalContent] = useState("");
  const debounceTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const versionRef = useRef(0);

  // 切换块时同步状态
  useEffect(() => {
    if (block) {
      setLocalContent(block.content);
      versionRef.current = block.version;
    } else {
      setLocalContent("");
    }
  }, [block?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleChange = useCallback(
    (e: React.ChangeEvent<HTMLTextAreaElement>) => {
      const newContent = e.target.value;
      setLocalContent(newContent);

      if (!block) return;

      // Debounce 自动保存
      if (debounceTimer.current) clearTimeout(debounceTimer.current);
      debounceTimer.current = setTimeout(() => {
        onChange(block.id, newContent, versionRef.current);
      }, DEBOUNCE_MS);
    },
    [block, onChange],
  );

  // 清理定时器
  useEffect(() => {
    return () => {
      if (debounceTimer.current) clearTimeout(debounceTimer.current);
    };
  }, []);

  if (!block) {
    return (
      <div className="editor-empty">
        <div className="editor-empty-icon">📄</div>
        <p>选择一个块开始编辑</p>
        <p className="editor-empty-hint">
          在左侧目录树中点击任意节点，或使用搜索 (Ctrl+P) 查找内容
        </p>
      </div>
    );
  }

  return (
    <div className="editor-pane">
      <div className="editor-header">
        <span className="editor-block-type">{block.block_type}</span>
        <span className="editor-block-id">ID: {block.id.slice(0, 8)}...</span>
        <span className="editor-block-version">v{block.version}</span>
      </div>
      <textarea
        className="editor-textarea"
        value={localContent}
        onChange={handleChange}
        placeholder="输入内容..."
        spellCheck={false}
      />
    </div>
  );
}
