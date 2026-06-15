import { useCallback } from "react";
import "./PdfMirrorLayer.css";

export interface MirrorBbox {
  x: number;
  y: number;
  w: number;
  h: number;
  id: string; // block ID for line matching
  block_type?: string; // block type for color coding
}

interface Props {
  bboxes: MirrorBbox[];
  pageRect?: { left: number; top: number; width: number; height: number } | null;
  visible?: boolean;
  selectedId?: string | null;
  hoveredId?: string | null;
  onBboxClick?: (id: string) => void;
}

// 块类型颜色映射 — 与 App.tsx 中的飞线颜色一致
const TYPE_COLORS: Record<string, { border: string, bg: string }> = {
  heading: { border: 'oklch(0.70 0.18 25 / 0.6)', bg: 'oklch(0.70 0.18 25 / 0.12)' },
  text: { border: 'oklch(0.60 0.15 230 / 0.5)', bg: 'oklch(0.60 0.15 230 / 0.1)' },
  interline_equation: { border: 'oklch(0.60 0.15 160 / 0.5)', bg: 'oklch(0.60 0.15 160 / 0.1)' },
  table: { border: 'oklch(0.70 0.16 80 / 0.5)', bg: 'oklch(0.70 0.16 80 / 0.1)' },
  image: { border: 'oklch(0.65 0.18 300 / 0.5)', bg: 'oklch(0.65 0.18 300 / 0.1)' },
  empty: { border: 'oklch(0.55 0 0 / 0.3)', bg: 'oklch(0.55 0 0 / 0.05)' },
};

const DEFAULT_COLOR = { border: 'oklch(0.75 0.16 90 / 0.35)', bg: 'oklch(0.75 0.16 90 / 0.06)' };

export default function PdfMirrorLayer({ bboxes, pageRect, visible = true, selectedId, hoveredId, onBboxClick }: Props) {
  if (!bboxes.length && !pageRect) return null;

  // 根据 pageRect 计算信息层容器的有效宽度，确保与 PDF 页面对齐
  // PDF 页面右侧有上下翻页按钮，需要留出相同宽度
  // 关键：容器本身 pointer-events: none，让事件穿透到 iframe（翻页按钮等）
  // 只有 .mirror-bbox 子元素设置 pointer-events: auto 来独立接收点击
  const containerStyle: React.CSSProperties = {
    opacity: visible ? 1 : 0,
    transition: 'opacity 0.25s ease',
    pointerEvents: 'none' as const,
  };
  if (pageRect) {
    // 右边界 = 页面左偏移 + 页面宽度，以此限制容器宽度
    containerStyle.width = pageRect.left + pageRect.width;
    containerStyle.overflow = 'hidden';
  }

  const handleClick = useCallback((id: string) => {
    onBboxClick?.(id);
  }, [onBboxClick]);

  return (
    <div className="pdf-mirror-layer" style={containerStyle}>
      {pageRect && (
        <div
          className="mirror-page-frame"
          style={{
            left: pageRect.left,
            top: pageRect.top,
            width: pageRect.width,
            height: pageRect.height,
          }}
        />
      )}
      {bboxes.map((bb) => {
        const colors = TYPE_COLORS[bb.block_type || ''] || DEFAULT_COLOR;
        const isSelected = bb.id === selectedId;
        const isHovered = bb.id === hoveredId;
        const borderColor = isSelected
          ? colors.border.replace(/[\d.]+\)$/, '0.9)')
          : isHovered
            ? colors.border.replace(/[\d.]+\)$/, '0.7)')
            : colors.border;
        return (
          <div
            key={bb.id}
            className={`mirror-bbox${bb.block_type ? ` mirror-bbox-${bb.block_type}` : ''}${isSelected ? ' mirror-bbox-selected' : ''}${isHovered ? ' mirror-bbox-hovered' : ''}`}
            data-mirror-id={bb.id}
            data-block-type={bb.block_type || ''}
            style={{
              left: bb.x,
              top: bb.y,
              width: bb.w,
              height: bb.h,
              border: `2px solid ${borderColor}`,
              background: colors.bg,
              cursor: onBboxClick ? 'pointer' : 'default',
              boxShadow: isSelected ? `0 0 0 3px ${colors.border.replace(/[\d.]+\)$/, '0.3)' )}` : 'none',
            }}
            onClick={() => handleClick(bb.id)}
          />
        );
      })}
    </div>
  );
}
