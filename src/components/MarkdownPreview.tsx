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
        <p>滚动 PDF 或选择目录以加载内容</p>
      </div>
    );
  }

  return (
    <div className="mdp-container">
      <div className="mdp-toolbar">
        <span className="mdp-title">
          {activeBlock ? "📄 单块" : blocks ? `📄 页面 (${blocks.length} 行)` : ""}
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
