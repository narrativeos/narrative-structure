import { useEffect, useState, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import PdfMirrorLayer, { MirrorBbox } from "./PdfMirrorLayer";
import "./PdfViewer.css";

export interface PageMappingData {
  pages: {
    page_num: number;
    page_size: [number, number];
    blocks: {
      block_type: string;
      bbox: [number, number, number, number];
      spans?: { content: string; bbox: [number, number, number, number] }[];
    }[];
  }[];
}

interface PdfViewerProps {
  projectPath: string;
  onPageChange?: (page: number) => void;
  mirrorBboxes?: MirrorBbox[];
  pageRect?: { left: number; top: number; width: number; height: number } | null;
  showAnnotations?: boolean;
  selectedBboxId?: string | null;
  hoveredBboxId?: string | null;
  onBboxClick?: (id: string) => void;
  onBboxRequest?: (page: number, bboxes: MirrorBbox[]) => void;
}

/**
 * PdfViewer 不再需要前端管理 PDF 路径。
 * PDF 路径由后端从当前打开的项目自动获取（搜索 *_origin.pdf）。
 */

interface PageImage {
  page_num: number;
  width: number;
  height: number;
  image_base64: string;
}

const PdfViewer = ({
  projectPath,
  onPageChange,
  mirrorBboxes,
  pageRect,
  showAnnotations,
  selectedBboxId,
  hoveredBboxId,
  onBboxClick,
  onBboxRequest,
}: PdfViewerProps) => {
  const [totalPages, setTotalPages] = useState(0);
  const [currentPage, setCurrentPage] = useState(1);
  const [pages, setPages] = useState<Record<number, PageImage>>({});
  const [loading, setLoading] = useState(true);
  const [loadingMsg, setLoadingMsg] = useState("加载 PDF...");
  const [pageMapping, setPageMapping] = useState<PageMappingData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  // LRU: 跟踪已加载的页面，超出窗口范围的自动清理
  const loadedPagesRef = useRef<Set<number>>(new Set());

  // LRU 清理：保留当前页 ±5 页，清除超出范围的旧图片
  const evictOldPages = useCallback((currPage: number) => {
    const keepRange = 5; // 保留 ±5 页
    const minKeep = Math.max(1, currPage - keepRange);
    const maxKeep = totalPages > 0 ? Math.min(totalPages, currPage + keepRange) : currPage + keepRange;

    setPages(prev => {
      const updated: Record<number, PageImage> = { ...prev };
      let evicted = false;
      loadedPagesRef.current.forEach((pageNum: number) => {
        if (pageNum < minKeep || pageNum > maxKeep) {
          delete updated[pageNum];
          loadedPagesRef.current.delete(pageNum);
          evicted = true;
        }
      });
      return evicted ? updated : prev;
    });
  }, [totalPages]);

  // 从 page mapping 计算 bbox 位置
  const calculateBboxes = useCallback((
    pageNum: number,
    img: PageImage,
    mapping: PageMappingData
  ): MirrorBbox[] => {
    const pageData = mapping.pages.find((p) => p.page_num === pageNum);
    if (!pageData) return [];

    const [origWidth, origHeight] = pageData.page_size;
    const scaleX = img.width / origWidth;
    const scaleY = img.height / origHeight;

    const bboxes: MirrorBbox[] = [];
    pageData.blocks.forEach((block, idx) => {
      if (block.block_type === 'empty') return;
      const [x1, y1, x2, y2] = block.bbox;
      bboxes.push({
        x: x1 * scaleX,
        y: y1 * scaleY,
        w: (x2 - x1) * scaleX,
        h: (y2 - y1) * scaleY,
        id: `block-${pageNum}-${idx}`,
        block_type: block.block_type,
      });
    });

    return bboxes;
  }, []);

  // 加载指定范围的页面（PDF 路径由后端自动获取）
  const loadPageRange = useCallback(
    async (page: number, total: number) => {
      const start = Math.max(1, page - 1);
      const end = Math.min(total, page + 1);
      const pageNumbers: number[] = [];
      for (let i = start; i <= end; i++) {
        pageNumbers.push(i);
      }

      try {
        setLoadingMsg(`渲染页面 ${start}-${end}...`);
        const newPages = await invoke<PageImage[]>("render_pdf_pages", {
          PageNumbers: pageNumbers,
          Dpi: 150,
        });

        setPages((prev) => {
          const updated = { ...prev };
          newPages.forEach((p) => {
            updated[p.page_num] = p;
            loadedPagesRef.current.add(p.page_num);
          });
          return updated;
        });

        // LRU 清理：加载新页面后清除超出范围的旧图片
        evictOldPages(page);

        // 请求 bbox 数据
        if (onBboxRequest) {
          const img = newPages.find((p) => p.page_num === page);
          if (img && pageMapping) {
            const bboxes = calculateBboxes(page, img, pageMapping);
            onBboxRequest(page, bboxes);
          }
        }
      } catch (err) {
        console.error("[PdfViewer] Failed to load pages:", err);
        setError(`渲染页面失败: ${err}`);
      }
    },
    [pageMapping, onBboxRequest, calculateBboxes, evictOldPages]
  );

  // 按需加载指定页范围的 page mapping
  const loadPageMappingRange = useCallback(
    async (page: number, total: number) => {
      const start = Math.max(1, page - 1);
      const end = Math.min(total, page + 1);
      try {
        const mappingJson = await invoke<string | null>("get_page_mapping_range", {
          PageStart: start,
          PageEnd: end,
        });
        if (mappingJson) {
          try {
            const mapping = JSON.parse(mappingJson);
            setPageMapping(mapping);
          } catch (e) {
            console.error("[PdfViewer] Failed to parse page mapping range:", e);
          }
        }
      } catch (e) {
        console.error("[PdfViewer] Failed to load page mapping range:", e);
      }
    },
    []
  );

  // 加载页数和 page mapping（PDF 路径由后端自动获取）
  useEffect(() => {
    // 没有项目路径时不初始化（等待项目打开）
    if (!projectPath) {
      return;
    }

    setLoading(true);
    setError(null);
    setLoadingMsg("加载 PDF...");
    setPages({});
    setCurrentPage(1);
    loadedPagesRef.current.clear();

    // 设置超时（使用 ref 来追踪是否已完成）
    const completedRef = { value: false };
    const timeout = setTimeout(() => {
      if (!completedRef.value) {
        setLoading(false);
        setError("加载超时（3分钟），PDF 文件可能过大或格式不支持");
      }
    }, 180000); // 180秒超时（后端渲染超时 120秒）

    // 只加载页数，page mapping 按需加载
    invoke<number>("get_pdf_page_count")
      .then(async (count) => {
        setTotalPages(count);

        // 按需加载初始页的 page mapping (1, 2, 3)
        await loadPageMappingRange(1, count);

        // 加载初始页面 (1, 2, 3)
        setLoadingMsg("渲染页面...");
        await loadPageRange(1, count);
        clearTimeout(timeout);
        completedRef.value = true;
        setLoading(false);
      })
      .catch((e) => {
        console.error("[PdfViewer] Failed to initialize:", e);
        clearTimeout(timeout);
        completedRef.value = true;
        setLoading(false);
        setError(`初始化失败: ${e}`);
      });
  }, [projectPath, loadPageRange, loadPageMappingRange]);

  // 翻页处理
  const goToPage = useCallback(
    (page: number) => {
      if (page < 1 || page > totalPages) return;
      setCurrentPage(page);
      onPageChange?.(page);
      // 按需加载新页范围的 page mapping
      loadPageMappingRange(page, totalPages);
      loadPageRange(page, totalPages);
    },
    [totalPages, onPageChange, loadPageRange, loadPageMappingRange]
  );

  // 键盘翻页
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "ArrowDown" || e.key === "PageDown" || e.key === " ") {
        e.preventDefault();
        goToPage(currentPage + 1);
      }
      if (e.key === "ArrowUp" || e.key === "PageUp") {
        e.preventDefault();
        goToPage(currentPage - 1);
      }
      if (e.key === "Home") {
        e.preventDefault();
        goToPage(1);
      }
      if (e.key === "End") {
        e.preventDefault();
        goToPage(totalPages);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [currentPage, totalPages, goToPage]);

  // 鼠标滚轮翻页
  useEffect(() => {
    let wheelTimer: any = null;
    const handler = (e: WheelEvent) => {
      e.preventDefault();
      clearTimeout(wheelTimer);
      wheelTimer = setTimeout(() => {
        if (e.deltaY > 30) goToPage(currentPage + 1);
        else if (e.deltaY < -30) goToPage(currentPage - 1);
      }, 80);
    };
    const container = containerRef.current;
    if (container) {
      container.addEventListener("wheel", handler, { passive: false });
    }
    return () => {
      if (container) {
        container.removeEventListener("wheel", handler);
      }
    };
  }, [currentPage, totalPages, goToPage]);

  // 显示错误
  if (error) {
    return (
      <div className="pdf-empty" style={{ flexDirection: "column", gap: "10px", padding: "20px" }}>
        <span>❌ {error}</span>
        <span style={{ fontSize: "12px", color: "#888" }}>
          提示：对于超大 PDF（如单页超过 10000 行），建议使用 PDF.js iframe 方式查看
        </span>
      </div>
    );
  }

  if (loading) return <div className="pdf-empty">⏳ {loadingMsg}</div>;
  if (totalPages === 0) return <div className="pdf-empty">（未找到 PDF）</div>;

  // 获取当前页和相邻页的图片
  const prevPage = pages[currentPage - 1];
  const currPage = pages[currentPage];
  const nextPage = pages[currentPage + 1];

  return (
    <div className="pdf-viewer" ref={containerRef}>
      <div className="pdf-content">
        <div className="page-stage">
          {/* 上一页 / 上方占位 */}
          {currentPage > 1 ? (
            prevPage ? (
              <div className="page-wrap prev-page" id={`page-${currentPage - 1}`}>
                <img
                  src={`data:image/png;base64,${prevPage.image_base64}`}
                  alt={`Page ${currentPage - 1}`}
                  style={{ width: "100%", height: "auto" }}
                />
                <div className="page-num">p{currentPage - 1}</div>
              </div>
            ) : (
              <div className="page-wrap prev-page placeholder-page loading-placeholder" />
            )
          ) : (
            <div className="page-wrap prev-page placeholder-page" />
          )}

          {/* 当前页 */}
          {currPage && (
            <div className="page-wrap current-page" id={`page-${currentPage}`}>
              <img
                src={`data:image/png;base64,${currPage.image_base64}`}
                alt={`Page ${currentPage}`}
                style={{ width: "100%", height: "auto" }}
              />
              <div className="page-num">p{currentPage}</div>
            </div>
          )}

          {/* 下一页 / 下方占位 */}
          {currentPage < totalPages ? (
            nextPage ? (
              <div className="page-wrap next-page" id={`page-${currentPage + 1}`}>
                <img
                  src={`data:image/png;base64,${nextPage.image_base64}`}
                  alt={`Page ${currentPage + 1}`}
                  style={{ width: "100%", height: "auto" }}
                />
                <div className="page-num">p{currentPage + 1}</div>
              </div>
            ) : (
              <div className="page-wrap next-page placeholder-page loading-placeholder" />
            )
          ) : (
            <div className="page-wrap next-page placeholder-page" />
          )}
        </div>

        {/* 标注层 */}
        <PdfMirrorLayer
          bboxes={mirrorBboxes || []}
          pageRect={pageRect}
          visible={showAnnotations !== false}
          selectedId={selectedBboxId}
          hoveredId={hoveredBboxId}
          onBboxClick={onBboxClick}
        />
      </div>

      {/* 页码指示器 */}
      <div className="page-indicator">
        {currentPage} / {totalPages}
      </div>

      {/* 翻页按钮 */}
      <div className="nav-buttons">
        <button onClick={() => goToPage(currentPage - 1)} disabled={currentPage <= 1}>
          ⬆
        </button>
        <button onClick={() => goToPage(currentPage + 1)} disabled={currentPage >= totalPages}>
          ⬇
        </button>
      </div>
    </div>
  );
};

export default PdfViewer;