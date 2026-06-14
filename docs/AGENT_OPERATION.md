# Agent 操作指南 — NarrativeStructure GUI 自动化

## 架构概述

Agent Proxy v2 使用**前端主动轮询**架构，绕过 Tauri 安全限制（注入的 JS 无法访问 `window.__TAURI__`）。

### 通信流程

```
外部 (Python/CLI/MCP)
    ↓ 写入 JSON 到 /tmp/narrative-agent-queue.json
前端 (每 500ms 轮询)
    ↓ invoke('agent_poll_queue', {}) 读取并清空队列
前端 (安全 JS 上下文)
    ↓ 执行命令，访问 window.__TAURI__、pageControllerBridge 等
    ↓ invoke('eval_result_read', {result: ...}) 写回结果
结果文件 /tmp/narrative-eval-result.txt
    ↓ 外部读取结果
```

### 关键文件

| 文件 | 用途 |
|------|------|
| `/tmp/narrative-agent-queue.json` | 命令输入（外部写入） |
| `/tmp/narrative-eval-result.txt` | 结果输出（前端写入） |

## 命令格式

### 输入 (写入 /tmp/narrative-agent-queue.json)

```json
{"command": "<命令名>", "params": {...}}
```

### 输出 (读取 /tmp/narrative-eval-result.txt)

```json
{"type": "agent-result", "result": {...}}
{"type": "agent-error", "error": "..."}
```

## 可用命令

| 命令 | 参数 | 说明 |
|------|------|------|
| `getState` | 无 | 获取当前页面状态（title, content, footer, header） |
| `openProject` | `{path, name?}` | 打开指定项目 |
| `closeProject` | 无 | 关闭当前项目，返回欢迎页 |
| `getProjectPath` | 无 | 获取当前项目路径 |
| `getProjectName` | 无 | 获取当前项目名称 |
| `navigateToPage` | `{page}` | 跳转到指定页码 |
| `getPage` | 无 | 获取当前页码 |
| `getText` | `{selector?}` | 获取页面/DOM 文本 |
| `getHtml` | `{selector?}` | 获取页面/DOM HTML |
| `click` | `{selector}` | 模拟点击元素 |
| `fill` | `{selector, value}` | 填写输入框 |
| `scroll` | `{direction?, pixels?}` | 滚动页面 |
| `screenshot` | 无 | 截图（备用，推荐 MCP takeScreenshot） |
| `eval` | `{script}` | 执行自定义 JavaScript |
| `importDocument` | `{zip_path}` | 导入 MinerU zip 包 |

## Python 调用示例

```python
import time
import json
import os

QUEUE = "/tmp/narrative-agent-queue.json"
RESULT = "/tmp/narrative-eval-result.txt"

def agent_call(command: str, params: dict = None, timeout: int = 15) -> dict:
    """通过 Agent Proxy 执行命令"""
    if os.path.exists(RESULT):
        os.remove(RESULT)
    cmd = {"command": command, "params": params or {}}
    with open(QUEUE, 'w') as f:
        f.write(json.dumps(cmd))
    for _ in range(timeout * 10):
        time.sleep(0.1)
        if os.path.exists(RESULT):
            with open(RESULT) as f:
                content = f.read().strip()
                if content:
                    data = json.loads(content)
                    if data.get('type') == 'agent-result':
                        return data.get('result', {})
                    elif data.get('type') == 'agent-error':
                        return {"error": data.get('error', 'unknown')}
                    return data
    return {"error": f"timeout ({timeout}s)"}

# 使用示例
# 1. 获取当前状态
result = agent_call('getState')
print(result.get('title', 'N/A'))

# 2. 打开项目
result = agent_call('openProject', {'path': '/path/to/project'}, timeout=20)
time.sleep(3)  # 等待项目加载

# 3. 翻页
result = agent_call('navigateToPage', {'page': 5})

# 4. 获取页面文本
result = agent_call('getText')
print(result.get('text', '')[:200])

# 5. 自定义 eval
result = agent_call('eval', {'script': 'document.title'})
```

## 注意事项

1. **命令串行执行**：前一个命令执行完（写回结果）后，才能发送下一个命令
2. **超时保护**：命令执行超过 30 秒会自动重置
3. **等待加载**：`openProject` 后建议 `sleep(3-5)` 等待 PDF 渲染
4. **翻页缓冲**：翻页使用 9 页窗口策略，中间 ±4 页直接切换无需重载

## 完整演示脚本

参见 `/tmp/test_gui_demo.py` 或运行：

```bash
python3 /tmp/test_gui_demo.py
```

预期输出：
```
============================================================
GUI 操作演示 (Agent Proxy v2)
============================================================

--- Step 1: 获取欢迎页状态 ---
Title: NarrativeStructure — 文档智能化重构工作台
✅ 欢迎页确认

--- Step 2: 打开项目 ---
✅ 项目打开: True

--- Step 3: 获取项目路径 ---
✅ 项目路径: /Users/.../Projects/xxx

--- Step 4: 获取项目名称 ---
✅ 项目名称: xxx

--- Step 5: 翻到第 3 页 ---
✅ 翻页成功

--- Step 6: 获取页面文本 ---
✅ 文本长度: XXXX 字符
   前200字符: ...

--- Step 7: 翻回第 1 页 ---
✅ 翻页成功

============================================================
✅ 演示完成! Agent Proxy v2 工作正常
============================================================