//! Agent access to Unilux over the Model Context Protocol.
//!
//! The protocol layer is a small JSON-RPC 2.0 handler (MCP revision
//! 2025-06-18, tools and resources only), served on stdio for `unilux mcp`
//! and on a Unix socket so an agent can attach to a running GUI and see
//! its live session.

pub mod mcp;
pub mod transport;

pub use mcp::{ConfirmHook, Server, ToolInfo, tools};
pub use transport::{proxy_stdio_to_socket, serve_socket, serve_stdio};
