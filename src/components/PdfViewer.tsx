import { useEffect, useState, useRef, forwardRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import PdfMirrorLayer, { MirrorBbox } from "./PdfMirrorLayer";
import "./PdfViewer.css";

interface PdfViewerProps {
  projectPath: string;
  onPageChange?: (page: number) => void;
  mirrorBboxes?: MirrorBbox[];
  pageRect?: { left: number; top: number; width: number; height: number } | null;
}

const PdfViewer = forwardRef<HTMLIFrameElement, PdfViewerProps>(
  function PdfViewer({ projectPath, onPageChange, mirrorBboxes, pageRect }, ref) {
  const [pdfUrl, setPdfUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const iframeRef = useRef<HTMLIFrameElement>(null);

  // 合并外部 ref 和内部 ref
  const setRef = useCallback((el: HTMLIFrameElement | null) => {
    (iframeRef as React.MutableRefObject<HTMLIFrameElement | null>).current = el;
    if (typeof ref === "function") ref(el);
    else if (ref) (ref as React.MutableRefObject<HTMLIFrameElement | null>).current = el;
  }, [ref]);

  // 加载 PDF + _middle.json 数据
  useEffect(() => {
    setLoading(true);
    setPdfUrl(null);
    Promise.all([
      invoke<string | null>("find_asset_file", { pattern: "_origin.pdf" }),
      invoke<string | null>("find_asset_file", { pattern: "_middle.json" }),
    ]).then(([pdfPath, middlePath]) => {
      if (pdfPath) setPdfUrl(`narrativestructure://localhost/${encodeURIComponent(pdfPath)}?t=${Date.now()}`);
      // 存储 middle.json 路径，等 iframe 加载后发送
      if (middlePath) {
        (window as any).__middleJsonPath = middlePath;
      }
      setLoading(false);
    }).catch(() => setLoading(false));
  }, [projectPath]);

  // 监听 iframe 加载完成 → 发送 _middle.json 数据
  useEffect(() => {
    if (!pdfUrl) return;
    const iframe = iframeRef.current;
    if (!iframe) return;

    const onLoad = () => {
      const middlePath = (window as any).__middleJsonPath;
      if (!middlePath) return;

      // 通过 narrativestructure 协议读取 _middle.json
      const url = `narrativestructure://localhost/${encodeURIComponent(middlePath)}?raw=1`;
      fetch(url)
        .then(r => r.json())
        .then(data => {
          iframe.contentWindow?.postMessage({ type: "middle-data", data: data.pdf_info }, "*");
        })
        .catch(() => {});
    };

    iframe.addEventListener("load", onLoad);
    return () => iframe.removeEventListener("load", onLoad);
  }, [pdfUrl]);

  // 监听 PDF 翻页事件
  useEffect(() => {
    const handler = (e: MessageEvent) => {
      if (e.data?.type === "pdf-page") onPageChange?.(e.data.page);
    };
    window.addEventListener("message", handler);
    return () => window.removeEventListener("message", handler);
  }, [onPageChange]);

  if (loading) return <div className="pdf-empty">⏳ 加载 PDF...</div>;
  if (!pdfUrl) return <div className="pdf-empty">（未找到 PDF）</div>;

  return (
    <div className="pdf-viewer">
      <div className="pdf-content">
        <iframe ref={setRef} src={pdfUrl} className="pdf-embed" title="PDF Preview" />
        <PdfMirrorLayer bboxes={mirrorBboxes || []} pageRect={pageRect} />
      </div>
    </div>
  );
});

export default PdfViewer;
