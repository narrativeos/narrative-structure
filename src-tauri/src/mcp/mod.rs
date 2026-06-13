//! MCP (Model Context Protocol) Server 模块
//!
//! 通过 stdio 提供 JSON-RPC 2.0 接口，让外部 AI 智能体可以
//! 以结构化的方式调用 NarrativeStructure 的所有功能。
//!
//! 协议规范参考: https://modelcontextprotocol.io/specification

pub mod server;
pub mod tools;

pub use server::McpServer;
