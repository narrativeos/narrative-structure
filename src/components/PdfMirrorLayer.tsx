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
  scrollY: number;
}

export default function PdfMirrorLayer({ bboxes, scrollY }: Props) {
  if (!bboxes.length) return null;

  return (
    <div className="pdf-mirror-layer" style={{ transform: `translateY(${-scrollY}px)` }}>
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
