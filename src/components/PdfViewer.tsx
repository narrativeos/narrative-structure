import { convertFileSrc } from "@tauri-apps/api/core";
import "./PdfViewer.css";

interface PdfViewerProps {
  projectPath: string;
  docName: string;
}

export default function PdfViewer({ projectPath, docName }: PdfViewerProps) {
  // 查找 _origin.pdf 文件
  const originPdf = `${projectPath}/assets/${docName}/${docName}_origin.pdf`;
  const pdfUrl = convertFileSrc(originPdf);

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
