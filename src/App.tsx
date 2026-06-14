import { useState, useCallback, useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
// PageController Bridge — 初始化 Page Agent 的 DOM 操作能力（不需要 LLM）
import "./lib/pageControllerBridge";
// Agent Proxy — 通过 postMessage 与外部通信，绕过 Tauri 安全限制
import { setupAgentProxy } from "./lib/agentProxy";

// Agent Proxy v2: 前端主动轮询 agent_poll_queue，在安全上下文中执行命令
// 不再需要 useEvalQueue - setupAgentProxy 内部处理轮询
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

// =========================================================================
// 全局截图函数 — 供 MCP / 外部调用
// =========================================================================
// 注意：html2canvas v1.x 不支持 Tailwind CSS 3+ 使用的 oklch() 颜色函数
// 在 Tauri 桌面应用中，请使用 MCP 的 takeScreenshot 工具（Playwright 原生截图）
// 此函数仅作为备用方案，在纯 RGB 颜色的页面上可用
async function takeScreenshot(): Promise<string> {
  try {
    // 动态导入 html2canvas（避免影响主 bundle）
    const html2canvasModule = await import('html2canvas');
    const html2canvas = html2canvasModule.default;
    
    const canvas = await html2canvas(document.body, {
      useCORS: true,
      backgroundColor: '#ffffff',
      scale: window.devicePixelRatio || 1,
      logging: false,
    });
    
    const dataUrl = canvas.toDataURL('image/png');
    const base64 = dataUrl.split(',')[1];
    
    // 调用后端保存文件
    try {
      await invoke('save_screenshot', { base64 });
    } catch {}
    
    return base64;
  } catch (e: any) {
    // oklch 颜色会导致 html2canvas 失败
    // 提示用户使用 MCP 工具的 Playwright 原生截图
    throw new Error(
      '截图失败: ' + e.message +
      '. 由于本应用使用 Tailwind CSS oklch 颜色，html2canvas 无法解析。' +
      '请使用 MCP 工具的 takeScreenshot 命令进行截图。'
    );
  }
}

// 挂载全局函数
if (typeof window !== 'undefined') {
  (window as any).screenshot = takeScreenshot;
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
  const pageReqIdRef = useRef(0);
  const loadedCenterRef = useRef(0); // 已加载数据的中心页码
  const pageBlocksRef = useRef<Block[] | null>(null);
  const [displayPage, setDisplayPage] = useState(1); // 触发 UI 重渲染
  const [pageInput, setPageInput] = useState("");

  // Agent Proxy v2: setupAgentProxy 内部处理轮询，无需 useEvalQueue
  
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
      document.querySelectorAll('[data-block-id]').forEach(el => {
        const id = el.getAttribute('data-block-id')!;
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
    // 提取当前页文本 → 请求 bbox (react-pdf 模式：不再需要 iframe postMessage)
      const requestBboxForCurrentPage = () => {
        const blocks = pageBlocksRef.current;
        if (!blocks?.length) return;
        const page = currentPageRef.current;
      const pageTexts = blocks
        .filter(b => { try { return (JSON.parse(b.metadata||'{}').page||0) === page; } catch { return false; } })
        .filter(b => b.block_type !== 'empty' && b.content.trim())
        .map(b => b.content);
      pageTextsRef.current = pageTexts;
      if (!pageTexts.length) return;
      const reqId = ++bboxRequestIdRef.current;
      (window as any).__flyRows = blocks.filter(b => pageTexts.includes(b.content)).map(b => ({ id: b.id, content: b.content }));
      (window as any).__flyReqId = reqId;
      // TODO: react-pdf 模式下 bbox 定位需要重新实现
    };
    requestBboxRef.current = requestBboxForCurrentPage;
    let rafId = 0;
    const handler = (e: MessageEvent) => {
      if (e.data?.type === 'pdf-scroll-offset') {
        cancelAnimationFrame(rafId); rafId = requestAnimationFrame(drawAllLines);
        return;
      }
      const reqId = (window as any).__flyReqId;
      if (reqId !== undefined && reqId !== bboxRequestIdRef.current) return;
      if (e.data?.type !== 'bbox-pos' || !e.data.bboxes?.length) return;
      const rows: any[] = (window as any).__flyRows || [];
      const bboxes = e.data.bboxes.map((bb: any, bi: number) => ({ x: bb.x, y: bb.y, w: bb.w, h: bb.h, id: rows[bi]?.id || "" })).filter((bb: MirrorBbox) => bb.id);
      if (e.data.pageRect) setPageRect(e.data.pageRect);
      setMirrorBboxes(bboxes);
      requestAnimationFrame(drawAllLines);
    };
    window.addEventListener('message', handler);
    let scrollEl: Element | null = null;
    const onBlockScroll = () => { cancelAnimationFrame(rafId); rafId = requestAnimationFrame(drawAllLines); };
        // 注意：新的 react-pdf 查看器不再需要 iframe postMessage 通信
        const timer = setInterval(() => { if (!scrollEl) { scrollEl = document.querySelector('.page-blocks-list'); if (scrollEl) scrollEl.addEventListener('scroll', onBlockScroll, { passive: true }); } }, 500);
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

  // PDF 翻页：缓冲区策略 — 中间5页直接用，超出则重载21页
  const handlePageChange = useCallback(async (page: number) => {
    const center = loadedCenterRef.current;
    // 缓冲区命中：当前页在已加载数据的中间5页内 → 只更新引用 + 滚动定位 + 刷新 bbox
    if (center > 0 && pageBlocksRef.current && page >= center - 3 && page <= center + 3) {
      currentPageRef.current = page;
      setDisplayPage(page);
      setMirrorBboxes([]); setLines([]); setPageRect(null);
      requestBboxRef.current?.();
      setImportLogs(prev => [...prev.slice(-19), `📖 翻到 p${page} → 命中缓冲区 (center=p${center})`]);
      requestAnimationFrame(() => {
        document.querySelector('.page-group-active')?.scrollIntoView({ block: 'start', behavior: 'smooth' });
      });
      return;
    }
    // 缓冲区未命中或无数据 → 重新加载
    const reqId = ++pageReqIdRef.current;
    try {
      const pageStart = Math.max(1, page - 4);
      const pageEnd = pageStart + 8; // 9 页窗口（当前 ±4）
      const blocks = await invoke<Block[]>("get_blocks_by_page", { pageStart, pageEnd });
      if (reqId !== pageReqIdRef.current) return;
      const loadedPages = new Set<number>();
      for (const b of blocks) {
        try {
          const p = JSON.parse(b.metadata || "{}").page || 0;
          if (p > 0) loadedPages.add(p);
        } catch {}
      }
      setImportLogs(prev => [...prev.slice(-19), `📖 翻到 p${page} → 请求 p${pageStart}-p${pageEnd}（9页窗口），实际 ${loadedPages.size} 页/${blocks.length} 行`]);
      const filledBlocks = fillPageWindow(blocks, pageStart, pageEnd);
      loadedCenterRef.current = page;
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
        loadedCenterRef.current = page;
        currentPageRef.current = page;
        setDisplayPage(page);
        if (blocks.length > 0) {
          pageBlocksRef.current = blocks;
          setPageBlocks(blocks);
        }
      } catch {}
    }
  }, [fillPageWindow]);

  // 暴露到全局 window，供外部 Agent 通过 eval_queue 调用
  useEffect(() => {
    (window as any).nsOpenProject = (path: string, name?: string) => loadProject(path, name);
    (window as any).nsCloseProject = () => handleCloseProject();
    (window as any).nsNavigateToPage = (page: number) => handlePageChange(page);
    (window as any).nsGetProjectPath = () => projectPath;
    (window as any).nsGetProjectName = () => projectName;
  }, [loadProject, handleCloseProject, handlePageChange, projectPath, projectName]);

  // 初始化 Agent Proxy (postMessage 通信)
  useEffect(() => {
    setupAgentProxy();
  }, []);

  // BlockEditor 和 MarkdownPreview 直接使用 pageBlocks（完整窗口）
  // 缓冲区命中逻辑（中间5页）仅在 handlePageChange 中用于判断是否重载
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
        currentPageRef.current = targetPage;
        setDisplayPage(targetPage);
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
                  handlePageChange(p);
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
                      <PdfViewer key={projectKey} projectPath={projectPath} onPageChange={handlePageChange} mirrorBboxes={mirrorBboxes} pageRect={pageRect} showAnnotations={showAnnotations} />
                    </div>
                  </div>
                </div>
                <div className="workspace-col">
                  <div className="workspace-pane">
                    <div className="workspace-pane-header">内容编辑</div>
                    <div className="workspace-pane-body">
                      <BlockEditor block={activeBlock} pageBlocks={pageBlocks} onChange={handleContentChange} currentPage={displayPage}
                        onBlockToggle={() => { requestAnimationFrame(() => drawLinesRef.current?.()); }}
                        onHoverBlock={(_b) => {
                          // TODO: react-pdf 模式下 bbox highlight 需要重新实现
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
                  <div className="sidebar-header">MCP 工具</div>
                  <div className="sidebar-content">
                    <AgentConsole
                      projectPath={projectPath}
                      projectName={projectName}
                      blocksTotal={tocTree.reduce((s, n) => s + countNodes(n), 0)}
                      onOpenProject={(path, name) => loadProject(path, name)}
                    />
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
