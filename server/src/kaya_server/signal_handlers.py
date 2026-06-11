"""共享信号处理器。

由 run_and_send.py 和 cli.py serve 命令共用。
每个 handler 签名: (client_id: str, data: dict) -> str | None
返回 None 表示不通知，返回字符串则作为 acp_inject 文本发送。
"""


def visual_input_handler(client_id: str, data: dict) -> str | None:
    source = data.get("source", "unknown")
    ts = data.get("timestamp", "")
    hint = data.get("hint", "none")
    text = f"[系统] 客户端 {client_id} 报告新的视觉输入可用 (来源: {source}, 时间: {ts})"
    if hint == "view":
        text += "。你可以通过 call_client_tool 获取截图查看。"
    return text


def clipboard_handler(client_id: str, data: dict) -> str | None:
    return (
        f"[系统] 客户端 {client_id} 剪贴板内容已变更。"
        f"如需读取，请调用 call_client_tool('{client_id}', 'get_clipboard', {{}})。"
    )


def copilot_query_handler(client_id: str, data: dict) -> str | None:
    question = data.get("question", "").strip()
    uia_tree = data.get("uia_tree", "")
    mode = data.get("mode", "single")
    window_rect = data.get("window_rect", {})
    screenshot_path = data.get("screenshot_path")

    lines = [f"🧑 **用户提问**: {question}"]
    lines.append(f"⚡ **模式**: {'持续监测' if mode == 'continuous' else '单次查询'}")

    if uia_tree:
        lines.append(f"\n📋 **当前窗口结构**:\n```\n{uia_tree[:3000]}\n```")

    if window_rect:
        x, y, w, h = window_rect.get("x", 0), window_rect.get("y", 0), window_rect.get("width", 0), window_rect.get("height", 0)
        lines.append(f"\n📐 **窗口屏幕坐标**: x={x} y={y} width={w} height={h}")
        lines.append(f"  截图时使用: take_screenshot(x={x}, y={y}, width={w}, height={h})")

    if screenshot_path:
        lines.append(f"\n📷 **截图路径**: {screenshot_path}")

    if mode == "continuous":
        x = window_rect.get("x", 0)
        y = window_rect.get("y", 0)
        w = window_rect.get("width", 0)
        h = window_rect.get("height", 0)
        lines.append(
            "\n\n🔄 **持续监测循环** — 你需要循环执行直到用户取消（按 Esc）:\n"
            "```\n"
            "loop:\n"
            f"  1. 调用 get_signal_status(client_id='{client_id}', signal_name='copilot_query')\n"
            "     如果返回 inactive → 退出循环，回复\"监测已结束\"\n"
            f"  2. 调用 call_client_tool(client_id='{client_id}', tool_name='take_screenshot', arguments={{\"x\": {x}, \"y\": {y}, \"width\": {w}, \"height\": {h}}})\n"
            "     获取当前窗口区域截图\n"
            "  3. 分析截图内容:\n"
            f"     a. 从 tool_result 里提取本地路径，调用 call_client_tool(client_id='{client_id}', tool_name='ocr_region', arguments={{\"path\": \"本地路径\"}})\n"
            "        如果返回的文字足够理解内容 → 直接进入步骤 4\n"
            "     b. 如果 OCR 结果太少或不相关（窗口含大量图标/图片）:\n"
            "        再用 @vision 服务端路径 做视觉分析作为补充\n"
            "  4. 根据分析结果执行操作:\n"
            f"     - 输入文字: call_client_tool(client_id='{client_id}', tool_name='type_text', arguments={{\"text\": \"...\"}})\n"
            f"     - 点击: call_client_tool(client_id='{client_id}', tool_name='mouse_click', arguments={{\"x\": N, \"y\": N}})\n"
            f"     - 按键: call_client_tool(client_id='{client_id}', tool_name='key_press', arguments={{\"keys\": \"enter\"}})\n"
            "  5. 等待 2 秒后回到步骤 1\n"
            "```\n"
            "⚠️ 每次循环只能执行一次操作，然后立刻回到步骤 1 检查信号状态。\n"
            "⚠️ OCR 优先于 vision：文字内容用 OCR 读又快又准，只有图标/按钮/布局才用 vision。"
        )
    else:
        x = window_rect.get("x", 0)
        y = window_rect.get("y", 0)
        w = window_rect.get("width", 0)
        h = window_rect.get("height", 0)
        lines.append(
            "\n\n⚡ **行动指令**:\n"
            "1. 先分析 UIA 树能否直接回答问题。如果能，直接回复并在末尾标注 `[source: uia]`。\n"
            f"2. 如果不能，调用 call_client_tool(client_id='{client_id}', tool_name='take_screenshot', arguments={{\"x\": {x}, \"y\": {y}, \"width\": {w}, \"height\": {h}}})"
            " 截取窗口区域图。\n"
            "3. 从 tool_result 里提取本地路径，再用 ocr_region 读文字。\n"
            "4. 只有 OCR 不够时（如图标/按钮/布局判断）才用 @vision 分析。\n"
            "5. **完成后必须回复一段文字**，总结你看到了什么、做了什么，末尾标注 `[source: ocr]` 或 `[source: vision]`。"
        )

    return "\n".join(lines)


def check_acp_health_handler(client_id: str, data: dict) -> str | None:
    """诊断 opencode acp 进程是否健康（无阻塞，纯即时检查）。

    返回格式:
      HEALTH:healthy:<detail>   — 进程存在
      HEALTH:dead:<reason>      — 进程不存在
    """
    import subprocess

    result = subprocess.run(
        ["pgrep", "-f", "opencode acp"],
        capture_output=True, text=True, timeout=5,
    )
    pids_text = result.stdout.strip()
    if not pids_text:
        return "HEALTH:dead:no opencode acp process found"

    pids = [int(p) for p in pids_text.split() if p.strip()]

    details = []
    for pid in pids:
        try:
            with open(f"/proc/{pid}/stat") as f:
                fields = f.read().split()
                state = fields[2]        # R/S/D/Z/T
                utime = int(fields[13])
                stime = int(fields[14])
                rss_pages = int(fields[23])
                rss_mb = rss_pages * 4 // 1024
            details.append(f"pid={pid} state={state} utime={utime} stime={stime} RSS={rss_mb}MB")
        except Exception:
            details.append(f"pid={pid} stat unreadable")

    return f"HEALTH:healthy:{'; '.join(details)}"


def register_all(tool_registry):
    """在 tool_registry 上注册所有默认信号处理器。"""
    tool_registry.register_signal_handler("visual_input_available", visual_input_handler)
    tool_registry.register_signal_handler("clipboard_changed", clipboard_handler)
    tool_registry.register_signal_handler("copilot_query", copilot_query_handler)
    tool_registry.register_signal_handler("check_acp_health", check_acp_health_handler)
