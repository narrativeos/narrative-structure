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

  return (
    <div className="pdf-mirror-layer">
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
