import { useMemo, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeRaw from "rehype-raw";
import type { Block } from "../App";
import "./MarkdownPreview.css";

interface Props {
  blocks: Block[] | null;
  activeBlock: Block | null;
  projectPath?: string | null;
  projectName?: string;
}

export default function MarkdownPreview({ blocks, activeBlock, projectPath, projectName }: Props) {
  const [viewMode, setViewMode] = useState<"preview" | "source">("preview");

  const mdText = useMemo(() => {
    if (activeBlock) return activeBlock.content;
    if (blocks && blocks.length > 0) {
      return blocks.map((b) => b.content).join("\n");
    }
    return "";
  }, [blocks, activeBlock]);

  const assetBase = useMemo(() => {
    if (!projectPath || !projectName) return "";
    return `narrativestructure://localhost/${encodeURIComponent(projectPath)}/assets/${encodeURIComponent(projectName)}/`;
  }, [projectPath, projectName]);

  if (!mdText) {
    return (
      <div className="mdp-empty">
        <div className="mdp-empty-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
            <polyline points="14 2 14 8 20 8"/>
            <line x1="16" y1="13" x2="8" y2="13"/>
            <line x1="16" y1="17" x2="8" y2="17"/>
          </svg>
        </div>
        <p>滚动 PDF 或选择目录以加载内容</p>
      </div>
    );
  }

  return (
    <div className="mdp-container">
      <div className="mdp-toolbar">
        <span className="mdp-title">
          {activeBlock ? "单块" : blocks ? `页面 (${blocks.length} 行)` : ""}
        </span>
        <div className="mdp-modes">
          <button
            className={`mdp-mode-btn${viewMode === "preview" ? " active" : ""}`}
            onClick={() => setViewMode("preview")}
          >👁 预览</button>
          <button
            className={`mdp-mode-btn${viewMode === "source" ? " active" : ""}`}
            onClick={() => setViewMode("source")}
          >&lt;/&gt; 源码</button>
        </div>
      </div>
      <div className="mdp-body">
        {viewMode === "preview" ? (
          <div className="markdown-body">
            <ReactMarkdown
              remarkPlugins={[remarkGfm]}
              rehypePlugins={[rehypeRaw]}
              components={{
                img: ({ src, alt, ...props }) => {
                  let resolved = src || "";
                  if (resolved && !resolved.startsWith("http") && !resolved.startsWith("data:") && !resolved.startsWith("narrativestructure:")) {
                    // 相对路径 → narrativestructure:// 协议
                    const clean = resolved.replace(/^\.?\/?/, "");
                    resolved = assetBase + clean + "?raw=1";
                  }
                  return <img src={resolved} alt={alt || ""} {...props} />;
                },
              }}
            >
              {mdText}
            </ReactMarkdown>
          </div>
        ) : (
          <pre className="mdp-source">{mdText}</pre>
        )}
      </div>
    </div>
  );
}
