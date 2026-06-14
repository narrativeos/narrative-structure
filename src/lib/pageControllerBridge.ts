/**
 * PageController Bridge — 暴露 Page Agent 的 DOM 操作能力给外部 MCP
 *
 * 使用 @page-agent/page-controller 的 PageController（不需要 LLM）
 * 外部 Agent 通过 MCP 调用以下接口来驱动 GUI：
 *   - page_get_state() → 获取简化 DOM
 *   - page_do_action() → 执行 click/fill/scroll/execute_js
 *   - page_screenshot() → 截图
 */

import { PageController, BrowserState } from '@page-agent/page-controller';

// ActionResult is not exported from page-controller, define it locally
interface ActionResult {
  success: boolean;
  message: string;
}

export interface PageAction {
  type: 'click' | 'fill' | 'scroll' | 'select' | 'execute_js';
  target?: string;  // index like "0", "1", etc.
  value?: string;   // text for fill/select, script for execute_js
  scrollDown?: boolean;
  pixels?: number;
}

export interface PageStateResult {
  success: boolean;
  state?: BrowserState;
  error?: string;
}

export interface PageActionResult {
  success: boolean;
  message?: string;
  error?: string;
}

// Singleton instance
let controller: PageController | null = null;

function getController(): PageController {
  if (!controller) {
    controller = new PageController({
      enableMask: false, // 我们不遮挡用户
      viewportExpansion: 0, // 只看当前视口
    });
  }
  return controller;
}

/**
 * 获取当前页面状态（简化 DOM）
 */
export async function pageGetState(): Promise<PageStateResult> {
  try {
    const ctrl = getController();
    const state = await ctrl.getBrowserState();
    return { success: true, state };
  } catch (e: any) {
    return { success: false, error: e.message || String(e) };
  }
}

/**
 * 执行页面操作
 */
export async function pageDoAction(action: PageAction): Promise<PageActionResult> {
  try {
    const ctrl = getController();

    // 确保 DOM 树已更新
    await ctrl.updateTree();

    switch (action.type) {
      case 'click': {
        const index = parseInt(action.target || '0', 10);
        const result: ActionResult = await ctrl.clickElement(index);
        return { success: result.success, message: result.message };
      }

      case 'fill': {
        const index = parseInt(action.target || '0', 10);
        const result: ActionResult = await ctrl.inputText(index, action.value || '');
        return { success: result.success, message: result.message };
      }

      case 'select': {
        const index = parseInt(action.target || '0', 10);
        const result: ActionResult = await ctrl.selectOption(index, action.value || '');
        return { success: result.success, message: result.message };
      }

      case 'scroll': {
        const down = action.scrollDown ?? true;
        const result: ActionResult = await ctrl.scroll({
          down,
          numPages: 1,
          pixels: action.pixels,
        });
        return { success: result.success, message: result.message };
      }

      case 'execute_js': {
        const result: ActionResult = await ctrl.executeJavascript(action.value || '');
        return { success: result.success, message: result.message };
      }

      default:
        return { success: false, error: `Unknown action type: ${action.type}` };
    }
  } catch (e: any) {
    return { success: false, error: e.message || String(e) };
  }
}

/**
 * 清理资源
 */
export function disposePageController(): void {
  if (controller) {
    controller.dispose();
    controller = null;
  }
}

// 暴露到全局供 eval-queue 调用
declare global {
  interface Window {
    pageControllerBridge: {
      getState: () => Promise<PageStateResult>;
      doAction: (action: PageAction) => Promise<PageActionResult>;
    };
  }
}

window.pageControllerBridge = {
  getState: pageGetState,
  doAction: pageDoAction,
};