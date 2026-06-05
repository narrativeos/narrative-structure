import { useEffect, useState } from "react";
import "./PdfViewer.css";

interface PdfViewerProps {
  projectPath: string;
  docName: string;
}

export default function PdfViewer({ projectPath, docName }: PdfViewerProps) {
  const [pdfUrl, setPdfUrl] = useState<string | null>(null);

  useEffect(() => {
    // 构建 PDF 路径: assets/<docName>/...origin.pdf
    // 目前通过 Tauri convertFileSrc 转换，简化版直接通过 assets 路径
    const searchName = docName + "_origin.pdf";
    setPdfUrl(`asset://localhost/${encodeURIComponent(projectPath + "/assets/" + docName + "/" + searchName)}`);
  }, [projectPath, docName]);

  if (!pdfUrl) {
    return <div className="pdf-empty">（无 PDF 预览）</div>;
  }

  return (
    <div className="pdf-viewer">
      <div className="pdf-toolbar">
        <span className="pdf-title">📄 PDF 预览</span>
      </div>
      <div className="pdf-content">
        <embed src={pdfUrl} type="application/pdf" className="pdf-embed" />
      </div>
    </div>
  );
}
