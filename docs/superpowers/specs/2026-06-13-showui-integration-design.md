# ShowUI 集成设计

## 概述

将 [ShowUI](https://github.com/showlab/ShowUI)（Vision-Language-Action 模型）集成到 kaya-transfer-hub，
替代当前 copilot loop 中的 `@vision` 子模型。ShowUI 负责 GUI 理解（图标、布局、按钮语义）和
操作坐标输出，补充 PaddleOCR 只读文字的能力短板。

## 架构

```
┌──────────────┐     MCP      ┌──────────────────┐    HTTP     ┌──────────────┐
│  Kaya Agent  │ ──────────→  │  kaya-server     │ ──────────→ │  ShowUI API  │
│  (opencode)  │ ←──────────  │  (showui_query   │ ←────────── │  (独立部署)   │
└──────────────┘              │   MCP 工具)      │             └──────────────┘
                              └──────────────────┘
                                     │ 配置
                                     ▼
                              ┌──────────────────┐
                              │  showui_api_url   │
                              │  (configurable)  │
                              └──────────────────┘
```

### 组件职责

| 组件 | 职责 |
|------|------|
| **ShowUI 服务** | 独立部署，暴露 HTTP API。用户决定跑在哪台机器上 |
| **kaya-server MCP 工具** | 新增 `showui_query` 工具，将截图+问题转发到 ShowUI API |
| **copilot signal handler** | 更新指令模板，vision 部分改为优先用 `showui_query` |
| **配置** | `showui_api_url` 存在服务端配置中，可动态修改 |

### 与当前架构的关系

```
改前:
  OCR 不够 → @vision (服务端固定多模态模型)

改后:
  OCR 不够 → showui_query (可配置 API 端点)
```

## ShowUI API 定义

### Grounding（定位）

```http
POST /showui
Content-Type: multipart/form-data

image: <截图文件>
query: "搜索框在哪里"
mode: "grounding"
```

```json
{
  "success": true,
  "mode": "grounding",
  "position": [0.49, 0.42],
  "response": "[0.49, 0.42]"
}
```

### Describe（描述）

```http
POST /showui
Content-Type: multipart/form-data

image: <截图文件>
query: "描述这个窗口的内容"
mode: "describe"
```

```json
{
  "success": true,
  "mode": "describe",
  "response": "这是一个文件管理窗口，左侧是目录树，右侧显示文件列表..."
}
```

## MCP 工具定义

在 kaya-server 中新增工具：

```
名称: showui_query
参数:
  path: string         # 截图本地路径（服务端可读）
  query: string        # 自然语言问题
  mode: string         # "grounding" | "describe"（默认 "describe"）
返回:
  response: string     # ShowUI 的回答或坐标描述
  position?: [number, number]  # grounding 模式下的坐标
```

## copilot loop 更新

当前指令模板（`signal_handlers.py`）第 3 步改为：

```
3. 分析截图内容:
   a. OCR 读文字（优先）
   b. OCR 不够 → 调用 showui_query:
      - 对整体界面:  mode=describe, query="描述这个窗口的内容"
      - 找特定元素:  mode=grounding, query="XX按钮在哪里"
   c. 拿到坐标后直接执行操作
```

## 部署方式（用户可选）

### 方式一：Hugging Face Spaces API（推荐体验，零部署）

```bash
pip install gradio-client
```

```python
from gradio_client import Client, handle_file
client = Client("showlab/ShowUI")
result = client.predict(
    image=handle_file("screenshot.png"),
    query="描述这个窗口",
    api_name="/on_submit"
)
```

无 GPU 需求，但有网络延迟。

### 方式二：本地 GPU 推理（Windows/Linux）

```bash
pip install transformers torch qwen-vl-utils
```

模型权重下载到本地，Qwen2-VL-2B-Instruct 底座 + ShowUI-2B LoRA/全量微调权重。
需要 ~8GB VRAM。

### 方式三：vLLM 部署（生产推荐）

```bash
pip install vllm
python -m vllm.entrypoints.openai.api_server \
    --model showlab/ShowUI-2B \
    --port 8000
```

通过 OpenAI 兼容 API 调用，适合多并发。

## ShowUI API 封装层

由于 ShowUI 原生接口是 Gradio 或 vLLM，需要一个轻量封装层统一暴露为 REST API：

```
showui-server/
├── requirements.txt    # transformers, torch, gradio-client 或 vllm
├── server.py           # FastAPI 应用
│   ├── POST /showui    # 主入口
│   ├── GET  /health    # 健康检查
│   └── 根据后端配置切换推理方式
├── config.yaml         # 后端类型（hf/gradio/vllm/local）
└── backends/
    ├── gradio.py       # Hugging Face Spaces 远程调用
    ├── vllm.py         # vLLM OpenAI 兼容接口
    └── local.py        # transformers 本地推理
```

### API 封装 server.py 核心逻辑

```python
@app.post("/showui")
async def showui(image: UploadFile, query: str, mode: str = "describe"):
    img_bytes = await image.read()
    
    if mode == "grounding":
        # 使用 grounding prompt
        system = "Based on the screenshot, I give a text description and you give its location..."
        response = await backend.predict(img_bytes, system + query)
        # 解析 [x, y] 坐标
        return {"mode": "grounding", "position": parse_coord(response), "response": response}
    else:
        # 使用描述 prompt
        system = "Describe what this window is showing in detail."
        response = await backend.predict(img_bytes, system + query)
        return {"mode": "describe", "response": response}
```

## 配置

在 kaya-server 中新增配置项：

```json
{
  "showui": {
    "enabled": false,
    "api_url": "http://localhost:8001/showui",
    "timeout": 30
  }
}
```

## 数据流（完整 copilot 循环）

```
用户按 Ctrl+Alt+S
  → copilot_query signal
  → Kaya 收到指令模板
  → 循环:
    1. get_signal_status → active?
    2. take_screenshot
    3. a. OCR 读文字
       b. OCR 不够 → showui_query(mode="describe")
          → ShowUI 返回界面描述
       c. 需要定位 → showui_query(mode="grounding", query="XX在哪里")
          → ShowUI 返回坐标
    4. mouse_click / type_text / key_press
    5. wait 2s → 回 1
```

## 安全边界

- ShowUI API 调用超时 30s，防止挂起
- 截图文件用完后清理
- API URL 支持 http/https，用户自己保证传输安全
- 失败降级：ShowUI 不可用时回退到原来的 `@vision` 或直接报错提示用户配置

## 未涵盖的范围（后续迭代）

- ShowUI 本地模型自动下载（参考 PaddleOCR 的 auto_download）
- 多模态模型切换（不只 ShowUI，可以换成 UI-TARS 等）
- Windows 客户端内置 ShowUI 推理引擎

## 规格自检

- [x] 无占位符/TODO
- [x] 架构描述一致
- [x] 范围聚焦：ShowUI 集成 + API 封装层，不涉及其他改动
- [x] 部署方式三种方案清晰列出
