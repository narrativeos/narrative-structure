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
        <marker
          id="line-start"
          viewBox="0 0 6 6"
          refX="3"
          refY="3"
          markerWidth="5"
          markerHeight="5"
        >
          <circle cx="3" cy="3" r="2.5" fill="currentColor" />
        </marker>
      </defs>
      {lines.map((l) => {
        const dx = l.x2 - l.x1;
        const cp = dx * 0.5; // 控制点偏移
        const d = `M ${l.x1} ${l.y1} C ${l.x1 + cp} ${l.y1}, ${l.x2 - cp} ${l.y2}, ${l.x2} ${l.y2}`;
        return (
          <path
            key={l.id}
            d={d}
            fill="none"
            stroke={l.color}
            strokeWidth={l.active ? 2.5 : 0.6}
            strokeDasharray={l.active ? "none" : "4,3"}
            opacity={l.active ? 0.95 : 0.25}
            markerStart={l.active ? "url(#line-start)" : undefined}
            markerEnd={l.active ? "url(#line-arrow)" : undefined}
            className={l.active ? "line-active" : ""}
          />
        );
      })}
    </svg>
  );
}
