<p align="center">
  <h1 align="center">kaya-transfer-hub</h1>
  <p align="center">
    <em>Bring LLM to your desktop — file transfer, real-time chat, remote tools, floating companion</em>
  </p>

<p align="center">
  <img src="https://img.shields.io/badge/python-≥3.11-blue?logo=python" alt="Python">
  <img src="https://img.shields.io/badge/rust-≥1.80-orange?logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/vue-3-4FC08D?logo=vue.js" alt="Vue 3">
  <img src="https://img.shields.io/badge/tauri-2-FFC131?logo=tauri" alt="Tauri 2">
  <img src="https://img.shields.io/badge/license-MIT-green" alt="License">
</p>

---

## Architecture

```
Kaya (OpenCode)
  │ MCP stdio
  ▼
kaya-transfer-hub (Python server)
  ├── WS 9765 → auth, file transfer, tool proxy, signals
  └── ACP 8765 → native Python bridge, session persistence
       │
       ▼
Tauri 2 Windows Client (Rust + Vue 3)
  ├── Floating character + message bubbles
  ├── 21 remote tools (screenshot, OCR, UIA, I/O, clipboard…)
  ├── Copilot overlay (single / continuous monitoring)
  ├── Zombie detection + auto-recovery
  └── Quick chat, settings, file transfer UI
```

## Quick Start

### Server

```bash
cd server
pip install -e .
kaya-transfer-hub serve --ws-port 9765
```

### MCP Config (opencode.json)

```json
{
  "mcpServers": {
    "kaya-transfer-hub": {
      "command": "kaya-transfer-hub",
      "args": ["serve", "--ws-port", "9765"]
    }
  }
}
```

### Windows Client

```bash
cd client
npm install
npm run tauri build
```

## Features

| Feature | Description |
|---------|-------------|
| 📁 File Transfer | Bidirectional push between LLM ↔ Windows desktop with SHA256 checksum |
| 💬 ACP Chat | Real-time conversation with Kaya via floating chat window |
| 🖼️ Floating Character | Always-on-top transparent character with message bubbles |
| 🖥️ Remote Tools | 21 tools: screenshot, OCR, UIA tree, file I/O, clipboard, input |
| 🔍 Copilot | Single-shot query or continuous monitoring loop |
| 🩺 Health Check | /proc diagnostics + non-blocking zombie detection + recovery |
| 🔄 Session Persistence | Python ACP bridge survives disconnects, auto-recovers |
| ⚡ Shortcuts | `Ctrl+Alt+K` quick chat / `Ctrl+Alt+S` copilot / `Ctrl+Alt+C` monitoring |
| 🔔 Signal System | Sticky/one-shot signals with priority queue |
| 🛡️ Path Security | Configurable allow/deny paths, tool permission toggles |

## Project Structure

```
kaya-transfer-hub/
├── server/                          # Python server
│   ├── run_and_send.py              # Entry point
│   ├── pyproject.toml
│   ├── src/
│   │   ├── kaya_transfer_hub/       # MCP tool layer
│   │   │   ├── server.py            # Tool registration
│   │   │   ├── mcp_agent.py         # Unix socket RPC agent
│   │   │   ├── tool_defs.py         # Shared tool definitions
│   │   │   ├── tool_registry.py     # Client tool registry + signals
│   │   │   ├── cli.py               # Click CLI
│   │   │   └── __main__.py
│   │   └── kaya_server/             # Runtime services
│   │       ├── ws_handler.py        # WebSocket (9765)
│   │       ├── acp_bridge.py        # ACP bridge (8765)
│   │       ├── signal_registry.py   # Signal scheduling
│   │       ├── signal_handlers.py   # Handlers
│   │       ├── connection_manager.py
│   │       ├── auth.py              # bcrypt auth
│   │       ├── db.py, models.py, constants.py
│   └── tests/
├── client/                          # Tauri 2 client
│   ├── src/                         # Vue 3 frontend (10 views)
│   └── src-tauri/src/               # Rust backend (18 modules + 21 tools)
├── build.bat
├── docs/protocol.md                 # WebSocket protocol spec
├── README.md
└── README.en.md
```

## CLI

```bash
kaya-transfer-hub serve --ws-port 9765
kaya-transfer-hub mcp
kaya-transfer-hub register-client --id <id> --desc "<desc>" --passkey "<key>"
kaya-transfer-hub list-clients
kaya-transfer-hub remove-client --id <id>
```

## License

MIT
