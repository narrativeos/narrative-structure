import "./LinesLayer.css";

export interface LineDef {
  id: string;
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  color: string;
  active: boolean;
}

interface LinesLayerProps {
  lines: LineDef[];
}

export default function LinesLayer({ lines }: LinesLayerProps) {
  if (!lines.length) return null;

  return (
    <svg
      className="lines-layer"
      xmlns="http://www.w3.org/2000/svg"
    >
      <defs>
        <marker
          id="line-arrow"
          viewBox="0 0 6 6"
          refX="3"
          refY="3"
          markerWidth="5"
          markerHeight="5"
        >
          <circle cx="3" cy="3" r="2.5" fill="currentColor" />
        </marker>
      </defs>
      {lines.map((l) => (
        <line
          key={l.id}
          x1={l.x1}
          y1={l.y1}
          x2={l.x2}
          y2={l.y2}
          stroke={l.color}
          strokeWidth={l.active ? 2 : 0.6}
          strokeDasharray={l.active ? "none" : "4,3"}
          opacity={l.active ? 0.9 : 0.25}
          markerEnd={l.active ? "url(#line-arrow)" : undefined}
          className={l.active ? "line-active" : ""}
        />
      ))}
    </svg>
  );
}
