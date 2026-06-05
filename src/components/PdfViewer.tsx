import { useEffect, useState } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import "./PdfViewer.css";

interface PdfViewerProps {
  projectPath: string;
  docName: string;
}

export default function PdfViewer({ projectPath }: PdfViewerProps) {
  const [pdfPath, setPdfPath] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setLoading(true);
    invoke<string | null>("find_asset_file", { pattern: "_origin.pdf" })
      .then((found) => {
        if (found) setPdfPath(convertFileSrc(found));
        setLoading(false);
      })
      .catch(() => setLoading(false));
  }, [projectPath]);

  if (loading) {
    return <div className="pdf-empty">⏳ 正在加载 PDF...</div>;
  }

  if (!pdfPath) {
    return <div className="pdf-empty">（未找到 PDF 文件）</div>;
  }

  return (
    <div className="pdf-viewer">
      <div className="pdf-toolbar">
        <span className="pdf-title">📄 PDF 预览</span>
      </div>
      <div className="pdf-content">
        <embed src={pdfPath} type="application/pdf" className="pdf-embed" />
      </div>
    </div>
  );
}
