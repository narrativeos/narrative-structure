import "./PdfMirrorLayer.css";

export interface MirrorBbox {
  x: number;
  y: number;
  w: number;
  h: number;
  id: string; // block ID for line matching
}

interface Props {
  bboxes: MirrorBbox[];
  pageRect?: { left: number; top: number; width: number; height: number } | null;
}

export default function PdfMirrorLayer({ bboxes, pageRect }: Props) {
  if (!bboxes.length && !pageRect) return null;

  // 根据 pageRect 计算信息层容器的有效宽度，确保与 PDF 页面对齐
  // PDF 页面右侧有上下翻页按钮，需要留出相同宽度
  const containerStyle: React.CSSProperties = {};
  if (pageRect) {
    // 右边界 = 页面左偏移 + 页面宽度，以此限制容器宽度
    containerStyle.width = pageRect.left + pageRect.width;
    containerStyle.overflow = 'hidden';
  }

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
      {bboxes.map((bb) => (
        <div
          key={bb.id}
          className="mirror-bbox"
          data-mirror-id={bb.id}
          style={{
            left: bb.x,
            top: bb.y,
            width: bb.w,
            height: bb.h,
          }}
        />
      ))}
    </div>
  );
}
