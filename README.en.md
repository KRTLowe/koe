<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="client/public/kaya-float.png">
    <img src="client/public/kaya-float.png" width="160" alt="Kaya Logo">
  </picture>
  <h1 align="center">Kaya-On-Everywhere</h1>
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
</p>

Kaya-On-Everywhere is the desktop extension layer for Kaya — connected via OpenCode MCP protocol, providing file transfer, real-time chat, and remote desktop tooling to the LLM.

## Quick Start

### Prerequisites

| Component | Server (Linux) | Client Build (Windows) |
|-----------|---------------|-----------------------|
| Python | ≥ 3.11 | — |
| Rust | — | ≥ 1.80 |
| Node.js | — | ≥ 18 |
| WebView2 | — | Built-in on Win11 |

### Server

```bash
cd server
pip install -e .

# Full startup (recommended)
python3 run_and_send.py 9765 8765

# MCP stdio only (limited)
kaya-transfer-hub serve --ws-port 9765
```

### Register a Client

```bash
kaya-transfer-hub register-client --id pc-01 --desc "Windows PC" --passkey "<your-key>"
kaya-transfer-hub list-clients
```

### opencode.json Config

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

### Windows Client Build

```bash
cd client
npm install
npm run tauri build
```

One-click on Windows: run `build.bat`.

## Features

| Feature | Description |
|---------|-------------|
| File Transfer | Bidirectional push between LLM ↔ Windows with SHA256 checksum |
| ACP Chat | Real-time conversation via floating chat window |
| Floating Character | Always-on-top transparent character with message bubbles |
| Remote Tools | 21 tools: screenshot, OCR, UIA tree, file I/O, clipboard, input |
| Copilot | Single-shot query or continuous monitoring loop |
| Session Persistence | Python ACP bridge survives disconnects, auto-recovers |
| Shortcuts | `Ctrl+Alt+K` quick chat / `Ctrl+Alt+S` copilot / `Ctrl+Alt+C` monitoring |
| Signal System | Sticky/one-shot signals with priority queue |
| Path Security | Configurable allow/deny path lists, tool permissions |

## Remote Tools (21)

| Tool | Description |
|------|-------------|
| `take_screenshot` | Capture screen region |
| `ocr_region` | Windows OCR text recognition |
| `uia_tree` | UIA accessibility tree |
| `get_clipboard` / `set_clipboard` | Clipboard read/write |
| `read_file` / `write_file` | Path-safe file I/O |
| `list_directory` / `file_info` | Directory / file metadata |
| `grep_file` | File content search |
| `pull_file` | Pull file to server |
| `run_command` | Execute command with output |
| `type_text` / `key_press` / `mouse_click` | Input simulation |
| `list_windows` / `get_foreground_window` | Window management |
| `start_process` / `kill_process` | Process management |
| `system_info` | System information |
| `open_path` | Open path in Explorer |

## CLI

```bash
kaya-transfer-hub serve --ws-port 9765
kaya-transfer-hub mcp
kaya-transfer-hub register-client --id <id> --desc "<desc>" --passkey "<key>"
kaya-transfer-hub list-clients
kaya-transfer-hub remove-client --id <id>
```

## Protocol

See [docs/protocol.md](docs/protocol.md) for WebSocket protocol details.

## Project Structure

```
server/                          # Python server
├── run_and_send.py              # Entry point (WS + ACP + Unix socket)
├── pyproject.toml
├── src/
│   ├── kaya_transfer_hub/       # MCP tool layer (8 tools + CLI)
│   └── kaya_server/             # Runtime (WS/ACP/signals/auth/DB)
└── tests/

client/                          # Tauri 2 Windows client
├── src/                         # Vue 3 frontend (10 views)
└── src-tauri/src/               # Rust backend (18 modules + 21 tools)

docs/
├── protocol.md                  # WebSocket protocol
└── superpowers/                 # Design docs
```

## License

MIT
