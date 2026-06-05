import "./PipelineStatus.css";

interface PipelineStatusProps {
  blocksTotal: number;
}

interface PipelineNode {
  id: string;
  label: string;
  icon: string;
  status: "done" | "active" | "pending";
  desc: string;
}

export default function PipelineStatus({ blocksTotal }: PipelineStatusProps) {
  const nodes: PipelineNode[] = [
    { id: "import", label: "导入解析", icon: "📥", status: "done", desc: "ZIP 解压 + 资源提取" },
    { id: "parse", label: "MD 解析", icon: "📝", status: "done", desc: "Markdown → SemanticBlock" },
    { id: "tree", label: "目录构建", icon: "🌳", status: "done", desc: "层级关系 + 全文索引" },
    { id: "review", label: "AI 复核", icon: "🔍", status: "pending", desc: "质量校验 + 纠错" },
    { id: "enrich", label: "语义增强", icon: "🧠", status: "pending", desc: "实体提取 + 摘要生成" },
    { id: "export", label: "导出发布", icon: "📤", status: "pending", desc: "Markdown / PDF 输出" },
  ];

  return (
    <div className="ps-container">
      <div className="ps-stat">
        <span className="ps-stat-label">语义块</span>
        <span className="ps-stat-value">{blocksTotal}</span>
      </div>
      <div className="ps-stat">
        <span className="ps-stat-label">完成节点</span>
        <span className="ps-stat-value dim">{nodes.filter(n => n.status === "done").length}/{nodes.length}</span>
      </div>

      <div className="ps-nodes">
        {nodes.map((node) => (
          <div key={node.id} className={`ps-node ${node.status}`}>
            <span className="ps-node-icon">{node.icon}</span>
            <div className="ps-node-info">
              <span className="ps-node-label">{node.label}</span>
              <span className="ps-node-desc">{node.desc}</span>
            </div>
            <span className="ps-node-badge">
              {node.status === "done" ? "✓" : node.status === "active" ? "⚡" : "○"}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
