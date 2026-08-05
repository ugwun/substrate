---
name: mcp-server
description: Maintain AI-agent access to Substrate through MCP. Use when changing MCP tools or schemas, the shared MindMcpServer handler, Streamable HTTP or stdio transports, protocol negotiation, Host or Origin validation, backend adapters, or MCP lifecycle and security behavior.
---

# MCP server

## Supported surface

- Use the official `rmcp` SDK pinned at 3.1.0.
- Advertise only MCP protocol `2026-07-28` and reject legacy initialize or session clients.
- Serve Streamable HTTP at `http://127.0.0.1:7777/mcp`.
- Serve stdio through the `mind-mcp` bridge and require a running daemon.
- Expose only `get_mind_status`, `trigger_diagnostic_pulse`, `execute_dsl_command`, and `query_circuit_graph` until the product surface changes.

Instantiate the same `MindMcpServer` handler in the daemon and bridge. Back direct HTTP with the engine and stdio with IPC. Never create a fallback engine in the bridge, duplicate mutations in a transport, or advertise unimplemented mechanics.

## Security and lifecycle

- Bind HTTP only to loopback.
- Validate Host and optional Origin against the configured endpoint.
- Require stateless per-request protocol metadata.
- Keep the local endpoint without a bearer token until the security model changes deliberately.
- Abort daemon startup before publishing runtime metadata when the MCP listener cannot bind.
- Cancel HTTP work before removing runtime metadata during daemon shutdown.
- Write protocol frames only to stdout in the stdio binary; send human diagnostics and tracing to stderr.
- Exit nonzero without starting MCP service when the daemon handshake fails.

## Tool contract

Return a concise text content block and schema-backed `structuredContent` for every successful tool. Surface backend failures as tool errors. Return malformed requests and unsupported protocol versions as protocol errors.

For every new tool, add typed core DTOs, a single authoritative engine operation, IPC coverage where the stdio bridge needs it, in-process MCP tests, HTTP tests, and documentation updates. Keep DSL and circuit graph tools as thin adapters over `MindEngine` or `IpcClient`. Defer visual grid tools until their owning system exists.
