import "./PipelineStatus.css";

interface PipelineStatusProps {
  blocksTotal: number;
}

const PIPELINE_STEPS = [
  { key: "import", label: "📥 导入解析", done: true },
  { key: "split", label: "🧩 语义分块", done: true },
  { key: "tree", label: "🌳 目录构建", done: true },
  { key: "review", label: "🔍 AI 复核", done: false },
  { key: "export", label: "📤 导出发布", done: false },
];

export default function PipelineStatus({ blocksTotal }: PipelineStatusProps) {
  return (
    <div className="ps-container">
      <div className="ps-stat">
        <span className="ps-stat-label">语义块总数</span>
        <span className="ps-stat-value">{blocksTotal}</span>
      </div>

      <div className="ps-steps">
        {PIPELINE_STEPS.map((step, i) => (
          <div key={step.key} className={`ps-step ${step.done ? "done" : ""}`}>
            <span className="ps-step-icon">{step.done ? "✅" : "⏳"}</span>
            <span className="ps-step-label">{step.label}</span>
            {i < PIPELINE_STEPS.length - 1 && <div className="ps-step-line" />}
          </div>
        ))}
      </div>
    </div>
  );
}
