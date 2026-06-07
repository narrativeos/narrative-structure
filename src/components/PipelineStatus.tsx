import "./PipelineStatus.css";

interface PipelineStatusProps {
  blocksTotal: number;
  currentStage?: string;
}

interface PipelineNode {
  id: string;
  label: string;
  icon: string;
  status: "done" | "active" | "pending";
  desc: string;
}

const importStageOrder = [
  "解压 ZIP",
  "初始化数据库",
  "解析 Markdown",
  "加载信息层",
  "匹配页码",
  "写入数据库",
  "项目准备",
  "完成",
];

function mapStageToIndex(stage?: string): number {
  if (!stage) return -1;
  return importStageOrder.findIndex((item) => item === stage);
}

export default function PipelineStatus({ blocksTotal, currentStage }: PipelineStatusProps) {
  const baseNodes: PipelineNode[] = [
    { id: "extract", label: "解压 ZIP", icon: "📥", status: "pending", desc: "ZIP 内容提取" },
    { id: "database", label: "初始化数据库", icon: "🗄️", status: "pending", desc: "创建项目与数据库" },
    { id: "markdown", label: "Markdown 解析", icon: "📝", status: "pending", desc: "语义行级分块" },
    { id: "bbox", label: "加载信息层", icon: "🧩", status: "pending", desc: "展开 _middle.json bbox" },
    { id: "match", label: "匹配页码", icon: "📏", status: "pending", desc: "MD 行与 bbox 行匹配" },
    { id: "write", label: "写入数据库", icon: "💾", status: "pending", desc: "插入 blocks 表" },
    { id: "prepare", label: "项目准备", icon: "⚙️", status: "pending", desc: "打开并准备项目" },
    { id: "done", label: "完成", icon: "✅", status: "pending", desc: "项目导入完成" },
  ];
  const stageIndex = mapStageToIndex(currentStage);
  const nodes = baseNodes.map((node, idx) => {
    if (stageIndex === -1) {
      return node;
    }
    return {
      ...node,
      status: idx < stageIndex ? "done" : idx === stageIndex ? "active" : "pending",
    };
  });

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
