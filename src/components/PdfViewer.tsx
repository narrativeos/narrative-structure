import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./PdfViewer.css";

interface PdfViewerProps {
  projectPath: string;
}

export default function PdfViewer({ projectPath }: PdfViewerProps) {
  const [dataUrl, setDataUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setLoading(true);
    setDataUrl(null);
    invoke<string | null>("find_asset_file", { pattern: "_layout.pdf" })
      .then(async (found) => {
        if (found) {
          const url = await invoke<string>("read_file_as_data_url", { path: found });
          setDataUrl(url);
        }
        setLoading(false);
      })
      .catch(() => setLoading(false));
  }, [projectPath]);

  if (loading) return <div className="pdf-empty">⏳ 加载 PDF...</div>;
  if (!dataUrl) return <div className="pdf-empty">（未找到 PDF）</div>;

  return (
    <div className="pdf-viewer">
      <div className="pdf-toolbar"><span className="pdf-title">📄 PDF 预览</span></div>
      <div className="pdf-content">
        <embed src={dataUrl} type="application/pdf" className="pdf-embed" />
      </div>
    </div>
  );
}
