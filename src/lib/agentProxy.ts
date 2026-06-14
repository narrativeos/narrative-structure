/**
 * Agent Proxy - 前端 Agent 通信代理 v2
 *
 * 架构：前端主动轮询命令文件，在执行上下文中调用 Tauri 命令
 * 
 * 流程：
 * 1. 外部写入 JSON 到 /tmp/narrative-agent-queue.json
 * 2. 前端轮询该文件（通过 Tauri invoke）
 * 3. 前端在安全的 JS 上下文中执行命令
 * 4. 结果通过 Tauri invoke 写回 /tmp/narrative-eval-result.txt
 */
import { invoke } from '@tauri-apps/api/core';

/**
 * 设置 Agent Proxy - 前端轮询方式
 */
export function setupAgentProxy(): void {
  const writeResult = async (data: any) => {
    try {
      await invoke('eval_result_read', { result: JSON.stringify(data) });
    } catch (e) {
      console.error('[AgentProxy] write result failed:', e);
    }
  };
  
  const executeCommand = async (cmd: string, params: Record<string, any>) => {
    try {
      switch (cmd) {
        case 'getState': {
          const state = (window as any).pageControllerBridge?.getState();
          if (state) {
            await writeResult({ type: 'agent-result', result: await state });
          } else {
            await writeResult({ type: 'agent-error', error: 'pageControllerBridge not available' });
          }
          break;
        }
        case 'openProject': {
          const { path, name } = params;
          if (!path) {
            await writeResult({ type: 'agent-error', error: 'path is required' });
            break;
          }
          try {
            await (window as any).nsOpenProject(path, name || path.split('/').pop());
            await writeResult({ type: 'agent-result', result: { success: true, path } });
          } catch (e: any) {
            await writeResult({ type: 'agent-error', error: e.message || String(e) });
          }
          break;
        }
        case 'closeProject': {
          const result = await (window as any).nsCloseProject();
          await writeResult({ type: 'agent-result', result });
          break;
        }
        case 'getProjectPath': {
          await writeResult({ type: 'agent-result', result: (window as any).nsGetProjectPath?.() });
          break;
        }
        case 'getProjectName': {
          await writeResult({ type: 'agent-result', result: (window as any).nsGetProjectName?.() });
          break;
        }
        case 'navigateToPage': {
          const { page } = params;
          if (page == null) {
            await writeResult({ type: 'agent-error', error: 'page number is required' });
          } else {
            await (window as any).nsNavigateToPage(page);
            await writeResult({ type: 'agent-result', result: { success: true, page } });
          }
          break;
        }
        case 'getPage': {
          const page = (window as any).pageControllerBridge?.getCurrentPage?.();
          await writeResult({ type: 'agent-result', result: { page } });
          break;
        }
        case 'screenshot': {
          if ((window as any).screenshot) {
            await writeResult({ type: 'agent-result', result: await (window as any).screenshot() });
          } else {
            await writeResult({ type: 'agent-error', error: 'screenshot not available' });
          }
          break;
        }
        case 'eval': {
          const { script } = params;
          if (!script) {
            await writeResult({ type: 'agent-error', error: 'script is required' });
            break;
          }
          try {
            const fn = new Function(script);
            const result = fn();
            if (result && typeof result.then === 'function') {
              await writeResult({ type: 'agent-result', result: await result });
            } else {
              await writeResult({ type: 'agent-result', result });
            }
          } catch (e: any) {
            await writeResult({ type: 'agent-error', error: e.message || String(e) });
          }
          break;
        }
        case 'getText': {
          const { selector } = params;
          const text = selector
            ? (document.querySelector(selector) as HTMLElement | null)?.innerText || ''
            : document.body?.innerText || '';
          await writeResult({ type: 'agent-result', result: { text: text.substring(0, 50000) } });
          break;
        }
        case 'getHtml': {
          const { selector } = params;
          const html = selector
            ? (document.querySelector(selector) as HTMLElement | null)?.outerHTML || ''
            : document.body?.outerHTML || '';
          await writeResult({ type: 'agent-result', result: { html: html.substring(0, 100000) } });
          break;
        }
        case 'click': {
          const { selector } = params;
          if (!selector) {
            await writeResult({ type: 'agent-error', error: 'selector is required' });
            break;
          }
          const el = document.querySelector(selector) as EventTarget | null;
          if (el) {
            el.dispatchEvent(new MouseEvent('click', { bubbles: true }));
            await writeResult({ type: 'agent-result', result: { success: true } });
          } else {
            await writeResult({ type: 'agent-error', error: `Element not found: ${selector}` });
          }
          break;
        }
        case 'fill': {
          const { selector, value } = params;
          if (!selector) {
            await writeResult({ type: 'agent-error', error: 'selector is required' });
            break;
          }
          const el = document.querySelector(selector) as HTMLInputElement | null;
          if (el) {
            el.value = value || '';
            el.dispatchEvent(new Event('input', { bubbles: true }));
            el.dispatchEvent(new Event('change', { bubbles: true }));
            await writeResult({ type: 'agent-result', result: { success: true } });
          } else {
            await writeResult({ type: 'agent-error', error: `Element not found: ${selector}` });
          }
          break;
        }
        case 'scroll': {
          const { direction = 'down', pixels = 300 } = params;
          window.scrollBy({ top: direction === 'down' ? pixels : -pixels, behavior: 'smooth' });
          await writeResult({ type: 'agent-result', result: { success: true } });
          break;
        }
        default:
          await writeResult({ type: 'agent-error', error: `Unknown command: ${cmd}` });
      }
    } catch (e: any) {
      await writeResult({ type: 'agent-error', error: e.message || String(e) });
    }
  };
  
  // 轮询命令队列
  let executing = false;
  let execStart = 0;
  const MAX_EXEC_TIME = 30000; // 30秒超时
  const poll = async () => {
    if (executing) {
      // 如果执行时间过长，强制重置
      if (Date.now() - execStart > MAX_EXEC_TIME) {
        console.warn('[AgentProxy] Command execution timeout, resetting');
        executing = false;
      } else {
        return;
      }
    }
    try {
      const result = (await invoke('agent_poll_queue', {})) as string;
      if (result !== 'empty' && result !== 'executed') {
        executing = true;
        execStart = Date.now();
        try {
          const cmd = JSON.parse(result);
          console.log('[AgentProxy] Executing command:', cmd.command);
          await executeCommand(cmd.command, cmd.params || {});
          console.log('[AgentProxy] Command completed:', cmd.command);
        } finally {
          executing = false;
        }
      }
    } catch (e) {
      console.error('[AgentProxy] Poll error:', e);
      executing = false;
    }
  };
  
  // 每 500ms 轮询一次
  const interval = setInterval(poll, 500);
  poll(); // 立即执行一次
  
  console.log('[AgentProxy] Agent proxy initialized (polling mode)');
}