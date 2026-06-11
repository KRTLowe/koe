<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="client/public/kaya-float.png">
    <img src="client/public/kaya-float.png" width="160" alt="KOE Logo">
  </picture>
  <h1 align="center">KOE — Kaya-On-Everywhere</h1>
  <p align="center">
    <em>Bridging LLMs to your desktop. File transfer, chat, remote tools — all through a floating companion.</em>
    <br>
    <a href="#features"><strong>Features</strong></a> ·
    <a href="#quick-start"><strong>Quick Start</strong></a> ·
    <a href="#architecture"><strong>Architecture</strong></a> ·
    <a href="#tech-stack"><strong>Tech Stack</strong></a>
    <br>
    <a href="README.md"><strong>中文版本</strong></a>
  </p>

<p align="center">
  <img src="https://img.shields.io/badge/python-≥3.11-blue?logo=python" alt="Python">
  <img src="https://img.shields.io/badge/rust-≥1.80-orange?logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/vue-3-4FC08D?logo=vue.js" alt="Vue 3">
  <img src="https://img.shields.io/badge/tauri-2-FFC131?logo=tauri" alt="Tauri 2">
  <img src="https://img.shields.io/badge/license-MIT-green" alt="License">
</p>
</p>

---

## Overview

KOE connects an LLM (Kaya) to your Windows desktop through a self-hosted bridge. It enables:

- **📁 File push** — Send files from the LLM directly to a Windows machine via MCP tools
- **💬 Real-time chat** — ACP (Agent Chat Protocol) conversation with the LLM in a native window
- **🖥️ Remote tools** — Screenshot capture, clipboard access, file search — expose your desktop to the LLM
- **🖼️ Floating companion** — A transparent, always-on-top character overlay with interactive message bubbles

The system is built around a **Python MCP server** (runs on Linux or any host) and a **Tauri 2 Windows client** (runs on the user's PC).

---

## Features

- **📁 File Transfer Hub** — Push files from LLM to Windows via WebSocket with binary frames, SHA256 checksums, and automatic retry
- **💬 ACP Chat** — Full-duplex JSON-RPC 2.0 chat with streaming responses, markdown rendering, and conversation history
- **🖼️ Floating Character** — An always-on-top, transparent window showing the Kaya character at the bottom-right corner of your screen
- **💭 Message Bubbles** — Auto-stacking, multi-column notification bubbles with 5-second debounce
- **⚡ Global Hotkeys** — `Ctrl+Alt+K` opens chat, `Ctrl+Alt+S` captures screenshot, `Ctrl+Alt+C` opens quick command
- **🖥️ Remote Desktop Tools** — Screenshot capture (`screenshots`), clipboard read/write (`arboard`), file search (`walkdir`)
- **🔐 Client Authentication** — Pre-registered client IDs with bcrypt-hashed passkeys
- **🎨 Themed UI** — CSS variable design system with a clean light theme (#6366F1 accent)

---

## Architecture

```
                                            ┌── OpenCode ACP Environment ─┐
                                            │                              │
                                            │  opencode acp                │
                                            │  ▲  stdin/stdout             │
                                            │  │                           │
                                            │  └─┴─────────────────────────┤
                                            │  stdio-to-ws (:8765)         │
                                            │  @rebornix/stdio-to-ws       │
                                            └──────────┬───────────────────┘
                                                       │ ACP JSON-RPC WS
                                                       │
┌──────────────────────┐    MCP stdio      ┌───────────┴──────────────────┐
│   LLM (Kaya)         │ ◄────────────────►│  Python Server              │
│   via OpenCode       │                   │  (MCP + WebSocket + Signals)│
└──────────────────────┘                   │  + SQLite Registry          │
                                             └──────┬──────────┬──────────┘
                                                     │          │
                                        ┌────────────┘          └────────────┐
                                        │ WebSocket (passkey)   │ ACP WS    │
                                        │ port 9765              │ port 8765 │
                                        ▼                       ▼           ▼
                              ┌──────────────────────────────────────────────────────┐
                              │              Tauri Windows Client                   │
                              │                                                      │
                              │  ┌─ WS 9765 ─────────────────────────────────┐      │
                              │  │  File Transfer · Signals · Remote Tools   │      │
                              │  ├─ WS 8765 ─────────────────────────────────┤      │
                              │  │  ACP Chat (direct to stdio-to-ws)         │      │
                              │  ├─ Main Window (960×640) ───────────────────┤      │
                              │  │ Sidebar 200px | 🏠 Home  💬 Chat  ⚙️ Set.│      │
                              │  │                📁 Files  🖼️ Capabilities  │      │
                              │  └────────────────────────────────────────────┘      │
                              │                                                      │
                              │  ┌─ Floating Windows (transparent, always-on-top) ┐  │
                              │  │  🖼️ kaya-float    Character at screen corner  │  │
                              │  │  💬 bubble-*      Message bubbles (auto-stack) │  │
                              │  │  ⚡ copilot       Copilot query overlay        │  │
                              │  └────────────────────────────────────────────────┘  │
                              │                                                      │
                              │  System Tray · Global Hotkeys · Desktop Notifications│
                              └──────────────────────────────────────────────────────┘
```

### Communication Layers

| Layer | Protocol | Port | Purpose |
|-------|----------|------|---------|
| Tool invocation | MCP stdio (JSON-RPC) | — | LLM calls `push_file`, `take_screenshot`, etc. |
| File / Signals | WebSocket (binary + JSON) | **9765** | 3-frame file transfer · Signals · Remote tool calls |
| ACP Chat | ACP JSON-RPC 2.0 over WebSocket | **8765** | Streaming conversation between user and LLM |
| ACP Bridge | stdio ↔ WebSocket | **8765** | `@rebornix/stdio-to-ws` wraps `opencode acp` stdio as WebSocket |

---

## ACP Chat Bridge: @rebornix/stdio-to-ws

The ACP (Agent Chat Protocol) chat feature requires an additional bridge component: **[@rebornix/stdio-to-ws](https://github.com/rebornix/stdio-to-ws)**.

### What It Does

`stdio-to-ws` is an npm package that wraps a stdio process's stdin/stdout as a WebSocket server. In this project, it converts `opencode acp`'s standard input/output into a WebSocket, allowing the Windows client to have ACP conversations with the LLM over WebSocket.

```
opencode acp (stdio)
     ▲     ▼
     │  stdin/stdout
     │
stdio-to-ws (:8765)
     ▲
     │  WebSocket (ACP JSON-RPC 2.0)
     │
Windows Client (acp_client.rs)
```

### Install

```bash
npm install -g @rebornix/stdio-to-ws
```

### systemd Service (opencode-bridge)

```ini
[Unit]
Description=ACP stdio-to-ws bridge for KOE
After=network.target

[Service]
Type=simple
ExecStart=npx @rebornix/stdio-to-ws --port 8766 -- /root/.opencode/bin/opencode acp
WorkingDirectory=/path/to/koe/server
Restart=always
RestartSec=3
User=your-user

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable opencode-bridge
sudo systemctl start opencode-bridge
```

> **Important:** `stdio-to-ws` spawns a separate `opencode acp` child process for each WebSocket client connection. Each client's ACP session is only valid within its own child process. Therefore, signal injection must go through the client's own ACP connection (i.e., `acp_inject` via 9765 → client → client's own ACP connection on 8765), rather than the server opening a separate connection for injection.

### Connection Flow

```
Client acp_client.rs                     stdio-to-ws (:8765)              opencode acp
      │                                      │                              │
      ├── WebSocket connect ────────────────►│                              │
      │                                      ├── spawn ───────────────────►│
      │                                      │                              │
      ├── {"jsonrpc":"2.0","id":1,           │                              │
      │    "method":"initialize",...}        │                              │
      │         ────────────────────────────►│──── stdin ──────────────────►│
      │                                      │                              │
      │◄── {"jsonrpc":"2.0","id":1,          │                              │
      │     "result":{"protocolVersion":1,...}│◄─── stdout ─────────────────│
      │         ◄────────────────────────────│                              │
      │                                      │                              │
      ├── session/new ──────────────────────►│──── stdin ──────────────────►│
      │◄── sessionId "ses_xxx" ◄────────────│◄─── stdout ─────────────────│
      │                                      │                              │
      ├── session/prompt ───────────────────►│──── stdin ──────────────────►│
      │◄── session/update (streaming) ◄─────│◄─── stdout ─────────────────│
      │◄── ResponseDone ◄───────────────────│◄─── stdout ─────────────────│
```

Each JSON-RPC message sent by the client is terminated with `\n` (newline-delimited JSON), and `stdio-to-ws` splits the stdio output by newlines, forwarding each as a WebSocket text frame.

---

## Quick Start

### 1. Deploy the Server (Linux recommended)

```bash
# Install
cd server
pip install -e .

# Register a client
kaya-transfer-hub register-client --id my-pc --desc "My Windows PC" --passkey "your-secret"

# Start the server
kaya-transfer-hub serve --ws-port 9765
```

### 2. Configure MCP (opencode.json)

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

### 3. Run the Windows Client

**Option A — Download a release** (coming soon)
**Option B — Build from source:**

```bash
cd client
npm install
npm run tauri build
```

On first launch, the client opens the **Settings** page where you configure:

| Field | Example | Description |
|-------|---------|-------------|
| WebSocket URL | `ws://192.168.1.100:9765` | Server address |
| Client ID | `my-pc` | The ID you registered |
| Passkey | `your-secret` | The passkey you set |
| Storage path | `~/kaya-transfer/` | Where received files are saved |

### 4. Use It

- **Push a file** — Ask Kaya to call the `push_file` MCP tool, and the file lands in your storage folder
- **Chat** — Press `Ctrl+Alt+K` or click tray menu "Open Chat" to talk to Kaya in real time
- **Capture screen** — Request a screenshot; the client captures it and sends it back to the LLM
- **Search files** — Ask Kaya to find a file on your Windows machine

---

## CLI Reference

```bash
# Register a new client
kaya-transfer-hub register-client --id <id> --desc "<description>" --passkey "<key>"

# List registered clients
kaya-transfer-hub list-clients

# Remove a client
kaya-transfer-hub remove-client --id <id>

# Start server
kaya-transfer-hub serve --ws-port <port>
```

---

## Tech Stack

### Server (Linux)

| Component | Technology |
|-----------|-----------|
| Runtime | Python ≥ 3.11 |
| Protocol | MCP SDK + WebSockets |
| Auth | bcrypt |
| CLI | Click |

### Client (Windows)

| Layer | Technology |
|-------|-----------|
| Desktop framework | Tauri 2 (Rust) |
| Frontend | Vue 3 + Pinia + Vue Router + Vite |
| WebSocket | tokio-tungstenite |
| Screenshots | screenshots crate |
| Clipboard | arboard crate |
| File search | walkdir crate |

### Communication

| Channel | Protocol |
|---------|----------|
| Tool invocation | MCP stdio (JSON-RPC 2.0) |
| File transfer | WebSocket (binary frames) |
| Chat | ACP JSON-RPC 2.0 (streaming) |
| Signals | WebSocket (JSON events) |

---

## Project Structure

```
koe/
├── server/                          # Python MCP server
│   ├── src/kaya_transfer_hub/
│   │   ├── server.py                # MCP tool registration
│   │   ├── ws_handler.py            # WebSocket handler (auth, heartbeat, file transfer, signals)
│   │   ├── connection_manager.py    # Online client connection pool
│   │   ├── db.py                    # SQLite client registry
│   │   ├── models.py                # Client data model
│   │   ├── auth.py                  # Passkey bcrypt hash/verify
│   │   ├── cli.py                   # Click CLI
│   │   ├── tool_registry.py         # Client-side tool registry
│   │   ├── signal_registry.py       # Signal dispatch (priority, sticky, TTL)
│   │   └── constants.py             # Constants
│   ├── tests/
│   │   ├── test_tool_registry.py
│   │   └── test_signal_registry.py
│   ├── pyproject.toml
│   └── run_and_send.py              # Combined entry: server + file push + ACP watchdog
│
├── client/                          # Tauri 2 Windows client
│   ├── public/
│   │   ├── kaya-float.png           # Character sprite (square)
│   │   └── kaya-full.png            # Character sprite (full-body)
│   ├── src/                         # Vue 3 frontend
│   │   ├── views/                   # Pages
│   │   │   ├── HomePage.vue         # Connection status + recent transfers
│   │   │   ├── FileTransferPage.vue # File transfer history (chat-bubble style)
│   │   │   ├── ChatPage.vue         # ACP chat window
│   │   │   ├── SettingsPage.vue     # Server / storage configuration
│   │   │   ├── CapabilitiesPage.vue # Tools, signals, and shortcuts
│   │   │   ├── FloatPage.vue        # Character overlay window
│   │   │   ├── BubblePage.vue       # Message bubble window
│   │   │   └── CopilotOverlayWindow.vue
│   │   ├── components/              # Reusable components
│   │   ├── stores/                  # Pinia state management
│   │   └── lib/                     # Utilities (types, IPC, WebSocket client)
│   ├── src-tauri/                   # Rust backend
│   │   ├── src/
│   │   │   ├── lib.rs               # App init, commands, ACP, bubble management
│   │   │   ├── config.rs            # Local config read/write
│   │   │   ├── ws_client.rs         # WebSocket file transfer client
│   │   │   ├── acp_client.rs        # ACP JSON-RPC 2.0 client
│   │   │   ├── file_handler.rs      # File save to disk
│   │   │   ├── tray.rs              # System tray
│   │   │   ├── notify.rs            # Desktop notifications
│   │   │   ├── copilot.rs           # Copilot engine
│   │   │   ├── signal_emitter.rs    # Signal emitter
│   │   │   ├── tool_executor.rs     # Local tool execution (screenshot, clipboard, file search)
│   │   │   └── uia_tree.rs          # UIA accessibility tree (stub)
│   │   ├── Cargo.toml
│   │   └── tauri.conf.json
│   ├── package.json
│   └── vite.config.ts
│
├── docs/
│   ├── protocol.md                  # WebSocket file transfer protocol spec
│   └── superpowers/                 # Design specs and implementation plans
│
├── README.md                        # Simplified Chinese
├── README.en.md                     # This file (English)
└── build.bat                        # Windows build script
```

---

## Protocol

The file transfer protocol uses a **3-frame WebSocket sequence**:

1. **Metadata** (JSON) — `{"type": "file_meta", "file_id": "...", "name": "...", "size": N}`
2. **Binary content** — raw file bytes
3. **End frame** (JSON) — `{"type": "file_end", "file_id": "...", "checksum": "sha256:..."}`

Client authentication uses pre-registered IDs with bcrypt-hashed passkeys. See [docs/protocol.md](docs/protocol.md) for the full specification.

---

## Security

- Passkeys are stored as **bcrypt hashes** — never in plaintext
- Authentication uses **constant-time comparison** to prevent timing attacks
- Clients must be **pre-registered** — unauthorized connections are rejected
- File integrity verified via **SHA256 checksums**
- ⚠️ Current transport is **ws:// (plain WebSocket)** — configure wss:// for production use

---

## systemd (Auto-Start)

### KOE Server

```ini
[Unit]
Description=KOE File Transfer Hub
After=network.target

[Service]
Type=simple
ExecStart=/usr/bin/python3 /path/to/koe/server/run_and_send.py 9765
WorkingDirectory=/path/to/koe/server
Restart=always
RestartSec=3
User=your-user

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable koe
sudo systemctl start koe
```

### ACP Bridge (opencode-bridge)

```ini
[Unit]
Description=ACP stdio-to-ws bridge for KOE
After=network.target

[Service]
Type=simple
ExecStart=npx @rebornix/stdio-to-ws --port 8766 -- /root/.opencode/bin/opencode acp
WorkingDirectory=/path/to/koe/server
Restart=always
RestartSec=3
User=your-user

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable opencode-bridge
sudo systemctl start opencode-bridge
```

---

## Roadmap

- [x] MCP file transfer (push files from LLM to client)
- [x] ACP chat (real-time bidirectional conversation)
- [x] Remote tools (screenshot, clipboard, file search)
- [x] Floating character overlay
- [x] Multi-column message bubbles
- [ ] End-to-end encryption (wss://)
- [ ] Client-to-server file push (bidirectional transfer)
- [ ] macOS client
- [ ] Plugin system for custom client tools

---

## Contributing

Contributions are welcome! This project is in active development. Feel free to open issues and pull requests.

---

## License

MIT
