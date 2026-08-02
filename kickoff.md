# **PROJECT SPECIFICATION: SUBSTRATE**

## **1\. Vision & Core Concept**

**Substrate** is an ambient, sci-fi TUI (Terminal User Interface) RPG built in Rust.

The player (and connected LLM agents) interacts with an awakening robotic intelligence on a desolate planet.

The game operates as a **decoupled daemon process** with a **rich TUI frontend** and a native **MCP (Model Context Protocol) server**, allowing it to run silently in the background while offering deep interactive simulation when attached or connected via an AI co-processor.

## **2\. Technical Architecture**

                       \+-----------------------------------+  
                       |        Ratatui TUI Client         |  
                       |    (Visual Canvas, DSL, Dash)     |  
                       \+-----------------+-----------------+  
                                         |  
                                         | IPC (Local TCP / Socket)  
                                         v  
\+------------------+   JSON-RPC   \+-----------------------------------+  
| LLM / AI Agent   | \------------\>|          Mind Daemon Engine       |  
| (via MCP Client) | \<------------|  \- High-precision Simulation Loop |  
\+------------------+              |  \- Circuit Solver Engine          |  
                                  |  \- SQLite / JSON Persistence      |  
                                  |  \- Embedded MCP Server            |  
                                  \+-----------------------------------+

### **Stack & Dependencies**

* **Language:** Rust (2021 Edition)  
* **Runtime:** tokio (Async I/O for daemon IPC, background ticks, and MCP protocol)  
* **TUI Frontend:** ratatui \+ crossterm  
* **Serialization & Data:** serde, serde\_json, rusqlite or redb  
* **IPC & Networking:** interprocess / tokio::net (Cross-platform Unix Domain Sockets & Windows Named Pipes or Local TCP)  
* **CLI Parsing:** clap

## **3\. Core Mechanics: The 3-Tier Progression**

The awakening mind regains functionality across three distinct operational tiers:

### **Tier 1: Abstract Upgrades (Awakening Phase)**

* **Gameplay:** Resource accumulation (![][image1] power, memory pages, bus bandwith) and spending raw signals to activate core modules.  
* **UI:** Metric dashboards, system logs, and upgrade nodes.

### **Tier 2: Textual & DSL Logic Wiring (Assembly Phase)**

* **Gameplay:** Writing text commands and lightweight DSL scripts to wire logic gates and create automated routines (e.g., WIRE SENSOR\_01 \-\> G\_AND\_01.A).  
* **UI:** Interactive terminal prompt with syntax highlights, autocomplete, and signal flow telemetry.

### **Tier 3: Visual Canvas Grid (Engineering Phase)**

* **Gameplay:** Full interactive ASCII/Unicode grid canvas where components (Gates, Transistors, Splitters, Micro-kernels) are physically placed, routed, and simulated in real time.  
* **UI:** Ratatui grid rendering engine with mouse/keyboard navigation, layer toggles, and oscilloscope visualizers.

## **4\. MCP (Model Context Protocol) Integration**

MCP is a **core architectural layer**, not an add-on. The Mind Daemon hosts an internal MCP Server exposed via stdio or local TCP port (127.0.0.1:7777).

### **Initial Exposed Tools (Day 1 MVP):**

1. get\_mind\_status: Returns current energy levels, active components, diagnostics, and recent log outputs.  
2. execute\_dsl\_command: Executes a wiring or operational script line directly in the daemon engine.  
3. query\_circuit\_graph: Returns a JSON representation of active nodes and logic paths.  
4. inspect\_visual\_grid: Provides a spatial ASCII/JSON matrix of the Tier 3 visual grid.  
5. trigger\_diagnostic\_pulse: Executes a logic state pulse through the system.

## **5\. Development Guidelines for Coding Agents**

Any AI coding agent (Claude Code, Codex, etc.) modifying or extending this codebase **must** strictly adhere to the following protocol:

### **Rule 1: Maintain Self-Documenting Repository Artifacts**

* When creating a new module or system (e.g., circuit solver, daemon IPC, TUI widget), the agent must create or update a corresponding skill file in .agent/skills/\<feature\_name\>.md.  
* Keep AGENTS.md continuously updated with build instructions, system architecture summaries, and operational commands.

### **Rule 2: Decouple Simulation from Rendering**

* **The Daemon is Source of Truth:** Never embed game state logic inside Ratatui widgets or frontend render routines.  
* All game tick calculations must occur in the background engine process and communicate via thread-safe channels or IPC.

### **Rule 3: Cross-Platform Native Guarantee**

* Ensure all file paths use std::path::PathBuf.  
* Ensure IPC transport works on Windows (Named Pipes/Local Sockets), macOS, and Linux without platform lock-in.  
* Graceful Unicode degradation for environments without full font glyph support.

## **6\. Phase 1 Implementation Roadmap**

The agent should execute the implementation in the following sequential phases:

### **Phase 1.1: Core Scaffold & IPC Engine**

* Initialize Rust workspace with binary targets: mind-daemon and mind-cli/mind-tui.  
* Establish local IPC protocol for state synchronization between daemon and client.  
* Create AGENTS.md and .agent/skills/ directory structure.

### **Phase 1.2: Embedded MCP Server**

* Implement the baseline stdio/TCP MCP server inside mind-daemon.  
* Expose get\_mind\_status and basic diagnostic tools.

### **Phase 1.3: Ratatui TUI Attachment**

* Build simple Ratatui dashboard to attach/detach from running mind-daemon.  
* Display real-time telemetry (Tick rate, CPU usage, simulated power ![][image1], scrolling log viewer).

### **Phase 1.4: Tier 1 & Tier 2 Circuit Mechanics**

* Implement core logic simulation loop (AND, OR, NOT, XOR gates).  
* Create DSL parser for textual wiring commands.  
* Expose DSL commands through both TUI shell and MCP tools.

[image1]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAACgAAAAaCAYAAADFTB7LAAACuElEQVR4Xu2WS2hTQRSGb0iFCj7wEaF53bywiIJifCCo4EIxoFLEnQhCQRG6kBaRooiCKze+QFSKIO7EnQ9EXBS6EbpyIbisIi5aRCgoqEj9/uZOcjqpCUJWcn/4OTPnnDn3vzPnThIEMWL8BwjDcBD+hHOO+Xz+LqEk4xEv9hte1bpsNrua8RsbKxaLB7zy3QHF++AH+L5SqaRsLJPJZPF/ghP9/f3LbQwk8N+Gw4yTXqx7YMdW8ZDJSGSfjRmBk8qzMXaxot3GLrX+riOVSi1DwLiESJAJJQqFwmhYP0JfvGJX4E7jawVHsoKFtXK5nNO8Wq0uyeVy21g44HwgUSqV1vO2x2Q1b1YIAnJ7qfEcfiOn6vyMN+J7FNZ7bYFA5Ulg4NVaAL05SfdIvgQ/whMUeYo9hb0AZxgfwt6HF8k9iZ2CI34tYg89gT3Mr9H4e8P67jZi2gTyb+qITYlWkHQQnqbIZgrMwtfs0ErF9LbRW08T32HWSMi4Xq5ZqeGfgzXNOYU9+EaD+lEuEE/sCL6zdv2iIOlMJO4o/AX3uRjjDfALRc87n+m1J0x7nF9QngRiD6vpsTdci1jx9Oga7ANi6+z6tmDBLfgunU6vdT69Jb4fPGi3yZsXDYecz8EJhMdFnYyLRfXnxWMHiQ3YtW2hu4lFE6G3K1HRKXYjY3xD8CviNzmfiUmYRFzH3nGtIhjxw9rZf7pWwuau6LKcx2JHGR3bS1FjYudsb0a7IxHTjPc7fxRzAmfVmzbWEWHUf385yoZoXS/MZ3R06i3GY/ZDYV6TCOKP9ZU6v2DEjwVe73YEBS+z8K2a1/mih32Hu5wv2tUX5L/CPsNucTEBEdV8/apq3IMmJoGfO14ri4EH9fpXBkjm6z9L/iWa1Ifk75CgH3+Ob3vQusb9Mdjq+2PEiBEjRnv8AW8lziAJsTpvAAAAAElFTkSuQmCC>
