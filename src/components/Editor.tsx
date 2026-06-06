import { useState, useRef, useCallback, useMemo, useEffect } from "react";
import Editor, { OnMount } from "@monaco-editor/react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeRaw from "rehype-raw";
import { diffLines } from "diff";
import type { Block } from "../App";
import "./Editor.css";

type EditorMode = "edit" | "preview" | "diff";

interface EditorProps {
  block: Block | null;
  onChange: (blockId: string, content: string, version: number) => void;
}

const DEBOUNCE_MS = 800;

export default function BlockEditor({ block, onChange }: EditorProps) {
  const [mode, setMode] = useState<EditorMode>("edit");
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const versionRef = useRef(0);
  // 当前编辑中的内容（ref，不触发重渲染）
  const editRef = useRef<string>("");

  // block 切换时重置编辑缓存
  useEffect(() => {
    if (block) {
      editRef.current = block.content;
    } else {
      editRef.current = "";
    }
  }, [block?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleMount: OnMount = () => {};

  const handleChange = useCallback(
    (value: string | undefined) => {
      if (!block || value === undefined) return;
      versionRef.current = block.version;
      editRef.current = value;

      if (debounceRef.current) clearTimeout(debounceRef.current);
      debounceRef.current = setTimeout(() => {
        onChange(block.id, value, versionRef.current);
      }, DEBOUNCE_MS);
    },
    [block, onChange],
  );

  // ---- Diff 计算：原始(DB) vs 当前编辑 ----
  const diffResult = useMemo(() => {
    if (mode !== "diff" || !block) return [];
    return diffLines(block.original_content, editRef.current);
  }, [mode, block, editRef.current]);

  // ---- 模式切换标签 ----
  const modeTabs: { id: EditorMode; label: string; icon: string }[] = [
    { id: "edit", label: "编辑", icon: "✏️" },
    { id: "preview", label: "预览", icon: "👁️" },
    { id: "diff", label: "对比", icon: "🔍" },
  ];

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
      {/* ---- 顶部信息栏 ---- */}
      <div className="editor-header">
        <span className="be-block-type">{block.block_type}</span>
        <span className="be-block-id">ID: {block.id.slice(0, 8)}…</span>
        <span className="be-block-page">
          {(() => {
            try {
              const meta = JSON.parse(block.metadata || "{}");
              return meta.page ? `📄 p${meta.page}` : "";
            } catch { return ""; }
          })()}
        </span>
        <span className="be-block-version">v{block.version}</span>
      </div>

      {/* ---- 模式切换栏 ---- */}
      <div className="editor-mode-bar">
        {modeTabs.map((t) => (
          <button
            key={t.id}
            className={`mode-tab${mode === t.id ? " active" : ""}`}
            onClick={() => setMode(t.id)}
            title={t.label}
          >
            {t.icon} {t.label}
          </button>
        ))}
      </div>

      {/* ---- 编辑模式 ---- */}
      {mode === "edit" && (
        <div className="editor-body">
          <Editor
            key={block.id}
            height="100%"
            defaultLanguage="markdown"
            theme="vs-dark"
            defaultValue={block.content}
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
      )}

      {/* ---- 预览模式 ---- */}
      {mode === "preview" && (
        <div className="editor-preview markdown-body">
          <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeRaw]}>
            {editRef.current || block.content}
          </ReactMarkdown>
        </div>
      )}

      {/* ---- Diff 模式 ---- */}
      {mode === "diff" && (
        <div className="editor-diff">
          <div className="diff-header">
            <span className="diff-label diff-label-orig">📄 原始版本</span>
            <span className="diff-label diff-label-curr">✏️ 当前版本</span>
          </div>
          <div className="diff-body">
            <pre className="diff-content">
              <code>
                {diffResult.map((part, i) => (
                  <span
                    key={i}
                    className={
                      part.added
                        ? "diff-added"
                        : part.removed
                        ? "diff-removed"
                        : ""
                    }
                  >
                    {part.value}
                  </span>
                ))}
              </code>
            </pre>
          </div>
        </div>
      )}
    </div>
  );
}


