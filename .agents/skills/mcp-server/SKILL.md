---
name: mcp-server
description: Maintain AI-agent access to Substrate through MCP. Use when changing MCP tools, resources, schemas, protocol negotiation, Streamable HTTP or stdio transports, Host or Origin validation, backend adapters, tool discovery, or MCP lifecycle and security.
---

# MCP server

## Current surface

- Use the official `rmcp` SDK pinned at 3.1.0 and advertise only MCP protocol `2026-07-28`.
- Serve Streamable HTTP at `http://127.0.0.1:7777/mcp` and stdio through `mind-mcp`, which requires a running daemon.
- Keep `get_mind_status` and `trigger_diagnostic_pulse` as the current diagnostic surface until their owning roadmap phase changes them.
- Instantiate the same `MindMcpServer` handler for direct-engine HTTP and IPC-backed stdio. Never create a fallback engine or transport-owned mutation.

## Security and lifecycle

- Bind only to loopback, validate Host and optional Origin, require stateless request metadata, and keep the endpoint without bearer authentication until the security model changes deliberately.
- Bind before daemon readiness, cancel HTTP before runtime metadata removal, and keep stdio stdout exclusive to protocol frames.
- Expose only player-visible, unlocked knowledge. Never expose hidden world state, unreleased story answers, private narrative sources, provider credentials, or an external agent's configuration.

## Tool and resource contract

- Return concise text plus schema-backed `structuredContent` for successful tools. Surface backend failures as tool errors and malformed requests as protocol errors.
- Add typed core DTOs, one authoritative engine operation, IPC coverage where needed, and direct-engine plus IPC-backed tests for every tool.
- Add future domain tools only after their gameplay system exists. Gate gameplay tool discovery and calls through authoritative research state.
- Expose unlocked gameplay SKILLS as versioned, spoiler-safe Markdown resources; possession of instructions must never bypass engine capability gates.
- Use stable `capability_locked` and `policy_denied` results for well-formed future gameplay denials.

Use the `external-coprocessor` skill for research-gated automation, policy, export, or pulse behavior. MCP `2026-07-28` does not permit standalone unsolicited LLM sampling; implement future periodic pulses through client heartbeat calls associated with active requests.
