import { useState, memo } from "react";
import type { TocNode } from "../App";
import "./TOC.css";

/** 去掉 Markdown 标题标记 `#` 和首尾空格 */
function cleanTitle(text: string): string {
  return text.replace(/^#{1,6}\s*/, "").trim();
}

interface TOCProps {
  nodes: TocNode[];
  onSelect: (nodeId: string) => void;
}

/** 单个 TOC 节点，支持展开/折叠 */
const TOCItem = memo(function TOCItem({ node, onSelect }: { node: TocNode; onSelect: (id: string) => void }) {
  const [expanded, setExpanded] = useState(node.level <= 1);
  const hasChildren = node.children && node.children.length > 0;

  const indent = node.level * 16;

  return (
    <div className="toc-item">
      <div
        className="toc-item-row"
        style={{ paddingLeft: `${indent + 8}px` }}
        onClick={() => {
          if (hasChildren) setExpanded(!expanded);
          onSelect(node.id);
        }}
      >
        {hasChildren ? (
          <span className={`toc-toggle ${expanded ? "expanded" : ""}`}>▸</span>
        ) : (
          <span className="toc-toggle placeholder" />
        )}
        <span className="toc-label" title={node.content_preview}>
          {cleanTitle(node.content_preview) || "(无标题)"}
        </span>
      </div>
      {hasChildren && expanded && (
        <div className="toc-children">
          {node.children.map((child) => (
            <TOCItem key={child.id} node={child} onSelect={onSelect} />
          ))}
        </div>
      )}
    </div>
  );
});

/** 目录树组件：递归渲染 blocks 逻辑树 */
export default function TOC({ nodes, onSelect }: TOCProps) {
  if (nodes.length === 0) {
    return <div className="toc-empty">（空项目）</div>;
  }

  return (
    <div className="toc-tree">
      {nodes.map((node) => (
        <TOCItem key={node.id} node={node} onSelect={onSelect} />
      ))}
    </div>
  );
}
