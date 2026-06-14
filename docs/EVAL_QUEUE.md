# External JS Evaluation Queue

## Overview

The `eval_js_queue` mechanism allows external processes (like MCP servers, scripts, or AI agents) to inject JavaScript code into the running Tauri application for execution.

## How It Works

### Architecture

```
External Process                    Tauri App
┌──────────────────┐                ┌──────────────────────┐
│ Write JS to file │                │ Frontend polls every │
│ /tmp/narrative-  │───────────────▶│ 1 second via         │
│ eval-queue.txt   │                │ useEvalQueue() hook  │
└──────────────────┘                └──────────┬───────────┘
                                               │
                                               ▼
                                        ┌──────────────────┐
                                        │ eval_js_queue()  │
                                        │ Tauri command    │
                                        │ reads file, evals│
                                        │ JS, clears file  │
                                        └──────────────────┘
```

### Components

1. **File Queue**: `/tmp/narrative-eval-queue.txt`
   - External processes write JavaScript code to this file
   - One script per write (the file is cleared after execution)

2. **Backend** (`src-tauri/src/project_manager.rs`):
   - `eval_js_queue()` - Reads file, evaluates JS in webview, clears file
   - Returns `"executed"` if a script was found and run
   - Returns `"empty"` if no script was pending

3. **Frontend** (`src/App.tsx`):
   - `useEvalQueue()` hook polls `eval_js_queue` every 1000ms
   - Runs automatically when the app starts

## Usage

### From Command Line

```bash
# Open a project
echo 'window.__TAURI__ && window.__TAURI__.core.invoke("open_project", {path: "/path/to/project"});' > /tmp/narrative-eval-queue.txt

# Close current project
echo 'window.__TAURI__ && window.__TAURI__.core.invoke("close_project");' > /tmp/narrative-eval-queue.txt

# Navigate to a specific page
echo 'window.__TAURI__ && window.__TAURI__.core.invoke("navigate_to_page", {page: 5});' > /tmp/narrative-eval-queue.txt

# Get project info
echo 'window.__TAURI__ && window.__TAURI__.core.invoke("get_project_path").then(r => console.log(r));' > /tmp/narrative-eval-queue.txt
```

### From Python

```python
import os

def eval_js(script: str):
    with open('/tmp/narrative-eval-queue.txt', 'w') as f:
        f.write(script)

# Open a project
eval_js('window.__TAURI__ && window.__TAURI__.core.invoke("open_project", {path: "/path/to/project"});')

# Import a document
eval_js('window.__TAURI__ && window.__TAURI__.core.invoke("import_document", {zip_path: "/path/to/file.zip"});')
```

### From MCP Server

The MCP server can use this mechanism to control the GUI programmatically:

```rust
// Write to the queue file
std::fs::write("/tmp/narrative-eval-queue.txt", 
    r#"window.__TAURI__.core.invoke("open_project", {path: "/path/to/project"});"#
).unwrap();
```

## Available Tauri Commands

These commands can be invoked via `window.__TAURI__.core.invoke()`:

| Command | Description | Parameters |
|---------|-------------|------------|
| `open_project` | Open a project | `{path: string}` |
| `close_project` | Close current project | none |
| `get_project_path` | Get current project path | none |
| `import_document` | Import a MinerU zip | `{zip_path: string}` |
| `get_toc` | Get table of contents | none |
| `get_page_stats` | Get page statistics | none |
| `get_blocks` | Get semantic blocks | `{parent_id?, limit, offset}` |
| `get_block` | Get single block | `{id: string}` |
| `get_blocks_by_page` | Get blocks by page range | `{page_start, page_end}` |
| `update_block` | Update block content | `{id, content, version}` |
| `search_blocks` | Search blocks | `{query, limit?}` |
| `list_project_files` | List project files | none |
| `find_asset_file` | Find asset file | `{pattern: string}` |
| `capture_window` | Capture window screenshot | none |
| `save_screenshot` | Save screenshot | none |

## Testing

Run the test script:

```bash
cd /Users/futuremeng/github/narrativeos/narrative-structure
bash scripts/test_mcp.sh
```

Or manually:

```bash
# Make sure Tauri dev is running
# Then write a script to the queue:
echo 'console.log("[EVAL] Queue system works!");' > /tmp/narrative-eval-queue.txt

# Check if it was consumed (should be empty after ~1 second):
sleep 2
cat /tmp/narrative-eval-queue.txt  # Should be empty
```

## Troubleshooting

### Queue not being consumed

1. **Check Tauri is running**: `ps aux | grep narrative-structure | grep -v mcp | grep -v grep`
2. **Check the file exists**: `ls -la /tmp/narrative-eval-queue.txt`
3. **Check frontend is loaded**: The `useEvalQueue` hook only runs after React mounts
4. **Check for errors**: Look at the Tauri dev terminal for any errors

### Scripts not executing

1. **Syntax**: Ensure the JavaScript is valid
2. **Tauri API**: Always check `window.__TAURI__` exists before using
3. **Async**: Tauri commands return promises - use `.then()` or `await`

## Security Note

⚠️ This mechanism executes arbitrary JavaScript in the application context. Only write trusted scripts to the queue file. The file path is fixed at `/tmp/narrative-eval-queue.txt` and is readable/writable by the current user.