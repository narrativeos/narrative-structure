# NarrativeStructure Agent 操控最佳实践

> 本文档记录了对 NarrativeStructure GUI 应用进行外部自动化操控的最佳实践方案。

---

## 1. 架构概述

### 核心问题

Tauri 的安全模型禁止通过 `window.eval()` 注入的 JavaScript 访问 `window.__TAURI__` API。因此，**外部进程（Python/CLI/MCP）无法直接向前端注入代码来调用 Tauri 命令**。

### 解决方案：前端主动轮询

```
┌─────────────────────────────────────────────────────────────────┐
│                     NarrativeStructure App                      │
│                                                                 │
│  ┌──────────┐    每500ms轮询     ┌──────────────────┐          │
│  │  外部    │ ──────────────→   │  前端 (React)     │          │
│  │  Python  │  写入 JSON         │                   │          │
│  │ / CLI /  │                    │  agentProxy.ts   │          │
│  │   MCP    │                     │  - 轮询队列     │          │
│  │          │                     │  - 执行命令     │          │
│  │          │  ←──────────────   │  - 写回结果     │          │
│  │          │    读取结果         └──────────────────┘          │
│  │          │                      ↑ invoke()                   │
│  └──────────┴───────────────────── └──────┬───────────────────┘
│                                           │
│                              ┌────────────┴───────────────────┐  │
│                              │    Rust (Tauri Backend)        │  │
│                              │   agent_poll_queue()           │  │
│                              │   eval_result_read()           │  │
│                              └────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘

文件通道:
  /tmp/narrative-agent-queue.json    ← 外部写入命令
  /tmp/narrative-eval-result.txt     ← 前端写回结果
```

### 关键设计决策

| 决策 | 说明 |
|------|------|
| **前端主动轮询** | 前端每 500ms 调用 `invoke('agent_poll_queue')` 读取命令 |
| **文件作为 IPC** | 使用 `/tmp/` 下的 JSON 文件作为命令队列和结果通道 |
| **串行执行** | 同一时间只执行一个命令，防止并发冲突 |
| **超时保护** | 单个命令执行超过 30 秒自动重置 |

---

## 2. 通信协议

### 命令格式（写入）

**文件**: `/tmp/narrative-agent-queue.json`

```json
{"command": "<命令名>", "params": {"key": "value"}}
```

### 结果格式（读取）

**文件**: `/tmp/narrative-eval-result.txt`

```json
{"type": "agent-result", "result": {...}}
```

或

```json
{"type": "agent-error", "error": "错误信息"}
```

---

## 3. 可用命令

### 3.1 页面状态

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `getState` | 无 | `{title, content, footer, header}` | 获取当前页面完整状态 |
| `getPage` | 无 | `{page: number}` | 获取当前 PDF 页码 |
| `getText` | `{selector?}` | `{text}` | 获取页面/DOM 文本 |
| `getHtml` | `{selector?}` | `{html}` | 获取页面/DOM HTML |

### 3.2 项目操作

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `openProject` | `{path, name?}` | `{success, path}` | 打开指定项目 |
| `closeProject` | 无 | `string` | 关闭当前项目 |
| `getProjectPath` | 无 | `string \| null` | 获取当前项目路径 |
| `getProjectName` | 无 | `string` | 获取当前项目名称 |

### 3.3 PDF 导航

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `navigateToPage` | `{page}` | `{success, page}` | 跳转到指定页码 |

### 3.4 UI 交互

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `click` | `{selector}` | `{success}` | 模拟点击元素 |
| `fill` | `{selector, value}` | `{success}` | 填写输入框 |
| `scroll` | `{direction?, pixels?}` | `{success}` | 滚动页面 |

### 3.5 高级

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `eval` | `{script}` | 任意 | 执行自定义 JavaScript |
| `screenshot` | 无 | `base64` | 截图（备用） |

---

## 4. Python 客户端

### 4.1 核心函数

```python
import time
import json
import os

QUEUE  = "/tmp/narrative-agent-queue.json"
RESULT = "/tmp/narrative-eval-result.txt"

def agent_call(command: str, params: dict = None, timeout: int = 15) -> dict:
    """
    通过 Agent Proxy 执行一个命令
    
    :param command: 命令名，如 'getState', 'openProject', 'navigateToPage'
    :param params: 命令参数，如 {'path': '/path/to/project'}
    :param timeout: 超时秒数，默认 15
    :return: 结果字典，成功时包含 result，失败时包含 error
    """
    # 1. 清理旧结果
    if os.path.exists(RESULT):
        os.remove(RESULT)
    
    # 2. 写入命令
    cmd = {"command": command, "params": params or {}}
    with open(QUEUE, 'w') as f:
        f.write(json.dumps(cmd))
    
    # 3. 轮询等待结果
    for _ in range(timeout * 10):
        time.sleep(0.1)  # 每 100ms 检查一次
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
```

### 4.2 使用示例

```python
# ===== 基本操作 =====

# 1. 检查当前页面状态
result = agent_call('getState')
print(f"当前页面: {result.get('title', 'N/A')[:60]}")

# 2. 打开项目（需要较长超时）
result = agent_call('openProject', {
    'path': '/Users/xxx/.narrativeos/narrative-structure/Projects/2024-01-01',
    'name': '我的项目'
}, timeout=30)
if 'error' in result:
    print(f"打开失败: {result['error']}")
else:
    time.sleep(5)  # ⚠️ 等待项目完全加载（PDF 渲染等）

# 3. 翻页
agent_call('navigateToPage', {'page': 10}, timeout=20)
time.sleep(2)  # 等待页面渲染

# 4. 获取页面文本
result = agent_call('getText')
text = result.get('text', '')
print(f"文本长度: {len(text)} 字符")
print(f"前200字符: {text[:200]}")

# 5. 获取项目信息
print(f"项目路径: {agent_call('getProjectPath')}")
print(f"项目名称: {agent_call('getProjectName')}")

# 6. 关闭项目
agent_call('closeProject')
```

---

## 5. 操作工作流

### 5.1 标准操作流程

```
启动 App
    ↓
getState()  → 确认在欢迎页
    ↓
openProject(path)  → 打开项目
    ↓
sleep(3-5s)  → ⚠️ 等待项目完全加载
    ↓
getProjectName()  → 确认项目已打开
    ↓
navigateToPage(n)  → 翻页
    ↓
sleep(1-2s)  → 等待 PDF 渲染
    ↓
getText()  → 获取页面内容
    ↓
... 重复翻页/获取 ...
    ↓
closeProject()  → 关闭项目
```

### 5.2 超时建议

| 操作 | 建议超时 | 说明 |
|------|---------|------|
| `getState` | 5s | 快速操作 |
| `openProject` | **30s** | DB 连接 + TOC 加载 + React 渲染 |
| `navigateToPage` | 15s | PDF 页面加载 |
| `getText` / `getHtml` | 10s | DOM 操作 |
| `eval` | 10s | 取决于脚本复杂度 |

### 5.3 等待策略

```python
# ❌ 错误：不等待直接操作
agent_call('openProject', {'path': '/path'})
agent_call('navigateToPage', {'page': 5})  # 可能失败！

# ✅ 正确：操作后等待
agent_call('openProject', {'path': '/path'}, timeout=30)
time.sleep(5)  # 等待项目完全加载
agent_call('navigateToPage', {'page': 5})
time.sleep(2)  # 等待 PDF 渲染
agent_call('getText')
```

---

## 6. 翻页策略

### 6.1 缓冲区机制

App 内部使用 **9 页窗口**策略（当前页 ±4）：
- 翻页在缓冲区范围内（±3 页）→ 直接切换，无需重载
- 翻页超出缓冲区 → 重新请求 9 页数据

### 6.2 推荐翻页方式

```python
# 顺序翻页（推荐）
for page in range(1, 51):
    agent_call('navigateToPage', {'page': page})
    time.sleep(1)  # 每页等待 1 秒
    result = agent_call('getText')
    # 处理文本...

# 随机翻页（需要更长等待）
for page in [1, 50, 10, 30, 5]:
    agent_call('navigateToPage', {'page': page}, timeout=20)
    time.sleep(3)  # 跨页跳转需要更长时间
    result = agent_call('getText')
```

---

## 7. MCP 集成

### 7.1 通过 MCP 调用

MCP Server 可以直接写入队列文件，等同于 Python 客户端：

```python
# MCP tool 实现示例
def navigate_to_page(page: int) -> str:
    """翻页工具 - 通过 Agent Proxy 执行"""
    cmd = {"command": "navigateToPage", "params": {"page": page}}
    with open("/tmp/narrative-agent-queue.json", "w") as f:
        f.write(json.dumps(cmd))
    # 等待结果...
```

### 7.2 MCP tools.yaml 配置

参见 `narrative-structure/src-tauri/tools.yaml`，定义了 MCP Server 暴露的工具。

---

## 8. 故障排查

### 8.1 常见问题

| 问题 | 原因 | 解决方案 |
|------|------|---------|
| 命令超时 | App 未启动或未响应 | 确认 App 窗口已打开 |
| `pageControllerBridge not available` | 在欢迎页调用项目相关命令 | 先 `getState` 确认页面状态 |
| 翻页后文本为空 | 等待时间不足 | 增加 `sleep(2-3s)` |
| `openProject` 超时 | 项目很大或 DB 慢 | 增加 `timeout=30` |

### 8.2 调试方法

```python
# 1. 检查队列文件
cat /tmp/narrative-agent-queue.json

# 2. 检查结果文件
cat /tmp/narrative-eval-result.txt

# 3. 检查 App 日志
tail -50 /tmp/narrative-tauri-dev-new.log

# 4. 用 eval 调试
result = agent_call('eval', {'script': 'window.__AGENT_PROXY__ ? "OK" : "NOT_FOUND"'})
print(result)
```

---

## 9. 完整示例脚本

```python
#!/usr/bin/env python3
"""NarrativeStructure Agent 操控完整示例"""
import time
import json
import os

QUEUE  = "/tmp/narrative-agent-queue.json"
RESULT = "/tmp/narrative-eval-result.txt"

def agent_call(command, params=None, timeout=15):
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

def main():
    PROJECT_PATH = "/Users/xxx/.narrativeos/narrative-structure/Projects/2024-01-01"
    
    # Step 1: 确认欢迎页
    print("=== 确认欢迎页 ===")
    state = agent_call('getState')
    print(f"Title: {state.get('title', 'N/A')[:60]}")
    
    # Step 2: 打开项目
    print("\n=== 打开项目 ===")
    result = agent_call('openProject', {'path': PROJECT_PATH}, timeout=30)
    if 'error' in result:
        print(f"ERROR: {result['error']}")
        return
    print(f"✅ 项目打开")
    time.sleep(5)  # ⚠️ 重要：等待项目加载
    
    # Step 3: 确认项目
    print("\n=== 确认项目 ===")
    name = agent_call('getProjectName')
    print(f"项目名称: {name}")
    
    # Step 4: 逐页提取文本
    print("\n=== 提取文本 ===")
    for page in range(1, 6):  # 前 5 页
        agent_call('navigateToPage', {'page': page}, timeout=15)
        time.sleep(2)
        result = agent_call('getText')
        text = result.get('text', '') if isinstance(result, dict) else ''
        print(f"  p{page}: {len(text)} 字符")
    
    # Step 5: 关闭项目
    print("\n=== 关闭项目 ===")
    agent_call('closeProject')
    print("✅ 完成")

if __name__ == "__main__":
    main()
```

---

## 10. 相关文件

| 文件 | 说明 |
|------|------|
| `src/lib/agentProxy.ts` | 前端 Agent 代理实现 |
| `src-tauri/src/project_manager.rs` | Rust 后端命令（`agent_poll_queue`, `eval_result_read`） |
| `src-tauri/src/lib.rs` | Tauri 命令注册 |
| `src/App.tsx` | App 入口，初始化 `setupAgentProxy()` |
| `docs/AGENT_OPERATION.md` | 命令参考文档 |
| `src/lib/pageControllerBridge.ts` | 页面控制器桥接 |

---

*最后更新: 2026-06-14*