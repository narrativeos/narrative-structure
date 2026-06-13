import { useEffect, useState, useRef, useCallback, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Document, Page, pdfjs } from "react-pdf";
import "react-pdf/dist/Page/AnnotationLayer.css";
import "react-pdf/dist/Page/TextLayer.css";
import PdfMirrorLayer, { MirrorBbox } from "./PdfMirrorLayer";
import "./PdfViewer.css";

// Set the worker source
pdfjs.GlobalWorkerOptions.workerSrc = new URL(
  'pdfjs-dist/build/pdf.worker.min.mjs',
  import.meta.url
).toString();

type LayoutMode = "single" | "double-horizontal" | "double-vertical";

interface PdfViewerProps {
  projectPath: string;
  onPageChange?: (page: number) => void;
  mirrorBboxes?: MirrorBbox[];
  pageRect?: { left: number; top: number; width: number; height: number } | null;
  showAnnotations?: boolean;
}

/** Auto-select layout based on screen resolution */
function autoSelectLayout(): LayoutMode {
  const w = window.innerWidth;
  const h = window.innerHeight;
  if (w >= 1600) return "double-horizontal";
  if (w >= 1200) return "double-horizontal";
  if (h > w) return "double-vertical";
  return "single";
}

/** Get visible page numbers for a given layout and current page */
function getVisiblePages(currentPage: number, layout: LayoutMode, totalPages: number): number[] {
  if (layout === "single") {
    return [currentPage];
  }
  
  // Double page: always show two pages
  // Current page and the next one (or previous if on an even page)
  let left: number;
  if (currentPage % 2 === 1) {
    // Odd page: show currentPage and currentPage+1
    left = currentPage;
  } else {
    // Even page: show currentPage-1 and currentPage
    left = currentPage - 1;
  }
  const right = left + 1;
  
  if (left < 1) return [1, 2].filter(p => p <= totalPages);
  if (right > totalPages) return [totalPages - 1, totalPages].filter(p => p >= 1);
  return [left, right];
}

const PdfViewer = function PdfViewer({ projectPath, onPageChange, mirrorBboxes, pageRect, showAnnotations }: PdfViewerProps) {
  const [pdfData, setPdfData] = useState<ArrayBuffer | null>(null);
  const [loading, setLoading] = useState(true);
  const [numPages, setNumPages] = useState(0);
  const [currentPage, setCurrentPage] = useState(1);
  const [layout, setLayout] = useState<LayoutMode>(autoSelectLayout());
  const [pageWidth, setPageWidth] = useState(600);
  const [error, setError] = useState<string | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const prevProjectPath = useRef<string>("");

  // Detect layout changes on resize
  useEffect(() => {
    const onResize = () => {
      setLayout(autoSelectLayout());
      // Recalculate page width based on container
      requestAnimationFrame(() => {
        updatePageWidth();
      });
    };
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  // Calculate page width based on container size and layout
  const updatePageWidth = useCallback(() => {
    if (!containerRef.current) return;
    const containerWidth = containerRef.current.clientWidth - 32; // padding
    const containerHeight = containerRef.current.clientHeight - 60; // padding + toolbar
    
    if (layout === "single") {
      // Single page: fit to container width, maintain aspect ratio
      const maxW = containerWidth - 40;
      const maxH = containerHeight;
      // A4 ratio is ~0.707 (width/height), so height = width / 0.707
      setPageWidth(Math.min(maxW, maxH * 0.707));
    } else {
      // Double page: each page is half of container
      const maxW = (containerWidth - 80) / 2; // two pages + gap
      const maxH = layout === "double-horizontal" ? containerHeight : (containerHeight - 80) / 2;
      setPageWidth(Math.min(maxW, maxH * 0.707));
    }
  }, [layout]);

  // Load PDF file as ArrayBuffer via Tauri invoke (read_file_bytes)
  useEffect(() => {
    if (projectPath === prevProjectPath.current) return;
    prevProjectPath.current = projectPath;
    
    setLoading(true);
    setPdfData(null);
    setCurrentPage(1);
    setError(null);
    
    invoke<string | null>("find_asset_file", { pattern: "_origin.pdf" })
      .then(async (pdfPath) => {
        if (!pdfPath) {
          setError("未找到 PDF 文件");
          setLoading(false);
          return;
        }
        try {
          // read_file_bytes returns Vec<u8> → Tauri serializes as Uint8Array
          const bytes = await invoke<Uint8Array>("read_file_bytes", { path: pdfPath });
          setPdfData(bytes.buffer as ArrayBuffer);
        } catch (e: any) {
          setError(`读取 PDF 失败: ${e.message || e}`);
          setLoading(false);
        }
      })
      .catch((e: any) => {
        setError(`查找 PDF 失败: ${e.message || e}`);
        setLoading(false);
      });
  }, [projectPath]);

  // Calculate page width when PDF loaded or layout changes
  useEffect(() => {
    if (pdfData) {
      setTimeout(updatePageWidth, 100);
    }
  }, [pdfData, layout, updatePageWidth]);

  // Page loaded callback
  const onDocumentLoadSuccess = useCallback(({ numPages: total }: any) => {
    setNumPages(total);
    setLoading(false);
    // Calculate proper page width once we know the document
    setTimeout(updatePageWidth, 50);
  }, [updatePageWidth]);

  // Document error callback
  const onDocumentLoadError = useCallback((err: any) => {
    setError(`PDF 加载错误: ${err?.message || err}`);
    setLoading(false);
  }, []);

  // Page navigation
  const goToPage = useCallback((page: number) => {
    if (page < 1) page = 1;
    if (page > numPages) page = numPages;
    setCurrentPage(page);
    onPageChange?.(page);
  }, [numPages, onPageChange]);

  const prevPage = useCallback(() => {
    goToPage(currentPage - 1);
  }, [currentPage, goToPage]);

  const nextPage = useCallback(() => {
    goToPage(currentPage + 1);
  }, [currentPage, goToPage]);

  // Keyboard shortcuts
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      // Don't intercept if user is typing in an input
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
      
      if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
        e.preventDefault();
        prevPage();
      } else if (e.key === "ArrowRight" || e.key === "ArrowDown") {
        e.preventDefault();
        nextPage();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [prevPage, nextPage]);

  // Layout toggle
  const cycleLayout = useCallback(() => {
    setLayout(prev => {
      if (prev === "single") return "double-horizontal";
      if (prev === "double-horizontal") return "double-vertical";
      return "single";
    });
  }, []);

  const layoutLabel = useMemo(() => {
    switch (layout) {
      case "single": return "📄 单页";
      case "double-horizontal": return "📖 横排";
      case "double-vertical": return "📖 竖排";
    }
  }, [layout]);

  // Build visible pages
  const visiblePages = useMemo(() => {
    if (!numPages) return [];
    return getVisiblePages(currentPage, layout, numPages);
  }, [currentPage, layout, numPages]);

  if (loading) return <div className="pdf-empty">⏳ 加载 PDF...</div>;
  if (error) return <div className="pdf-empty">❌ {error}</div>;
  if (!pdfData) return <div className="pdf-empty">（未找到 PDF）</div>;

  return (
    <div className="pdf-viewer">
      {/* Toolbar */}
      <div className="pdf-controls">
        <button
          className="pdf-ctrl-btn"
          onClick={prevPage}
          disabled={currentPage <= 1}
          title="上一页 (↑/←)"
        >
          ◀
        </button>
        <input
          className="pdf-page-input"
          type="number"
          min="1"
          max={numPages}
          value={currentPage}
          onChange={(e) => {
            const p = parseInt(e.target.value, 10);
            if (p > 0 && p <= numPages) setCurrentPage(p);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              (e.target as HTMLInputElement).blur();
              goToPage(currentPage);
            }
          }}
          title="跳转到页码"
        />
        <span className="pdf-page-total">/ {numPages}</span>
        <button
          className="pdf-ctrl-btn"
          onClick={nextPage}
          disabled={currentPage >= numPages}
          title="下一页 (↓/→)"
        >
          ▶
        </button>
        <div style={{ flex: 1 }} />
        <button
          className="pdf-ctrl-btn pdf-layout-btn"
          onClick={cycleLayout}
          title="切换布局 (单页/横排/竖排)"
          style={{ width: "auto", padding: "0 8px", fontSize: "11px" }}
        >
          {layoutLabel}
        </button>
      </div>

      {/* PDF Content */}
      <div className="pdf-content" ref={containerRef}>
        <Document
          file={{ data: pdfData }}
          onLoadSuccess={onDocumentLoadSuccess}
          onLoadError={onDocumentLoadError}
          loading={null}
        >
          <div className={`pdf-pages pdf-layout-${layout}`}>
            {visiblePages.map((pageNum) => (
              <div key={pageNum} className="pdf-page-wrapper">
                <Page
                  className="pdf-page-render"
                  pageNumber={pageNum}
                  width={pageWidth}
                  scale={undefined}
                  loading={null}
                />
              </div>
            ))}
          </div>
        </Document>
        {showAnnotations !== false && (
          <PdfMirrorLayer bboxes={mirrorBboxes || []} pageRect={pageRect} />
        )}
      </div>
    </div>
  );
};

export default PdfViewer;