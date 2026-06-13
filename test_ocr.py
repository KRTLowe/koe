"""
PP-OCRv6 ONNX 本地测试脚本
用法: python3 test_ocr.py <图片路径>
"""

import sys
import cv2
import numpy as np
import onnxruntime as ort
from PIL import Image, ImageDraw


# ── 模型路径 ────────────────────────────────────────
DET_ONNX = "models/PaddleOCR/PP-OCRv6_medium_det/inference.onnx"
REC_ONNX = "models/PaddleOCR/PP-OCRv6_medium_rec/inference.onnx"
REC_YML  = "models/PaddleOCR/PP-OCRv6_medium_rec/inference.yml"


# ── 检测预处理（NCHW 布局） ──────────────────────────
def preprocess_det(bgr_img: np.ndarray, max_side=1024):
    """bgr_img: HWC BGR numpy array (from cv2.imread)"""
    h, w, _ = bgr_img.shape
    # DetResizeForTest default: limit_side_len=736, limit_type="min"
    # 对于 >736 的图 ratio=1.0，只做 32-padding
    ratio = 1.0
    nh = max(int(round(h * ratio / 32) * 32), 32)
    nw = max(int(round(w * ratio / 32) * 32), 32)

    # resize 到 32 倍数
    if (h, w) != (nh, nw):
        bgr_img = cv2.resize(bgr_img, (nw, nh))

    # NormalizeImage: scale=1/255, mean=[0.485,0.456,0.406], std=[0.229,0.224,0.225], order=hwc
    img = bgr_img.astype(np.float32) / 255.0
    mean = np.array([0.485, 0.456, 0.406], dtype=np.float32).reshape(1, 1, 3)
    std = np.array([0.229, 0.224, 0.225], dtype=np.float32).reshape(1, 1, 3)
    img = (img - mean) / std

    # ToCHWImage
    img = img.transpose((2, 0, 1))  # HWC → CHW

    return img[np.newaxis, :]  # [1, 3, H, W]


# ── 检测后处理（连通域 + NMS） ──────────────────────
def dbnet_postprocess(prob, orig_size, thresh=0.3):
    H, W = prob.shape
    sx = orig_size[0] / W
    sy = orig_size[1] / H

    binary = (prob > thresh).astype(np.uint8)
    # 简单连通域标记（替代 scipy.ndimage.label，避免额外依赖）
    labeled = np.zeros_like(binary, dtype=np.int32)
    num_features = 0
    for y in range(H):
        for x in range(W):
            if binary[y, x] and labeled[y, x] == 0:
                num_features += 1
                stack = [(y, x)]
                labeled[y, x] = num_features
                while stack:
                    cy, cx = stack.pop()
                    for dy, dx in [(0,1),(0,-1),(1,0),(-1,0)]:
                        ny, nx = cy + dy, cx + dx
                        if 0 <= ny < H and 0 <= nx < W and binary[ny, nx] and labeled[ny, nx] == 0:
                            labeled[ny, nx] = num_features
                            stack.append((ny, nx))

    boxes = []
    for i in range(1, num_features + 1):
        ys, xs = np.where(labeled == i)
        if len(xs) < 3 or len(ys) < 3:
            continue
        x1, x2 = int(xs.min() * sx), int(xs.max() * sx)
        y1, y2 = int(ys.min() * sy), int(ys.max() * sy)
        if x2 - x1 >= 3 and y2 - y1 >= 3:
            boxes.append([x1, y1, x2, y2])

    # 按面积排序
    boxes.sort(key=lambda b: (b[2] - b[0]) * (b[3] - b[1]), reverse=True)

    # 简单 NMS
    keep = []
    for b in boxes:
        suppressed = False
        for k in keep:
            ix = max(b[0], k[0])
            iy = max(b[1], k[1])
            iw = max(0, min(b[2], k[2]) - ix)
            ih = max(0, min(b[3], k[3]) - iy)
            inter = iw * ih
            union = (b[2]-b[0])*(b[3]-b[1]) + (k[2]-k[0])*(k[3]-k[1]) - inter
            if union > 0 and inter / union > 0.5:
                suppressed = True
                break
        if not suppressed:
            keep.append(b)
    return keep


# ── 识别预处理（PP-OCRv6: H=48, 3ch, [-1,1]） ────────
def preprocess_rec(crop_bgr: np.ndarray):
    """crop_bgr: HWC BGR numpy array"""
    REC_H = 48
    REC_MAX_W = 320

    h, w, _ = crop_bgr.shape
    scale = REC_H / h
    tw = min(int(w * scale), REC_MAX_W)
    tw = max(tw, 4)

    resized = cv2.resize(crop_bgr, (tw, REC_H))
    # PP-OCRv6 rec: value/127.5 - 1.0 → [-1, 1]
    data = resized.astype(np.float32) / 127.5 - 1.0

    # NCHW, pad to REC_MAX_W
    chw = np.zeros((3, REC_H, REC_MAX_W), dtype=np.float32)
    chw[:, :, :tw] = data.transpose(2, 0, 1)

    return chw[np.newaxis, :]  # [1, 3, 48, 320]


# ── CTC 解码 ────────────────────────────────────────
def ctc_decode(output, chars):
    """output: [T, C], chars: list of str (index 0 = blank)"""
    preds = output.argmax(axis=1)
    result = []
    prev = 0
    for p in preds:
        if p != 0 and p != prev:
            if p < len(chars):
                result.append(chars[p])
        prev = p
    return "".join(result)


# ── 解析 character_dict 从 inference.yml ────────────
def parse_char_dict(yml_path):
    import yaml
    with open(yml_path, encoding="utf-8") as f:
        data = yaml.safe_load(f)
    raw = data["PostProcess"]["character_dict"]
    chars = [""] + raw  # 索引 0 = blank
    # 补到 18710 防止越界
    while len(chars) < 18710:
        chars.append("")
    return chars


# ── 主流程 ──────────────────────────────────────────
def main(image_path):
    print(f"[*] 加载图片: {image_path}")
    img_bgr = cv2.imread(image_path, cv2.IMREAD_COLOR)
    if img_bgr is None:
        print("[!] OpenCV 无法读取图片")
        sys.exit(1)
    h, w, _ = img_bgr.shape
    orig_size = (w, h)
    print(f"    尺寸: {orig_size}  BGR shape: {img_bgr.shape}")

    # 加载 ONNX 模型
    print("[*] 加载 ONNX 模型...")
    so = ort.SessionOptions()
    so.log_severity_level = 3

    det_session = ort.InferenceSession(DET_ONNX, so)
    rec_session = ort.InferenceSession(REC_ONNX, so)

    # 检测输入名
    det_input_name = det_session.get_inputs()[0].name
    rec_input_name = rec_session.get_inputs()[0].name

    print(f"    检测输入: {det_input_name} {det_session.get_inputs()[0].shape}")
    print(f"    识别输入: {rec_input_name} {rec_session.get_inputs()[0].shape}")
    print(f"    识别输出: {rec_session.get_outputs()[0].shape}")

    # 解析字符字典
    print(f"[*] 解析字符字典: {REC_YML}")
    chars = parse_char_dict(REC_YML)
    print(f"    共 {len(chars)} 项")

    # ── 检测 ──
    print("[*] 检测文字区域...")
    det_input = preprocess_det(img_bgr)
    det_out = det_session.run(None, {det_input_name: det_input})[0]
    # ONNX 模型内部已含 sigmoid，输出就是概率
    prob = det_out[0, 0]
    print(f"    输出概率图: min={prob.min():.4f} max={prob.max():.4f} mean={prob.mean():.4f}")
    print(f"    大于 0.3 的像素: {(prob > 0.3).sum()} / {prob.size}")
    print(f"    大于 0.01 的像素: {(prob > 0.01).sum()} / {prob.size}")

    boxes = dbnet_postprocess(prob, orig_size, thresh=0.2)
    print(f"    找到 {len(boxes)} 个文本区域 (thresh=0.2)")
    if not boxes:
        print("[!] 未检测到任何文字")
        print("    尝试更低阈值...")
        boxes = dbnet_postprocess(prob, orig_size, thresh=0.1)
        print(f"    找到 {len(boxes)} 个区域 (thresh=0.1)")
        if not boxes:
            print("    仍无结果，可能预处理或模型文件有问题")
            sys.exit(1)

    # ── 识别 ──
    print("[*] 识别文字...")
    json_mode = "--json" in sys.argv
    results = []
    for i, box in enumerate(boxes):
        x1, y1, x2, y2 = box
        crop_bgr = img_bgr[y1:y2, x1:x2]
        ch = crop_bgr.shape[0]
        cw = crop_bgr.shape[1]
        if not json_mode:
            print(f"    [{i}] 裁剪区域: {cw}x{ch}")
        if cw < 4 or ch < 4:
            if not json_mode:
                print(f"        跳过: 区域太小")
            continue

        rec_input = preprocess_rec(crop_bgr)
        rec_out = rec_session.run(None, {rec_input_name: rec_input})[0]
        txt = ctc_decode(rec_out[0], chars)
        if txt:
            results.append({"box": [x1, y1, x2, y2], "text": txt})
            if not json_mode:
                print(f"        ✅ {txt}")

    if json_mode:
        import json
        print(json.dumps({"results": results, "count": len(results)}, ensure_ascii=False, indent=2))
    else:
        # ── 可视化 ──
        vis = img_bgr.copy()
        for r in results:
            x1, y1, x2, y2 = r["box"]
            cv2.rectangle(vis, (x1, y1), (x2, y2), (0, 0, 255), 2)
            cv2.putText(vis, r["text"], (x1, y1 - 8), cv2.FONT_HERSHEY_SIMPLEX, 0.5, (0, 0, 255), 1)
        out_path = "ocr_result.png"
        cv2.imwrite(out_path, vis)
        print(f"\n[*] 结果已保存: {out_path}")
        print(f"[*] 共识别 {len(results)} 段文字")


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--check-dict":
        chars = parse_char_dict(REC_YML)
        print(f"字典共 {len(chars)} 项")
        for idx in [16, 34, 37, 3219, 3248, 17673]:
            print(f"  chars[{idx}] = '{chars[idx]}' (len={len(chars[idx])})" if idx < len(chars) else f"  chars[{idx}] = index 越界")
        sys.exit(0)

    if len(sys.argv) < 2:
        print("用法: python3 test_ocr.py <图片路径>")
        sys.exit(1)
    main(sys.argv[1])
