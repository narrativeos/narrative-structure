//! NarrativeStructure MCP Server & CLI
//!
//! 支持两种模式：
//! 1. MCP Server 模式 (默认/serve)：通过 stdio 与外部 AI 智能体通信
//! 2. CLI 模式 (tools/call/project)：直接调用功能并输出 JSON
//!
//! 使用方法:
//!   # MCP Server 模式
//!   narrative-mcp serve --project /path/to/project
//!   narrative-mcp -p /path/to/project    # 简写
//!
//!   # CLI 模式
//!   narrative-mcp tools --project /path/to/project
//!   narrative-mcp call get_toc --project /path/to/project
//!   narrative-mcp call get_blocks --limit 10 --project /path/to/project
//!   narrative-mcp project --project /path/to/project
//!   narrative-mcp project --json --project /path/to/project

use narrative_structure::mcp::server::run_mcp_server;

fn print_usage() {
    println!("NarrativeStructure MCP Server & CLI v0.1.0");
    println!();
    println!("USAGE:");
    println!("  narrative-mcp <COMMAND> [OPTIONS]");
    println!();
    println!("COMMANDS:");
    println!("  serve       Start MCP Server (stdio mode, default)");
    println!("  tools       List all available tools (JSON output)");
    println!("  call        Call a tool directly");
    println!("  project     Show project information");
    println!("  help        Show this help message");
    println!();
    println!("OPTIONS:");
    println!("  -p, --project <PATH>   Project directory path (with narrative.db)");
    println!("  --json                 Output in JSON format (for project command)");
    println!("  -h, --help             Show help for a command");
    println!();
    println!("EXAMPLES:");
    println!("  # Start MCP Server");
    println!("  narrative-mcp serve -p ~/.narrativeos/narrative-structure/projects/MyDoc");
    println!();
    println!("  # List all tools");
    println!("  narrative-mcp tools -p ~/.narrativeos/narrative-structure/projects/MyDoc");
    println!();
    println!("  # Call a tool");
    println!("  narrative-mcp call get_toc -p ~/.narrativeos/narrative-structure/projects/MyDoc");
    println!("  narrative-mcp call get_blocks --limit 10 -p ~/.narrativeos/narrative-structure/projects/MyDoc");
    println!("  narrative-mcp call search_blocks --query 'keyword' -p ~/.narrativeos/narrative-structure/projects/MyDoc");
    println!();
    println!("  # Get project info");
    println!("  narrative-mcp project -p ~/.narrativeos/narrative-structure/projects/MyDoc");
    println!("  narrative-mcp project --json -p ~/.narrativeos/narrative-structure/projects/MyDoc");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }
    
    let command = args[1].as_str();
    
    match command {
        "help" | "-h" | "--help" => {
            print_usage();
            return;
        }
        "version" | "-v" | "--version" => {
            println!("narrative-mcp 0.1.0");
            return;
        }
        _ => {}
    }
    
    // Parse common options
    let mut project_path: Option<String> = None;
    let mut json_output = false;
    let mut tool_args: Vec<String> = Vec::new();
    
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--project" | "-p" => {
                if i + 1 < args.len() {
                    project_path = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: --project requires a path argument");
                    std::process::exit(1);
                }
            }
            "--json" => {
                json_output = true;
                i += 1;
            }
            _ => {
                tool_args.push(args[i].clone());
                i += 1;
            }
        }
    }
    
    match command {
        // ---------------------------------------------------------------
        // MCP Server 模式
        // ---------------------------------------------------------------
        "serve" | "" => {
            // Backward compatibility: narrative-mcp -p <path> starts server
            if project_path.is_none() && args.len() >= 3 {
                // Check if old style args are used
                let mut j = 2;
                while j < args.len() {
                    match args[j].as_str() {
                        "--project" | "-p" => {
                            if j + 1 < args.len() {
                                project_path = Some(args[j + 1].clone());
                                j += 2;
                            } else {
                                j += 1;
                            }
                        }
                        _ => {
                            // Unknown arg in old mode, might be project path directly
                            if project_path.is_none() {
                                project_path = Some(args[j].clone());
                            }
                            j += 1;
                        }
                    }
                }
            }
            
            eprintln!("[MCP] Starting NarrativeStructure MCP Server...");
            if let Some(ref p) = project_path {
                eprintln!("[MCP] Project path: {}", p);
            }
            run_mcp_server(project_path);
        }
        
        // ---------------------------------------------------------------
        // CLI: List tools
        // ---------------------------------------------------------------
        "tools" => {
            let path = project_path.clone().unwrap_or_else(|| {
                eprintln!("Error: --project is required for 'tools' command");
                std::process::exit(1);
            });
            
            let result = call_mcp_tool(&path, "tools/list", &serde_json::json!({}));
            match result {
                Ok(response) => {
                    println!("{}", serde_json::to_string_pretty(&response).unwrap());
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        
        // ---------------------------------------------------------------
        // CLI: Call a tool directly
        // ---------------------------------------------------------------
        "call" => {
            if tool_args.is_empty() {
                eprintln!("Error: 'call' requires a tool name");
                eprintln!("Usage: narrative-mcp call <tool_name> [--key value]* -p <project>");
                std::process::exit(1);
            }
            
            let tool_name = tool_args[0].clone();
            let path = project_path.clone().unwrap_or_else(|| {
                eprintln!("Error: --project is required for 'call' command");
                std::process::exit(1);
            });
            
            // Parse key-value arguments as JSON
            let mut arguments = serde_json::Map::new();
            let mut j = 1;
            while j < tool_args.len() {
                let key = tool_args[j].trim_start_matches("--");
                if j + 1 < tool_args.len() && !tool_args[j + 1].starts_with("--") {
                    // Try to parse as number
                    if let Ok(n) = tool_args[j + 1].parse::<i64>() {
                        arguments.insert(key.to_string(), serde_json::json!(n));
                    } else {
                        arguments.insert(key.to_string(), serde_json::json!(tool_args[j + 1]));
                    }
                    j += 2;
                } else {
                    arguments.insert(key.to_string(), serde_json::json!(true));
                    j += 1;
                }
            }
            
            let method = "tools/call".to_string();
            let params = serde_json::json!({
                "name": tool_name,
                "arguments": arguments
            });
            
            let result = call_mcp_tool(&path, &method, &params);
            match result {
                Ok(response) => {
                    if let Some(error) = response.get("error") {
                        eprintln!("Error: {}", error);
                        std::process::exit(1);
                    }
                    println!("{}", serde_json::to_string_pretty(&response).unwrap());
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        
        // ---------------------------------------------------------------
        // CLI: Project info
        // ---------------------------------------------------------------
        "project" => {
            let path = project_path.clone().unwrap_or_else(|| {
                eprintln!("Error: --project is required for 'project' command");
                std::process::exit(1);
            });
            
            let result = call_mcp_tool(&path, "tools/call", &serde_json::json!({
                "name": "get_project_info",
                "arguments": {}
            }));
            
            match result {
                Ok(response) => {
                    if let Some(error) = response.get("error") {
                        eprintln!("Error: {}", error);
                        std::process::exit(1);
                    }
                    if json_output {
                        println!("{}", serde_json::to_string_pretty(&response).unwrap());
                    } else {
                        // Pretty print the text content
                        if let Some(content_arr) = response.get("result")
                            .and_then(|r| r.get("content"))
                            .and_then(|c| c.as_array()) 
                        {
                            if let Some(first) = content_arr.first()
                                .and_then(|f| f.get("text"))
                                .and_then(|t| t.as_str()) 
                            {
                                println!("{}", first);
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        
        _ => {
            eprintln!("Unknown command: {}", command);
            print_usage();
            std::process::exit(1);
        }
    }
}

/// Call an MCP tool via subprocess (for CLI mode)
/// This spawns the MCP server, sends a request, and reads the response
fn call_mcp_tool(project_path: &str, method: &str, params: &serde_json::Value) -> Result<serde_json::Value, String> {
    // Build the JSON-RPC request
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params
    });
    
    // For direct database access, we use rusqlite directly
    // This avoids the complexity of forking self
    match method {
        "tools/list" => {
            // Return the tools list directly
            Ok(narrative_structure::mcp::tools::list_tools_response())
        }
        "tools/call" => {
            // Extract tool name and arguments
            let tool_name = params.get("name")
                .and_then(|v| v.as_str())
                .ok_or("Missing tool name")?;
            let arguments = params.get("arguments").cloned().unwrap_or(serde_json::json!({}));
            
            // Create a temporary McpState for this call
            let state = narrative_structure::mcp::server::McpState::new_with_path(project_path.to_string());
            
            // Call the tool directly
            let result = narrative_structure::mcp::tools::call_tool(tool_name, &arguments, &state)?;
            
            Ok(serde_json::json!({
                "content": result
            }))
        }
        _ => Err(format!("Unknown method: {}", method)),
    }
}