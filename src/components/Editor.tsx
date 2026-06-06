import { useState, useRef, useCallback, useMemo, useEffect } from "react";
import Editor, { OnMount } from "@monaco-editor/react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeRaw from "rehype-raw";
import { diffLines } from "diff";
import type { Block } from "../App";
import "./Editor.css";

type LeftMode = "edit" | "diff";

interface EditorProps {
  block: Block | null;
  onChange: (blockId: string, content: string, version: number) => void;
}

const DEBOUNCE_MS = 800;

export default function BlockEditor({ block, onChange }: EditorProps) {
  const [leftMode, setLeftMode] = useState<LeftMode>("edit");
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const versionRef = useRef(0);
  const editRef = useRef<string>("");

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

  const diffResult = useMemo(() => {
    if (leftMode !== "diff" || !block) return [];
    return diffLines(block.original_content, editRef.current);
  }, [leftMode, block, editRef.current]);

  if (!block) {
    return (
      <div className="editor-empty">
        <div className="editor-empty-icon">📝</div>
        <p>选择一个块开始编辑</p>
        <p className="editor-empty-hint">在左侧目录树中点击任意标题</p>
      </div>
    );
  }

  const pageMeta = (() => {
    try {
      const meta = JSON.parse(block.metadata || "{}");
      return meta.page ? `📄 p${meta.page}` : "";
    } catch { return ""; }
  })();

  return (
    <div className="block-editor">
      {/* 顶部信息栏（模式切换合并到同一行） */}
      <div className="editor-header">
        <span className="be-block-type">{block.block_type}</span>
        <span className="be-block-page">{pageMeta}</span>
        <button
          className={`mode-tab${leftMode === "edit" ? " active" : ""}`}
          onClick={() => setLeftMode("edit")}
        >✏️</button>
        <button
          className={`mode-tab${leftMode === "diff" ? " active" : ""}`}
          onClick={() => setLeftMode("diff")}
        >🔍</button>
        <span className="be-block-version">v{block.version}</span>
        <span className="be-block-id">ID: {block.id.slice(0, 8)}…</span>
      </div>

      {/* 双栏：左编辑/Diff | 右预览 */}
      <div className="editor-split">
        <div className="editor-split-left">
          {leftMode === "edit" && (
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
                  minimap: { enabled: false },
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
          {leftMode === "diff" && (
            <div className="editor-diff">
              <div className="diff-header">
                <span className="diff-label diff-label-orig">📄 原始</span>
                <span className="diff-label diff-label-curr">✏️ 当前</span>
              </div>
              <div className="diff-body">
                <pre className="diff-content">
                  <code>
                    {diffResult.map((part, i) => (
                      <span key={i} className={
                        part.added ? "diff-added" : part.removed ? "diff-removed" : ""
                      }>{part.value}</span>
                    ))}
                  </code>
                </pre>
              </div>
            </div>
          )}
        </div>

        <div className="editor-split-right">
          <div className="editor-preview markdown-body">
            <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeRaw]}>
              {editRef.current || block.content}
            </ReactMarkdown>
          </div>
        </div>
      </div>
    </div>
  );
}


