//! MCP Server — JSON-RPC 2.0 over stdio
//!
//! 实现 MCP 协议的服务端，通过标准输入/输出与外部智能体通信。
//! 支持 initialize、tools/list、tools/call 三个核心 RPC 方法。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 协议类型
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    pub fn success(id: Value, result: Value) -> Self {
        Self { jsonrpc: "2.0".to_string(), id, result: Some(result), error: None }
    }

    pub fn error(id: Value, code: i32, message: &str) -> Self {
        Self { jsonrpc: "2.0".to_string(), id, result: None, error: Some(JsonRpcError { code, message: message.to_string(), data: None }) }
    }

    pub fn error_with_data(id: Value, code: i32, message: &str, data: Value) -> Self {
        Self { jsonrpc: "2.0".to_string(), id, result: None, error: Some(JsonRpcError { code, message: message.to_string(), data: Some(data) }) }
    }
}

// ---------------------------------------------------------------------------
// MCP Server 状态
// ---------------------------------------------------------------------------

/// 共享的项目状态，供 MCP 工具调用
pub struct McpState {
    pub project_path: Mutex<Option<String>>,
}

impl McpState {
    pub fn new() -> Self {
        Self { project_path: Mutex::new(None) }
    }
    
    /// Create McpState with a pre-set project path (for CLI mode)
    pub fn new_with_path(path: String) -> Self {
        Self { project_path: Mutex::new(Some(path)) }
    }
}

// ---------------------------------------------------------------------------
// MCP Server
// ---------------------------------------------------------------------------

pub struct McpServer {
    state: Arc<McpState>,
}

impl McpServer {
    pub fn new(state: McpState) -> Self {
        Self { state: Arc::new(state) }
    }

    /// 启动 MCP Server，从 stdin 读取请求，向 stdout 返回响应
    pub fn run(&self) {
        let stdin = io::stdin();
        let stdout = io::stdout();
        let reader = BufReader::new(stdin);
        let mut writer = stdout.lock();

        eprintln!("[MCP] Server started, waiting for requests on stdio...");

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("[MCP] Error reading stdin: {}", e);
                    break;
                }
            };

            if line.is_empty() {
                continue;
            }

            let response = self.handle_request(&line);
            let response_str = serde_json::to_string(&response).unwrap_or_default();
            let _ = writeln!(writer, "{}", response_str);
            let _ = writer.flush();
        }

        eprintln!("[MCP] Server shutting down (stdin closed)");
    }

    /// 处理单个 JSON-RPC 请求
    fn handle_request(&self, line: &str) -> JsonRpcResponse {
        // 解析请求
        let request: JsonRpcRequest = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(e) => {
                return JsonRpcResponse::error(Value::Null, -32700, &format!("Invalid JSON: {}", e));
            }
        };

        // 路由到对应方法
        match request.method.as_str() {
            "initialize" => self.handle_initialize(&request.id),
            "tools/list" => self.handle_tools_list(&request.id),
            "tools/call" => self.handle_tools_call(&request.id, &request.params),
            "notifications/initialized" => JsonRpcResponse::success(request.id, json!({})),
            _ => JsonRpcResponse::error(request.id, -32601, &format!("Unknown method: {}", request.method)),
        }
    }

    /// 初始化握手
    fn handle_initialize(&self, id: &Value) -> JsonRpcResponse {
        JsonRpcResponse::success(id.clone(), json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {
                    "listChanged": true
                }
            },
            "serverInfo": {
                "name": "narrative-structure-mcp",
                "version": "0.1.0"
            }
        }))
    }

    /// 列出所有可用的工具
    fn handle_tools_list(&self, id: &Value) -> JsonRpcResponse {
        let tools = crate::mcp::tools::list_tools();
        JsonRpcResponse::success(id.clone(), json!({ "tools": tools }))
    }

    /// 调用指定工具
    fn handle_tools_call(&self, id: &Value, params: &Value) -> JsonRpcResponse {
        let tool_name = params.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

        eprintln!("[MCP] Calling tool: {} with args: {}", tool_name, arguments);

        let state = self.state.clone();
        let result = crate::mcp::tools::call_tool(tool_name, &arguments, &state);

        match result {
            Ok(response) => JsonRpcResponse::success(id.clone(), json!({ "content": response })),
            Err(err) => JsonRpcResponse::error(id.clone(), -32603, &err),
        }
    }
}

// ---------------------------------------------------------------------------
// 从命令行参数中读取项目路径并运行
// ---------------------------------------------------------------------------

pub fn run_mcp_server(project_path_opt: Option<String>) {
    let state = McpState::new();
    if let Some(path) = project_path_opt {
        let mut p = state.project_path.lock().unwrap();
        *p = Some(path);
    }

    let server = McpServer::new(state);
    server.run();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonrpc_response_success() {
        let resp = JsonRpcResponse::success(json!(1), json!({"ok": true}));
        assert_eq!(resp.jsonrpc, "2.0");
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_jsonrpc_response_error() {
        let resp = JsonRpcResponse::error(json!(1), -32601, "Not found");
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        assert_eq!(resp.error.as_ref().unwrap().code, -32601);
    }

    #[test]
    fn test_parse_jsonrpc_request() {
        let line = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let req: JsonRpcRequest = serde_json::from_str(line).unwrap();
        assert_eq!(req.method, "initialize");
        assert_eq!(req.id, json!(1));
    }
}