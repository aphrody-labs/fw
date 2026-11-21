//! MCP over ACP: the gateway as one in-process rmcp server per session, attached to an ACP connection
//! without a child process, a socket or a port.

use crate::Gateway;
use crate::server::GatewayServer;
use agent_client_protocol::mcp_server::McpServer;
use agent_client_protocol::role::Role;
use agent_client_protocol_rmcp::rmcp_3::McpServerExt;
use std::sync::Arc;

/// The name an agent sees as `serverId` prefix of the session's MCP server.
pub const SERVER_NAME: &str = "fw-gateway";

/// A server for `counterpart` (an `Agent` when a client attaches it to a new session, a `Client` when a proxy
/// does). Every session gets its own `GatewayServer`; they share the one gateway, so the catalogue, the open
/// MCP sessions and the secrets exist once per process.
pub fn mcp_server<Counterpart: Role>(gateway: Arc<Gateway>) -> McpServer<Counterpart> {
    McpServer::<Counterpart>::from_rmcp(SERVER_NAME, move || GatewayServer::new(gateway.clone()))
}
