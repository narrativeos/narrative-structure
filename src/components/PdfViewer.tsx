import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./PdfViewer.css";

interface PdfViewerProps {
  projectPath: string;
}

export default function PdfViewer({ projectPath }: PdfViewerProps) {
  const [pdfUrl, setPdfUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setLoading(true);
    setPdfUrl(null);
    invoke<string | null>("find_asset_file", { pattern: "_layout.pdf" })
      .then((found) => {
        if (found) {
          const url = `narrativestructure://localhost/${encodeURIComponent(found)}?t=${Date.now()}#view=FitH`;
          console.log("PDF URL:", url);
          setPdfUrl(url);
        }
        setLoading(false);
      })
      .catch(() => setLoading(false));
  }, [projectPath]);

  if (loading) return <div className="pdf-empty">⏳ 加载 PDF...</div>;
  if (!pdfUrl) return <div className="pdf-empty">（未找到 PDF）</div>;

  return (
    <div className="pdf-viewer">
      <button
        className="pdf-fit-btn"
        title="恢复全宽"
        onClick={() => {
          // force reload with FitH
          const base = pdfUrl!.split("#")[0].split("?")[0];
          setPdfUrl(`${base}?t=${Date.now()}#view=FitH`);
        }}
      >
        ↔
      </button>
      <div className="pdf-content">
        <iframe src={pdfUrl} className="pdf-embed" title="PDF Preview" />
      </div>
    </div>
  );
}
