<p align="center">
  <h1 align="center">kaya-transfer-hub</h1>
  <p align="center">
    <em>Bring LLM to your desktop — file transfer, real-time chat, remote tools</em>
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
Kaya (OpenCode) → MCP stdio → kaya-transfer-hub (Python)
  ├─ WS 9765 → file transfer, signals, tool proxy
  └─ ACP 8765 → native Python bridge (session persistence)
       ↓
  Tauri 2 Windows Client (Rust + Vue 3)
```

## Quick Start

### Server

```bash
mkdir -p /opt/kaya_transfer_hub
cp -r server/src/kaya_transfer_hub /opt/kaya_transfer_hub/
cp -r server/src/kaya_server /opt/kaya_transfer_hub/
cp server/pyproject.toml /opt/kaya_transfer_hub/
cp server/run_and_send.py /opt/kaya_transfer_hub/kaya_server/

cd /opt/kaya_transfer_hub
sed -i 's/where = \["src"\]/where = ["."]/' pyproject.toml
pip install -e .

python3 kaya_server/run_and_send.py 9765 8765
```

### Windows Client

```bash
cd client
npm install
npm run tauri build
```

## License

MIT
