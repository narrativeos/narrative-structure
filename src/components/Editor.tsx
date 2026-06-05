import { useRef, useCallback } from "react";
import Editor, { OnMount } from "@monaco-editor/react";
import type { Block } from "../App";
import "./Editor.css";

interface EditorProps {
  block: Block | null;
  onChange: (blockId: string, content: string, version: number) => void;
}

const DEBOUNCE_MS = 800;

export default function BlockEditor({ block, onChange }: EditorProps) {
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const versionRef = useRef(0);

  const handleMount: OnMount = () => {};

  const currentContent = block?.content ?? "";
  const currentId = block?.id ?? "";

  const handleChange = useCallback(
    (value: string | undefined) => {
      if (!block || value === undefined) return;
      versionRef.current = block.version;

      if (debounceRef.current) clearTimeout(debounceRef.current);
      debounceRef.current = setTimeout(() => {
        onChange(currentId, value, versionRef.current);
      }, DEBOUNCE_MS);
    },
    [block, onChange, currentId],
  );

  if (!block) {
    return (
      <div className="editor-empty">
        <div className="editor-empty-icon">📝</div>
        <p>选择一个块开始编辑</p>
        <p className="editor-empty-hint">在左侧目录树中点击任意标题</p>
      </div>
    );
  }

  return (
    <div className="block-editor">
      <div className="editor-header">
        <span className="be-block-type">{block.block_type}</span>
        <span className="be-block-id">ID: {block.id.slice(0, 8)}…</span>
        <span className="be-block-version">v{block.version}</span>
      </div>
      <div className="editor-body">
        <Editor
          height="100%"
          defaultLanguage="markdown"
          theme="vs-dark"
          value={currentContent}
          onChange={handleChange}
          onMount={handleMount}
          options={{
            minimap: { enabled: true },
            lineNumbers: "on",
            wordWrap: "on",
            fontSize: 14,
            fontFamily: "'Cascadia Code', 'Fira Code', monospace",
            scrollBeyondLastLine: false,
            automaticLayout: true,
          }}
        />
      </div>
    </div>
  );
}

