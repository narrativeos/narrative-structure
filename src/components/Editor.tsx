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
  pageBlocks: Block[] | null;
  onChange: (blockId: string, content: string, version: number) => void;
  onHoverBlock?: (block: Block | null) => void;
  onBlockToggle?: () => void;
  currentPage?: number;
}

const DEBOUNCE_MS = 800;

export default function BlockEditor({ block, pageBlocks, onChange, onHoverBlock, onBlockToggle, currentPage }: EditorProps) {
  const [leftMode, setLeftMode] = useState<LeftMode>("edit");
  const [expandedBlocks, setExpandedBlocks] = useState<Set<string>>(new Set());
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

  if (!block && !pageBlocks?.length) {
    return (
      <div className="editor-empty">
        <div className="editor-empty-icon">📝</div>
        <p>选择一个块开始编辑</p>
        <p className="editor-empty-hint">在左侧目录树中点击任意标题，或滚动 PDF</p>
      </div>
    );
  }

  // 页面模式：按页码分组显示
  if (pageBlocks && pageBlocks.length > 0 && !block) {
    // 提取页码并排序
    const withPage = pageBlocks.map(b => {
      let p = 0;
      try { p = JSON.parse(b.metadata || "{}").page || 0; } catch {}
      return { block: b, page: p };
    }).filter(wp => wp.page > 0).sort((a, b) => a.page - b.page || a.block.order_idx - b.block.order_idx);
    // 按 page 分组
    const groups: { page: number; blocks: Block[]; empty: boolean }[] = [];
    for (const { block, page } of withPage) {
      const last = groups[groups.length - 1];
      if (last && last.page === page) {
        last.blocks.push(block);
      } else {
        groups.push({ page, blocks: [block], empty: false });
      }
    }
    for (const group of groups) {
      if (group.blocks.every((b) => b.block_type === 'empty')) {
        group.empty = true;
      }
    }

    return (
      <div className="block-editor page-mode">
        <div className="editor-header">
          <span className="be-block-type">📄 页面内容</span>
          <span className="editor-header-pages">
            {groups.map((g) => (
              <button key={g.page} className={`page-btn${currentPage === g.page ? ' active' : ''}${g.empty ? ' page-btn-empty' : ''}`}
                onClick={() => {
                  const el = document.querySelector(`.page-group[data-page="${g.page}"]`);
                  el?.scrollIntoView({ block: 'start', behavior: 'smooth' });
                }}
              >{g.page}</button>
            ))}
          </span>
          <span className="editor-header-count">{pageBlocks.length} 行</span>
        </div>
        <div className="page-blocks-list">
          {groups.map((g, gi) => (
            <div key={gi} className={`page-group${g.empty ? ' page-group-empty' : ''}${currentPage === g.page ? ' page-group-active' : ''}`} data-page={g.page}>
              <div className="page-group-header">— p{g.page} —</div>
              {g.blocks.map((b) => {
                const isLong = b.content && b.content.length > 60;
                const expanded = expandedBlocks.has(b.id);
                return (
                <div key={b.id} className={`page-block-row ${b.block_type}${isLong && !expanded ? ' clamped' : ''}${expanded ? ' expanded' : ''}`} data-block-id={b.id}
                  onMouseEnter={() => onHoverBlock?.(b)}
                  onMouseLeave={() => onHoverBlock?.(null)}
                  onClick={() => { if (isLong) { setExpandedBlocks(prev => { const next = new Set(prev); if (next.has(b.id)) next.delete(b.id); else next.add(b.id); return next; }); setTimeout(() => onBlockToggle?.(), 0); } }}>
                  <span className="pbr-type">{b.block_type === "heading" ? `H${b.level}` : b.block_type === "empty" ? "" : "·"}</span>
                  <span className="pbr-content">{b.content || (b.block_type === "empty" ? "\u00A0" : "")}</span>
                  {isLong && <span className="pbr-toggle">{expanded ? '▲' : '▼'}</span>}
                </div>
                );
              })}
            </div>
          ))}
        </div>
      </div>
    );
  }

  // 单块模式（此时 block 不为 null，已在前面两个 early return 后）
  if (!block) return null;
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


