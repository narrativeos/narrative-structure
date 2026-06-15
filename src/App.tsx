import { useState, useCallback, useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
// PageController Bridge — 初始化 Page Agent 的 DOM 操作能力
import "./lib/pageControllerBridge";
// Agent Proxy — 通过 postMessage 与外部通信，绕过 Tauri 安全限制
import { setupAgentProxy } from "./lib/agentProxy";
import {
  Panel,
  Group,
  Separator,
} from "react-resizable-panels";
import { Sun, Moon } from "lucide-react";
import { useTheme } from "./components/ThemeProvider";
import TOC from "./components/TOC";
import BlockEditor from "./components/Editor";
import type { MirrorBbox } from "./components/PdfMirrorLayer";
import MarkdownPreview from "./components/MarkdownPreview";
import FileExplorer from "./components/FileExplorer";
import PdfViewer from "./components/PdfViewer";
import AgentConsole from "./components/AgentConsole";
import PipelineStatus from "./components/PipelineStatus";
import LogPanel from "./components/LogPanel";
import LinesLayer, { LineDef } from "./components/LinesLayer";

export interface TocNode {
  id: string;
  parent_id: string | null;
  order_idx: number;
  level: number;
  block_type: string;
  content_preview: string;
  children: TocNode[];
}

export interface Block {
  id: string;
  parent_id: string | null;
  order_idx: number;
  level: number;
  block_type: string;
  content: string;
  original_content: string;
  metadata: string;
  version: number;
  created_at: string;
  updated_at: string;
}

function countNodes(node: TocNode): number {
  return 1 + node.children.reduce((s, c) => s + countNodes(c), 0);
}

// ---- 最近项目持久化 ----
interface ImportProgress { stage: string; percent: number; detail: string; }
interface RecentProject { name: string; path: string; time: number }
const RECENT_KEY = "narrative-structure-recent";

function loadRecent(): RecentProject[] {
  try { return JSON.parse(localStorage.getItem(RECENT_KEY) || "[]"); } catch { return []; }
}
function saveRecent(list: RecentProject[]) {
  localStorage.setItem(RECENT_KEY, JSON.stringify(list.slice(0, 10)));
}
function addRecent(name: string, path: string) {
  const list = loadRecent().filter(r => r.path !== path);
  list.unshift({ name, path, time: Date.now() });
  saveRecent(list);
}

function fmtTime(ts: number): string {
  const d = new Date(ts);
  return `${d.getMonth()+1}/${d.getDate()} ${d.getHours()}:${String(d.getMinutes()).padStart(2,"0")}`;
}

function App() {
  const { theme, setTheme } = useTheme();
  const [projectPath, setProjectPath] = useState<string | null>(null);
  const [projectName, setProjectName] = useState("");
  const [tocTree, setTocTree] = useState<TocNode[]>([]);
  const [activeBlock, setActiveBlock] = useState<Block | null>(null);
  const [pageBlocks, setPageBlocks] = useState<Block[] | null>(null);
  const [statusMsg, setStatusMsg] = useState("");
  const [recentProjects, setRecentProjects] = useState<RecentProject[]>(loadRecent);
  const refreshRecent = useCallback(() => setRecentProjects(loadRecent()), []);
  const [projectKey, setProjectKey] = useState(0);
  const pdfIframeRef = useRef<HTMLIFrameElement>(null);
  const [lines, setLines] = useState<LineDef[]>([]);
  const [mirrorBboxes, setMirrorBboxes] = useState<MirrorBbox[]>([]);
  const [pageRect, setPageRect] = useState<{left:number;top:number;width:number;height:number}|null>(null);
  const currentPageRef = useRef(1);
  const [importProgress, setImportProgress] = useState<ImportProgress | null>(null);
  const [importLogs, setImportLogs] = useState<string[]>([]);
  const importStages = ["解压 ZIP", "初始化数据库", "解析 Markdown", "加载信息层", "匹配页码", "写入数据库", "项目准备", "完成"];
  const importProgressTimer = useRef<number | null>(null);
  const workspaceRef = useRef<HTMLDivElement>(null);
  const bboxRequestIdRef = useRef(0);
  const pageTextsRef = useRef<string[]>([]);
  const scrollBboxTimerRef = useRef<ReturnType<typeof setTimeout>>();
  const drawLinesRef = useRef<() => void>();
  const requestBboxRef = useRef<() => void>();
  const [showAnnotations, setShowAnnotations] = useState(true);
  const [showFlyLines, setShowFlyLines] = useState(true);
  const [selectedBboxId, setSelectedBboxId] = useState<string | null>(null);
  const [hoveredBboxId, setHoveredBboxId] = useState<string | null>(null);
  const pageReqIdRef = useRef(0);
  const pageBlocksRef = useRef<Block[] | null>(null);
  const [displayPage, setDisplayPage] = useState(1); // 触发 UI 重渲染
  const [pageInput, setPageInput] = useState("");

  // 初始化 Agent Proxy (postMessage 通信)
  useEffect(() => {
    setupAgentProxy();
  }, []);

  // 区块标注开关 → 同步 iframe 信息层
  useEffect(() => {
    pdfIframeRef.current?.contentWindow?.postMessage({ type: "set-overlay", visible: showAnnotations }, "*");
  }, [showAnnotations]);

  // pageBlocks 变化 → 请求 bbox 数据填充 mirror 层（仅当前页）
  useEffect(() => {
    setMirrorBboxes([]); setLines([]); setPageRect(null);
    requestBboxRef.current?.();
  }, [pageBlocks]);

    // 接收 bbox-pos → mirror 层 + 飞线；接收 scroll-offset → mirror 滚动
    useEffect(() => {
      const drawAllLines = () => {
        const wsRect = workspaceRef.current?.getBoundingClientRect();
        if (!wsRect) return;
        const typeColor: Record<string, [string, string]> = {
          heading: ['#ef4444', '#dc2626'],
          text: ['#3b82f6', '#60a5fa'],
          interline_equation: ['#10b981', '#34d399'],
          table: ['#f59e0b', '#fbbf24'],
          image: ['#8b5cf6', '#a78bfa'],
          empty: ['#6b7280', '#9ca3af'],
        };
        const newLines: LineDef[] = [];
        let prevType = '', alt = false;
        const currentPage = currentPageRef.current;
        // 只绘制当前页的飞线 — 过滤 block 属于当前页的元素
        document.querySelectorAll('[data-block-id]').forEach(el => {
          const id = el.getAttribute('data-block-id')!;
          // 检查该 block 是否属于当前页
          let blockPage = 0;
          try {
            const blockData = (window as any).__flyRowMap?.[id];
            if (blockData?.page) blockPage = blockData.page;
          } catch {}
          if (blockPage !== currentPage) return;
          const mirrorEl = document.querySelector(`[data-mirror-id="${id}"]`);
          if (!mirrorEl) return;
          const r1 = el.getBoundingClientRect();
          const r2 = mirrorEl.getBoundingClientRect();
          let btype = 'text';
          el.classList.forEach(c => { if (typeColor[c]) btype = c; });
          if (btype === prevType) { alt = !alt; } else { alt = false; prevType = btype; }
          const colors = typeColor[btype] || ['#fbbf24', '#fcd34d'];
          newLines.push({ id: `line-${id}`, x1: r1.left - wsRect.left, y1: r1.top + 6 - wsRect.top, x2: r2.left + r2.width - wsRect.left, y2: r2.top + r2.height/2 - wsRect.top, color: colors[alt ? 1 : 0], active: true });
        });
        setLines(newLines);
      };
      drawLinesRef.current = drawAllLines;
      // 从三页（page-1, page, page+1）获取 blocks 数据 → 请求 bbox
      const requestBboxForVisiblePages = () => {
        const blocks = pageBlocksRef.current;
        if (!blocks?.length) return;
        const iframe = pdfIframeRef.current?.contentWindow;
        if (!iframe) return;
        const page = currentPageRef.current;
        // 过滤三页的 blocks: page-1, page, page+1
        const threePageBlocks = blocks.filter(b => {
          try {
            const bpage = JSON.parse(b.metadata||'{}').page||0;
            return bpage >= page - 1 && bpage <= page + 1;
          } catch { return false; }
        });
        // 按页码分组，向 iframe 发送三页的 get-bbox-pos 请求
        const pageMap = new Map<number, Block[]>();
        threePageBlocks.forEach(b => {
          try {
            const bpage = JSON.parse(b.metadata||'{}').page||0;
            if (bpage > 0 && b.block_type !== 'empty' && b.content.trim()) {
              if (!pageMap.has(bpage)) pageMap.set(bpage, []);
              pageMap.get(bpage)!.push(b);
            }
          } catch {}
        });
        // 当前页的 texts 用于飞线匹配
        const currentPageBlocks = pageMap.get(page) || [];
        const pageTexts = currentPageBlocks.map(b => b.content);
        pageTextsRef.current = pageTexts;
        // 构建 flyRowMap — 只包含当前页的 block（用于飞线绘制过滤）
        const flyRowMap: Record<string, { id: string, content: string, page: number, block_type: string }> = {};
        currentPageBlocks.forEach(b => {
          flyRowMap[b.id] = { id: b.id, content: b.content, page, block_type: b.block_type };
        });
        (window as any).__flyRowMap = flyRowMap;
        // 收集三页的所有 bboxes 用于 mirror 层渲染
        const allBboxesPromise: Promise<MirrorBbox[]>[] = [];
        const reqId = ++bboxRequestIdRef.current;
        (window as any).__flyReqId = reqId;
        // 对每一页发送请求
        pageMap.forEach((pageBlocks, p) => {
          const texts = pageBlocks.map(b => b.content);
          if (texts.length > 0) {
            // 构建该页的 flyRows
            const pageFlyRows = pageBlocks.map(b => ({ id: b.id, content: b.content, page: p, block_type: b.block_type }));
            allBboxesPromise.push(
              new Promise<MirrorBbox[]>((resolve) => {
                const oneTimeHandler = (e: MessageEvent) => {
                  if (e.data?.type === 'bbox-pos' && e.data.page === p) {
                    const bboxes = (e.data.bboxes || []).map((bb: any, bi: number) => ({
                      x: bb.x, y: bb.y, w: bb.w, h: bb.h,
                      id: pageFlyRows[bi]?.id || "",
                      block_type: pageFlyRows[bi]?.block_type
                    })).filter((bb: MirrorBbox) => bb.id);
                    window.removeEventListener('message', oneTimeHandler);
                    resolve(bboxes);
                  }
                };
                window.addEventListener('message', oneTimeHandler);
                iframe.postMessage({ type: "get-bbox-pos", page: p, texts }, "*");
              })
            );
          }
        });
        // 等待所有页的 bbox 结果汇总
        if (allBboxesPromise.length > 0) {
          Promise.all(allBboxesPromise).then(allBboxes => {
            if (reqId === bboxRequestIdRef.current) {
              const flatBboxes = allBboxes.flat();
              setMirrorBboxes(flatBboxes);
              requestAnimationFrame(drawAllLines);
            }
          });
        } else if (pageTexts.length > 0) {
          // 只有当前页有内容
          (window as any).__flyRows = currentPageBlocks.map(b => ({ id: b.id, content: b.content }));
          iframe.postMessage({ type: "get-bbox-pos", page, texts: pageTexts }, "*");
        }
      };
      requestBboxRef.current = requestBboxForVisiblePages;
      let rafId = 0;
      const handler = (e: MessageEvent) => {
        if (e.data?.type === 'pdf-scroll-offset') {
          cancelAnimationFrame(rafId); rafId = requestAnimationFrame(drawAllLines);
          // 滚动时也刷新 bbox 位置（debounce 150ms）— 重新请求三页
          clearTimeout(scrollBboxTimerRef.current);
          scrollBboxTimerRef.current = setTimeout(() => {
            requestBboxRef.current?.();
          }, 150);
          return;
        }
        // iframe 内 👁 按钮 → 同步父窗口 🏷️ 状态
        if (e.data?.type === 'overlay-toggled') {
          setShowAnnotations(e.data.visible);
          return;
        }
        // bbox-pos 响应 — 已由 requestBboxForVisiblePages 中的 Promise 处理
        // 但保留向后兼容：处理单次请求的情况
        const reqId = (window as any).__flyReqId;
        if (reqId !== undefined && reqId !== bboxRequestIdRef.current) return;
        if (e.data?.type !== 'bbox-pos' || !e.data.bboxes?.length) return;
        const flyRowMap = (window as any).__flyRowMap || {};
        const rows: any[] = Object.values(flyRowMap);
        const bboxes = e.data.bboxes.map((bb: any, bi: number) => ({ x: bb.x, y: bb.y, w: bb.w, h: bb.h, id: rows[bi]?.id || "", block_type: rows[bi]?.block_type })).filter((bb: MirrorBbox) => bb.id);
        if (e.data.pageRect) setPageRect(e.data.page);
        setMirrorBboxes(bboxes);
        requestAnimationFrame(drawAllLines);
      };
    window.addEventListener('message', handler);
    // Blocks 区域滚动 → 动态重绘飞线端点
    let scrollEl: Element | null = null;
    const onBlockScroll = () => { cancelAnimationFrame(rafId); rafId = requestAnimationFrame(drawAllLines); };
    const timer = setInterval(() => {
      if (!scrollEl) {
        // 使用 ScrollArea 的 viewport 元素 (data-slot="scroll-area-viewport") 作为滚动容器
        const viewport = document.querySelector('.page-blocks-scroll [data-slot="scroll-area-viewport"]');
        if (viewport) {
          scrollEl = viewport;
          scrollEl.addEventListener('scroll', onBlockScroll, { passive: true });
        }
      }
    }, 500);
    return () => { window.removeEventListener('message', handler); clearInterval(timer); clearTimeout(scrollBboxTimerRef.current); if (scrollEl) scrollEl.removeEventListener('scroll', onBlockScroll); };
  }, []);

  const createEmptyPagePlaceholder = useCallback((page: number): Block => ({
    id: `empty-page-${page}`,
    parent_id: null,
    order_idx: 0,
    level: 0,
    block_type: "empty",
    content: "",
    original_content: "",
    metadata: JSON.stringify({ page }),
    version: 1,
    created_at: "",
    updated_at: "",
  }), []);

  const fillPageWindow = useCallback((blocks: Block[], pageStart: number, pageEnd: number) => {
    const pageMap = new Map<number, Block[]>();
    for (const b of blocks) {
      let p = 0;
      try { p = JSON.parse(b.metadata || "{}").page || 0; } catch {}
      if (p <= 0 || p < pageStart || p > pageEnd) continue;
      if (!pageMap.has(p)) pageMap.set(p, []);
      pageMap.get(p)!.push(b);
    }
    const filled: Block[] = [];
    for (let p = pageStart; p <= pageEnd; p++) {
      const group = pageMap.get(p);
      if (group && group.length > 0) {
        group.sort((a, b) => a.order_idx - b.order_idx);
        filled.push(...group);
      } else {
        filled.push(createEmptyPagePlaceholder(p));
      }
    }
    return filled;
  }, [createEmptyPagePlaceholder]);

  // =========================================================================
  // 加载项目
  // =========================================================================
  const clearImportProgressTimer = useCallback(() => {
    if (importProgressTimer.current !== null) {
      window.clearInterval(importProgressTimer.current);
      importProgressTimer.current = null;
    }
  }, []);

  const stageMaxPercent = useCallback((stage: string): number => {
    switch (stage) {
      case "解压 ZIP": return 3;
      case "初始化数据库": return 4;
      case "解析 Markdown": return 5;
      case "加载信息层": return 6;
      case "匹配页码": return 90;
      case "写入数据库": return 94;
      case "项目准备": return 99;
      default: return 100;
    }
  }, []);

  const startImportProgressHeartbeat = useCallback(() => {
    clearImportProgressTimer();
    importProgressTimer.current = window.setInterval(() => {
      setImportProgress((prev) => {
        if (!prev) {
          return prev;
        }
        // 匹配页码阶段由后端驱动进度，心跳不介入
        if (prev.stage === "匹配页码") {
          return prev;
        }
        const maxPercent = stageMaxPercent(prev.stage);
        if (prev.percent >= maxPercent) {
          return prev;
        }
        // 窄阶段 (max-min <= 3) 用 1% 增量
        const narrowStages = new Set(["解压 ZIP", "初始化数据库", "解析 Markdown", "加载信息层"]);
        const delta = narrowStages.has(prev.stage) ? 1 : 3;
        return {
          ...prev,
          percent: Math.min(prev.percent + delta, maxPercent),
        };
      });
    }, 300);
  }, [clearImportProgressTimer, stageMaxPercent]);

  const stopImportProgressHeartbeat = useCallback(() => {
    clearImportProgressTimer();
  }, [clearImportProgressTimer]);

  const loadProject = useCallback(async (path: string, name?: string) => {
    try {
      const msg = await invoke<string>("open_project", { path });
      const pName = name || path.split("/").pop() || path;
      setProjectPath(path);
      setProjectName(pName);
      setProjectKey(k => k + 1);
      setStatusMsg(msg);
      addRecent(pName, path);
      const toc = await invoke<TocNode[]>("get_toc");
      setTocTree(toc);
      // 页码统计
      const stats = await invoke<[number, number][]>("get_page_stats");
      setImportLogs(prev => [...prev, `📊 页码分布: ${stats.length} 个不同页码，共 ${stats.reduce((s,[,c]) => s+c, 0)} 行`,
        ...stats.map(([p, c]) => `  p${p}: ${c} 行`).slice(0, 30),
        stats.length > 30 ? `  ... 共 ${stats.length} 页` : ""
      ].filter(Boolean));
    } catch (err) {
      setStatusMsg(`错误: ${err}`);
    }
  }, []);

  // =========================================================================
  // 导入文档 = 新建项目
  // =========================================================================
  const handleImportNewProject = useCallback(async () => {
    let unlistenProgress: (() => void) | null = null;
    let unlistenLog: (() => void) | null = null;
    try {
      setStatusMsg("正在打开文件选择器...");
      const selected = await open({
        filters: [{ name: "ZIP 压缩包", extensions: ["zip"] }],
        multiple: false,
        title: "选择 MinerU 输出 zip 包",
      });

      if (!selected) {
        setStatusMsg("已取消选择");
        return;
      }

      const zipPath = typeof selected === "string" ? selected : String(selected);
      setStatusMsg(`正在导入: ${zipPath} ...`);
      setImportProgress({ stage: "解压 ZIP", percent: 1, detail: "读取压缩包..." });
      setImportLogs([]);
      startImportProgressHeartbeat();

      unlistenProgress = await listen<ImportProgress>("import-progress", (e) => {
        setImportProgress((prev) => {
          const next = e.payload;
          if (!prev) return next;
          const prevIdx = importStages.indexOf(prev.stage);
          const nextIdx = importStages.indexOf(next.stage);
          if (nextIdx >= 0 && prevIdx >= 0 && nextIdx < prevIdx) {
            return prev;
          }
          const percent = Math.max(prev.percent, next.percent);
          return { stage: next.stage, percent, detail: next.detail };
        });
      });
      unlistenLog = await listen<string>("import-log", (e) => {
        setImportLogs(prev => [...prev.slice(-19), e.payload]);
      });

      // 短暂延迟，让 React 渲染进度条和事件监听器完成
      await new Promise(r => setTimeout(r, 30));

      const msg = await invoke<string>("import_new_project", {
        zipPath: zipPath,
      });

      // 保持进度条至少显示 600ms（防止一闪而过）
      await new Promise(r => setTimeout(r, 600));
      setImportProgress(null);
      setStatusMsg(msg);

      const parts = msg.split(" | ");
      const name = parts[0] || "";
      const pathPart = parts[2] || "";

      setProjectName(name);
      setProjectPath(pathPart.trim());
      setProjectKey(k => k + 1);
      addRecent(name, pathPart.trim());

      const toc = await invoke<TocNode[]>("get_toc");
      setTocTree(toc);
    } catch (err) {
      setStatusMsg(`导入失败: ${err}`);
      setImportProgress(null);
    } finally {
      stopImportProgressHeartbeat();
      unlistenProgress?.();
      unlistenLog?.();
    }
  }, [startImportProgressHeartbeat, stopImportProgressHeartbeat]);

  // =========================================================================
  // 打开已有项目（原生文件夹选择）
  // =========================================================================
  const handleOpenProject = useCallback(async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "选择项目文件夹（包含 narrative.db）",
      });

      if (selected) {
        await loadProject(selected as string);
      }
    } catch (err) {
      setStatusMsg(`打开失败: ${err}`);
    }
  }, [loadProject]);

  // =========================================================================
  // 关闭当前项目 → 回到欢迎页
  // =========================================================================
  const handleCloseProject = useCallback(async () => {
    try { await invoke<string>("close_project"); } catch {}
    setProjectPath(null);
    setTocTree([]);
    setActiveBlock(null);
    setStatusMsg("");
    refreshRecent();
  }, [refreshRecent]);

  // =========================================================================
  // 导入 MinerU 输出 zip
  // =========================================================================
  const handleImportDocument = useCallback(async () => {
    let unlistenProgress: (() => void) | null = null;
    let unlistenLog: (() => void) | null = null;
    try {
      const selected = await open({
        filters: [{ name: "ZIP 压缩包", extensions: ["zip"] }],
        multiple: false,
        title: "选择 MinerU 输出 zip 包",
      });

      if (!selected) return;

      setStatusMsg("正在导入...");
      setImportProgress({ stage: "解压 ZIP", percent: 1, detail: "读取压缩包..." });
      setImportLogs([]);
      startImportProgressHeartbeat();

      unlistenProgress = await listen<ImportProgress>("import-progress", (e) => {
        setImportProgress((prev) => {
          const next = e.payload;
          if (!prev) return next;
          const prevIdx = importStages.indexOf(prev.stage);
          const nextIdx = importStages.indexOf(next.stage);
          if (nextIdx >= 0 && prevIdx >= 0 && nextIdx < prevIdx) {
            return prev;
          }
          const percent = Math.max(prev.percent, next.percent);
          return { stage: next.stage, percent, detail: next.detail };
        });
      });
      unlistenLog = await listen<string>("import-log", (e) => {
        setImportLogs(prev => [...prev.slice(-19), e.payload]);
      });

      await new Promise(r => setTimeout(r, 30));

      const msg = await invoke<string>("import_document", {
        zipPath: selected as string,
      });

      await new Promise(r => setTimeout(r, 600));
      setImportProgress(null);
      setStatusMsg(msg);

      const toc = await invoke<TocNode[]>("get_toc");
      setTocTree(toc);
    } catch (err) {
      setStatusMsg(`导入失败: ${err}`);
      setImportProgress(null);
    } finally {
      stopImportProgressHeartbeat();
      unlistenProgress?.();
      unlistenLog?.();
    }
  }, [startImportProgressHeartbeat, stopImportProgressHeartbeat]);

  // PDF 翻页：每次直接根据当前PDF页码加载三页（page-1, page, page+1）
  const handlePageChange = useCallback(async (page: number) => {
    const reqId = ++pageReqIdRef.current;
    try {
      const pageStart = Math.max(1, page - 1);
      const pageEnd = page + 1; // 3 页窗口（当前 ±1）
      const blocks = await invoke<Block[]>("get_blocks_by_page", { pageStart, pageEnd });
      if (reqId !== pageReqIdRef.current) return;
      const loadedPages = new Set<number>();
      for (const b of blocks) {
        try {
          const p = JSON.parse(b.metadata || "{}").page || 0;
          if (p > 0) loadedPages.add(p);
        } catch {}
      }
      setImportLogs(prev => [...prev.slice(-19), `📖 翻到 p${page} → 请求 p${pageStart}-p${pageEnd}（3页窗口），实际 ${loadedPages.size} 页/${blocks.length} 行`]);
      const filledBlocks = fillPageWindow(blocks, pageStart, pageEnd);
      currentPageRef.current = page;
      setDisplayPage(page);
      pageBlocksRef.current = filledBlocks;
      setPageBlocks(filledBlocks);
      setActiveBlock(null);
    } catch {
      if (reqId !== pageReqIdRef.current) return;
      try {
        const blocks = await invoke<Block[]>("get_blocks_paginated", { limit: 1, offset: page - 1 });
        if (reqId !== pageReqIdRef.current) return;
        currentPageRef.current = page;
        setDisplayPage(page);
        if (blocks.length > 0) {
          pageBlocksRef.current = blocks;
          setPageBlocks(blocks);
        }
      } catch {}
    }
  }, [fillPageWindow]);

  // BlockEditor 和 MarkdownPreview 直接使用 pageBlocks（完整窗口）
  const handleSelectBlock = useCallback(async (nodeId: string) => {
    try {
      const blocks = await invoke<Block[]>("get_block_chunk", { id: nodeId });
      if (blocks.length === 0) return;
      const head = blocks[0];
      setActiveBlock(head);
      setPageBlocks(null); // 切换到单块模式

      // 用 metadata.page 精确跳转 PDF
      let targetPage = 0;
      try {
        const meta = JSON.parse(head.metadata || "{}");
        if (meta.page) targetPage = meta.page as number;
      } catch {}
      if (targetPage > 0) {
        pdfIframeRef.current?.contentWindow?.postMessage({ type: "navigate", page: targetPage }, "*");
      }
    } catch (err) {
      setStatusMsg(`加载块失败: ${err}`);
    }
  }, []);

  const handleContentChange = useCallback(
    async (blockId: string, newContent: string, version: number) => {
      try {
        const updated = await invoke<Block>("update_block", {
          id: blockId,
          content: newContent,
          expectedVersion: version,
        });
        setActiveBlock(updated);
        setStatusMsg("已保存");
      } catch (err) {
        setStatusMsg(`保存失败: ${err}`);
      }
    },
    [],
  );

  // =========================================================================
  // 欢迎页 (无项目打开时)
  // =========================================================================
  if (!projectPath) {
    return (
      <div className="welcome-screen">
        <button
          className="topbar-theme-btn"
          style={{ position: "fixed", top: 16, right: 16, zIndex: 9999 }}
          onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
          title={theme === "dark" ? "切换浅色模式" : "切换深色模式"}
        >
          {theme === "dark" ? <Sun className="w-4 h-4" /> : <Moon className="w-4 h-4" />}
        </button>
        <div className="welcome-card">
          <div className="welcome-icon">
            <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/><polyline points="10 9 9 9 8 9"/></svg>
          </div>
          <h1 className="welcome-title">NarrativeStructure</h1>
          <p className="welcome-subtitle">文档智能化重构工作台</p>
          <p className="welcome-desc">
            导入 MinerU 输出的 zip 压缩包，自动解析为语义块并构建可编辑的文档树
          </p>

          <div className="welcome-actions">
            <button
              className="btn-primary"
              onClick={() => handleImportNewProject()}
            >
              📥 导入文档开始
            </button>
            <button className="btn-secondary" onClick={handleOpenProject}>
              📂 打开已有项目
            </button>
          </div>

          {statusMsg && <p className="welcome-status">{statusMsg}</p>}

          <div className="welcome-recent">
            <h4>最近打开</h4>
            {recentProjects.length === 0 ? (
              <p className="welcome-recent-empty">（暂无历史记录）</p>
            ) : (
              <div className="recent-list">
                {recentProjects.map((r) => (
                  <div
                    key={r.path}
                    className="recent-item"
                    onClick={() => loadProject(r.path, r.name)}
                    title={r.path}
                  >
                    <span className="recent-icon">📁</span>
                    <span className="recent-name">{r.name}</span>
                    <span className="recent-time">{fmtTime(r.time)}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>

        {importProgress && (
          <div className="import-overlay">
            <div className="import-card">
              <div className="import-stage">{importProgress.stage}</div>
              <div className="import-bar-wrap">
                <div className="import-bar" style={{ width: `${importProgress.percent}%` }} />
              </div>
              <div className="import-detail">{importProgress.detail}</div>
              <div className="import-pct">{importProgress.percent}%</div>
              {importLogs.length > 0 && (
                <div className="import-logs">
                  {importLogs.map((log, i) => (
                    <div key={i} className="import-log-line">{log}</div>
                  ))}
                </div>
              )}
            </div>
          </div>
        )}
      </div>
    );
  }

  // =========================================================================
  // 主界面
  // =========================================================================
  return (
    <div className="app-shell">
      {/* ======== 顶栏：固定高度，固定顶部 ======== */}
      <header className="topbar">
        <div className="topbar-left">
          <h1 className="topbar-logo">NarrativeStructure</h1>
          <div className="topbar-project">
            <span className="topbar-project-path" title={projectPath}>📁 {projectName}</span>
            <button className="btn-close" onClick={handleCloseProject} title="关闭项目">✕</button>
          </div>
        </div>

        <div className="topbar-actions">
          <button className={`btn-sm${showAnnotations ? ' active' : ''}`} onClick={() => { setShowAnnotations(!showAnnotations); if (showAnnotations) setShowFlyLines(false); }} title="区块标注">🏷️ 标注</button>
          <button className={`btn-sm${showFlyLines ? ' active' : ''}`} onClick={() => setShowFlyLines(!showFlyLines)} disabled={!showAnnotations} title="飞线连">🔗 飞线</button>
          <input
            className="page-jump-input"
            type="number" min="1"
            placeholder="页码"
            value={pageInput}
            onChange={(e) => setPageInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                const p = parseInt(pageInput, 10);
                if (p > 0) {
                  pdfIframeRef.current?.contentWindow?.postMessage({ type: "navigate", page: p }, "*");
                }
              }
            }}
          />
            <button className="btn-sm" onClick={handleImportDocument} title="追加导入">📥 导入</button>
        </div>
        <div className="topbar-right">
          <span className="topbar-status">{statusMsg}</span>
          <button
            className="btn-sm topbar-theme-btn"
            onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
            title={theme === "dark" ? "切换浅色模式" : "切换深色模式"}
          >
            {theme === "dark" ? <Sun className="w-4 h-4" /> : <Moon className="w-4 h-4" />}
          </button>
        </div>
      </header>
      {/* ======== 主体区域：可拖拽面板布局 ======== */}
      <Group orientation="vertical" style={{ flex: 1, overflow: "hidden" }}>
        {/* 中间区域：左 + 中 + 右 */}
        <Panel defaultSize="78%" minSize="50%">
          <Group orientation="horizontal" style={{ overflow: "hidden" }}>
            {/* 左栏：TOC + 文件资产，可调宽度 */}
            <Panel defaultSize="18%" minSize={200} maxSize="30%" className="panel-left">
              <div className="sidebar-panel">
                <div className="sidebar-header">
                  <span>语义目录 ({tocTree.reduce((s, n) => s + countNodes(n), 0)})</span>
                </div>
                <div className="sidebar-content" style={{ flex: 3 }}>
                  <TOC nodes={tocTree} onSelect={handleSelectBlock} />
                </div>
                <div className="sidebar-content" style={{ flex: 1, borderTop: "1px solid oklch(var(--border))" }}>
                  <FileExplorer projectPath={projectPath} />
                </div>
              </div>
            </Panel>

            <Separator className="resize-handle resize-handle-h" />

            {/* 中栏工作区：PDF | Blocks列表 | Markdown */}
            <Panel defaultSize="62%">
              <div className="workspace-area" ref={workspaceRef} style={{ position: "relative" }}>
                <div className="workspace-col">
                  <div className="workspace-pane">
                    <div className="workspace-pane-header">PDF 视图</div>
                    <div className="workspace-pane-body">
                      <PdfViewer ref={pdfIframeRef} key={projectKey} projectPath={projectPath} onPageChange={handlePageChange} mirrorBboxes={mirrorBboxes} pageRect={pageRect} showAnnotations={showAnnotations} selectedBboxId={selectedBboxId} hoveredBboxId={hoveredBboxId} onBboxClick={setSelectedBboxId} />
                    </div>
                  </div>
                </div>
                <div className="workspace-col">
                  <div className="workspace-pane">
                    <div className="workspace-pane-header">内容编辑</div>
                    <div className="workspace-pane-body">
                      <BlockEditor block={activeBlock} pageBlocks={pageBlocks} onChange={handleContentChange} currentPage={displayPage}
                        onBlockToggle={() => { requestAnimationFrame(() => drawLinesRef.current?.()); }}
                        onHoverBlock={(b) => {
                          if (b && b.block_type !== 'empty' && b.content.trim()) {
                            setHoveredBboxId(b.id);
                          } else {
                            setHoveredBboxId(null);
                          }
                        }}
                      />
                    </div>
                  </div>
                </div>
                <div className="workspace-col">
                  <div className="workspace-pane">
                    <div className="workspace-pane-header">Markdown 预览</div>
                    <div className="workspace-pane-body">
                      <div className="md-preview-panel">
                        <MarkdownPreview blocks={pageBlocks} activeBlock={activeBlock} projectPath={projectPath} projectName={projectName} />
                      </div>
                    </div>
                  </div>
                </div>

                {/* 连线信息层 */}
                {showFlyLines && showAnnotations && <LinesLayer lines={lines} />}
              </div>
            </Panel>

            {/* 右栏：流程管线 + 智能对话，固定宽度 */}
            <div className="sidebar-panel sidebar-tools" style={{ width: 280 }}>
              <div className="sidebar-section">
                <div className="sidebar-header">流程管线</div>
                <div className="sidebar-content">
                  <PipelineStatus blocksTotal={tocTree.reduce((s, n) => s + countNodes(n), 0)} currentStage={importProgress?.stage} />
                </div>
              </div>
              <div className="sidebar-divider" />
              <div className="sidebar-section">
                <div className="sidebar-header">智能对话</div>
                <div className="sidebar-content">
                  <AgentConsole />
                </div>
              </div>
            </div>
          </Group>
        </Panel>

        <Separator className="resize-handle resize-handle-v" />

        {/* 底栏：处理日志，可调高度 */}
        <Panel defaultSize="15%" minSize="6%" maxSize="35%" className="panel-bottom">
          <div className="bottom-panel">
            <LogPanel externalLogs={importLogs} />
          </div>
        </Panel>
      </Group>

      {/* ======== 导入进度遮罩 ======== */}
      {importProgress && (
        <div className="import-overlay">
          <div className="import-card">
            <div className="import-stage">{importProgress.stage}</div>
            <div className="import-bar-wrap">
              <div className="import-bar" style={{ width: `${importProgress.percent}%` }} />
            </div>
            <div className="import-detail">{importProgress.detail}</div>
            <div className="import-stage-steps">
              {importStages.map((label, idx) => {
                const currentIndex = importProgress ? importStages.indexOf(importProgress.stage) : -1;
                const state = currentIndex === -1
                  ? "pending"
                  : idx < currentIndex
                    ? "done"
                    : idx === currentIndex
                      ? "active"
                      : "pending";
                return (
                  <div key={label} className={`import-stage-step ${state}`}>
                    <span className="step-dot" />
                    <span>{label}</span>
                  </div>
                );
              })}
            </div>
            <div className="import-pct">{importProgress.percent}%</div>
            {importLogs.length > 0 && (
              <div className="import-logs">
                {importLogs.map((log, i) => (
                  <div key={i} className="import-log-line">{log}</div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
