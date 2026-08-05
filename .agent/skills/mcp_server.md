# MCP server

Use this guide when changing AI-agent access to Substrate.

## Supported surface

- Official `rmcp` SDK pinned at 3.1.0
- MCP protocol `2026-07-28` only; legacy initialize/session clients are rejected
- Streamable HTTP at `http://127.0.0.1:7777/mcp`
- Stdio through the `mind-mcp` bridge, which requires a running daemon
- Tools: `get_mind_status`, `trigger_diagnostic_pulse`, `execute_dsl_command`, and `query_circuit_graph`

The daemon and bridge must instantiate the same `MindMcpServer` handler. Direct HTTP uses an engine-backed adapter; stdio uses an IPC-backed adapter. Never create a fallback engine in the bridge, duplicate mutations in a transport, or advertise mechanics that are not implemented.

## Security and lifecycle

Bind HTTP only to loopback, validate Host and optional Origin against the configured endpoint, and require stateless per-request protocol metadata. The local endpoint has no bearer token in this phase. Any MCP bind failure aborts daemon startup before runtime metadata is published.

Daemon shutdown must cancel HTTP work before removing runtime metadata. The stdio binary writes protocol frames only to stdout; all human diagnostics and tracing go to stderr. If the daemon handshake fails, exit nonzero without starting the MCP service.

## Tool contract

Every successful tool returns both a concise text content block and schema-backed `structuredContent`. Backend failures are visible tool errors. Malformed requests and unsupported protocol versions are protocol errors.

New tools require typed core DTOs, a single authoritative engine operation, IPC coverage where the stdio bridge needs it, in-process MCP tests, HTTP tests, and documentation updates. DSL and circuit graph tools must remain thin adapters over `MindEngine`/`IpcClient`; visual grid tools remain deferred until their owning system exists.
