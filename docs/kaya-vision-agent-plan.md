# Kaya Vision Agent — 课题设计

## 背景

kaya-transfer-hub 已在 copilot monitoring loop 中实现了截图 → OCR → 操作的流程。
当前 vision 层依赖 PaddleOCR（读文字）和 @vision 服务端多模态模型（理解界面）。
这个课题的目标是用 ShowUI / Qwen2.5-VL 替代 @vision，用 UIA 树做操作校验，
形成一个可配置、可落地、端到端的桌面 GUI Agent 方案。

---

## 整体架构

```
┌──────────────┐     MCP      ┌──────────────────┐
│  Kaya Agent  │ ←──────────→ │  kaya-server      │
│  (opencode)  │              │                   │
└──────────────┘              └──────────────────┘
       │                            │
       │ call_client_tool           │ showui_query (MCP)
       ▼                            ▼
┌──────────────────┐      ┌──────────────────┐
│  Windows 客户端    │      │  ShowUI API      │
│                   │      │  (独立部署)        │
│  take_screenshot  │      │                  │
│  ocr_region       │      │  Grounding       │
│  uia_tree         │      │  Describe        │
│  mouse_click      │      │  Drag (ShowUI-π) │
│  type_text        │      │                  │
└──────────────────┘      └──────────────────┘
```

---

## 子课题清单

### 课题 A: ShowUI API 封装层

**目标**：将 ShowUI（或 Qwen2.5-VL）包装成可配置的 HTTP API，替换当前 @vision。

**交付物**：
- `showui-server/` 目录，独立于 kaya-transfer-hub 部署
- Flask/FastAPI 服务，暴露 `POST /showui` 和 `GET /health`
- 支持三种后端：Hugging Face Gradio / vLLM / transformers 本地推理
- `mode=describe`：截图 + 问题 → 返回文字描述
- `mode=grounding`：截图 + "找某元素" → 返回 `[x, y]` 坐标

**关键决策**：
- 后端抽象层设计（`backends/gradio.py`, `backends/vllm.py`, `backends/local.py`）
- 请求/响应格式
- 超时、错误处理、降级

**测试点**：
- 三种后端模式都能返回有效结果
- grounding 坐标在截图范围内
- describe 返回内容与截图相关
- 超时/断连时优雅降级

---

### 课题 B: showui_query MCP 工具

**目标**：在 kaya-server 中新增 MCP 工具 `showui_query`，Kaya Agent 可直接调用。

**交付物**：
- `server/src/kaya_server/tools/showui_query.py`（或集成到现有工具注册机制）
- 工具定义：名称、参数、返回格式
- 调用 `showui-server` HTTP API
- 配置项 `showui.api_url`（可动态修改）

**工作流程**：
```
Kaya Agent:
  showui_query(path="screenshot.png", query="描述这个窗口", mode="describe")
  → kaya-server 转发到 ShowUI API
  → 返回 {"response": "这是一个文件管理器窗口..."}
```

**测试点**：
- MCP 工具注册成功
- 调用 ShowUI API 成功并返回格式化结果
- 配置为空/不可用时返回友好错误
- 截图文件路径不存在时处理得当

---

### 课题 C: ShowUI-π 拖拽能力集成

**目标**：为 copilot loop 增加拖拽操作能力（ShowUI-π，450M 参数）。

**交付物**：
- `showui-server` 中扩展 `mode=drag`：截图 + "从 A 拖到 B" → 返回拖拽路径
- 客户端新增 `mouse_drag` 工具（start_x, start_y, end_x, end_y）
- copilot loop 指令模板中增加拖拽操作说明

**测试点**：
- drag 模式返回有效的拖拽轨迹
- `mouse_drag` 执行拖拽操作
- 拖拽后截图确认操作效果

---

### 课题 D: UIA 操作校验

**目标**：用 UIA 树替代纯视觉判断操作是否成功，解决"错一步后面全偏"问题。

**交付物**：
- copilot loop 中新增"操作前 UIA 快照 → 操作 → 操作后 UIA 快照对比"流程
- 或在 `showui_query` 中集成 UIA 信息作为辅助输入
- 指令模板更新，让 Kaya 学会用 UIA 做断言

**工作流程**：
```
1. Kaya 决定"点新建网格按钮"
2. UIA 查 Button[name=新建网格] → 确认存在 + 拿到坐标 + 记录当前状态
3. 执行 click(x, y)
4. UIA 再查同个元素 → 状态变了？新元素出现了？
5. 一致 → 继续；不一致 → 重试/降级到截图分析
```

**测试点**：
- UIA 能精确找到有 name/automationId 的控件
- 操作后 UIA 状态变化能被检测到
- UIA 找不到元素时降级到视觉定位
- 复杂界面（嵌套、自定义控件）的容错

---

### 课题 E: Copilot Loop 整合

**目标**：将 A-D 整合进完整的 copilot monitoring loop。

**交付物**：
- 更新 `signal_handlers.py` 中的指令模板
- 完整的 loop 流程：

```
loop:
  1. get_signal_status → active?
  
  2. take_screenshot + uia_tree（并行获取）
  
  3. Kaya 分析:
     a. ShowUI describe → 理解界面全局
     b. UIA 树 → 确定可用控件
  
  4. Kaya 决定下一步操作
  
  5. 定位:
     a. UIA 查 automationId/name → 精确坐标（优先）
     b. ShowUI grounding → 视觉坐标（回退）
  
  6. 执行:
     a. mouse_click / type_text / key_press
     b. mouse_drag（ShowUI-π）
  
  7. 校验:
     a. UIA 查元素状态变化（优先）
     b. 截图 → ShowUI describe 比对（回退）
  
  8. 不一致 → 重试 / 上报异常
     一致 → wait 2s → 回 1
```

**测试点**：
- 完整流程在简单场景（浏览器操作）能跑通
- 操作失败后能正确重试
- UIA 降级到视觉后仍有可用性
- 50+ 步长任务不累积错误

---

### 课题 F: Live2D 建模可行性验证

**目标**：验证整套系统在 Live2D Cubism Editor 上的实际可用性。

**范围**：
- 不要求完整走完一个建模流程
- 验证关键环节的可行性：界面理解、工具切换、画布操作、参数绑定
- 记录失败模式和瓶颈

**测试点**：
- ShowUI/Qwen2.5-VL 能识别 Cubism Editor 的主要面板
- 通过 UIA 能定位 Cubism 的工具栏按钮
- 画布上的网格拖拽操作可执行
- 工作流中每一步的 UIA 校验能生效
- 记录：哪些环节成功、哪些失败、瓶颈在哪

---

## 依赖关系

```
课题 A (API 封装) ──→ 课题 B (MCP 工具) ──→ 课题 E (Loop 整合)
                                                 ↑
课题 C (拖拽能力) ─────────────────────────────────┘
                                                 ↑
课题 D (UIA 校验) ────────────────────────────────┘
                                                 ↑
课题 F (Live2D 验证) ─────────────────────────────┘
```

A 和 B 是前置依赖。
C 和 D 可并行。
E 需要 A+B+C+D 完成后整合。
F 是验收课题，需要 E 完成后执行。

---

## 技术栈

| 组件 | 技术选型 |
|------|---------|
| ShowUI API 封装 | Python FastAPI + httpx |
| 推理后端 | transformers / vLLM / gradio-client |
| MCP 工具 | Python（现有 kaya-server 框架） |
| Windows 远程工具 | Rust（已有 `mouse_click`，新增 `mouse_drag`） |
| UIA 树抓取 | Rust `windows` crate（已有 `uia_tree` 工具） |
| 操作校验逻辑 | Kaya Agent prompt 模板 + UIA 前后对比 |
| ShowUI 模型 | ShowUI-2B / Qwen2.5-VL-7B / ShowUI-π |

---

## 未涵盖的范围

- ShowUI 模型训练/微调（直接用开源权重）
- 多用户并发场景
- 非 Windows 平台支持
- 长时间运行的任务持久化与断点恢复
