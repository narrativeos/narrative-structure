import "./PipelineStatus.css";

interface PipelineStatusProps {
  blocksTotal: number;
  currentStage?: string;
}

interface PipelineNode {
  id: string;
  label: string;
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

const baseNodes: PipelineNode[] = [
  { id: "extract", label: "解压 ZIP", status: "pending" as const, desc: "ZIP 内容提取" },
  { id: "database", label: "初始化数据库", status: "pending" as const, desc: "创建项目与数据库" },
  { id: "markdown", label: "Markdown 解析", status: "pending" as const, desc: "语义行级分块" },
  { id: "bbox", label: "加载信息层", status: "pending" as const, desc: "展开 _middle.json bbox" },
  { id: "match", label: "匹配页码", status: "pending" as const, desc: "MD 行与 bbox 行匹配" },
  { id: "write", label: "写入数据库", status: "pending" as const, desc: "插入 blocks 表" },
  { id: "prepare", label: "项目准备", status: "pending" as const, desc: "打开并准备项目" },
  { id: "done", label: "完成", status: "pending" as const, desc: "项目导入完成" },
];

export default function PipelineStatus({ blocksTotal, currentStage }: PipelineStatusProps) {
  const stageIndex = mapStageToIndex(currentStage);
  const nodes: PipelineNode[] = baseNodes.map((node, idx) => ({
    ...node,
    status: stageIndex === -1 ? "pending" as const : idx < stageIndex ? "done" as const : idx === stageIndex ? "active" as const : "pending" as const,
  }));
  const doneCount = nodes.filter(n => n.status === "done").length;
  const hasProgress = stageIndex >= 0;

  return (
    <div className="ps-container">
      <div className="ps-stats-row">
        <div className="ps-stat-card">
          <span className="ps-stat-card-value">{blocksTotal.toLocaleString()}</span>
          <span className="ps-stat-card-label">语义块</span>
        </div>
        <div className="ps-stat-card accent">
          <span className="ps-stat-card-value">{hasProgress ? doneCount : "—"}</span>
          <span className="ps-stat-card-label">已完成</span>
        </div>
      </div>

      <div className="ps-divider" />

      <div className="ps-timeline">
        {nodes.map((node, idx) => (
          <div key={node.id} className={`ps-tl-item ${node.status}`}>
            <div className="ps-tl-track">
              <div className="ps-tl-dot">
                {node.status === "active" && <span className="ps-tl-pulse" />}
              </div>
              {idx < nodes.length - 1 && <div className="ps-tl-line" />}
            </div>
            <div className="ps-tl-content">
              <span className="ps-tl-label">{node.label}</span>
              <span className="ps-tl-desc">{node.desc}</span>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
