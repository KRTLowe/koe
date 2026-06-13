// ── 模型路径 ────────────────────────────────────────
const DET_ONNX: &str = "models/PaddleOCR/PP-OCRv6_medium_det/inference.onnx";
const REC_ONNX: &str = "models/PaddleOCR/PP-OCRv6_medium_rec/inference.onnx";
const REC_YML:  &str = "models/PaddleOCR/PP-OCRv6_medium_rec/inference.yml";

// ── 字符字典解析 ────────────────────────────────────
fn parse_char_dict(path: &str) -> Vec<String> {
    let content = std::fs::read_to_string(path).expect("读取 inference.yml 失败");
    let value: serde_yaml::Value = serde_yaml::from_str(&content).expect("解析 YAML 失败");
    let raw = value["PostProcess"]["character_dict"]
        .as_sequence()
        .expect("未找到 character_dict");

    let mut keys = vec![String::new()]; // 索引 0 = blank
    for v in raw {
        keys.push(v.as_str().unwrap_or("").to_string());
    }
    keys.resize(18710, String::new());
    keys
}

fn det_max_side(img_max: u32) -> u32 {
    match img_max {
        0..=1200 => img_max,
        1201..=2000 => 1536,
        _ => 1920,
    }
}

// ── 检测预处理 ──────────────────────────────────────
fn preprocess_det(img: &image::DynamicImage, max_side: u32) -> (Vec<f32>, Vec<i64>) {
    let (w, h) = (img.width(), img.height());
    let scale = ((max_side as f64) / (w.max(h) as f64)).min(1.0);
    let nw = ((w as f64 * scale).round() as u32).max(32);
    let nh = ((h as f64 * scale).round() as u32).max(32);
    let rw = ((nw + 31) / 32) * 32;
    let rh = ((nh + 31) / 32) * 32;

    let resized = img.resize_exact(rw, rh, image::imageops::FilterType::Lanczos3);
    let rgb = resized.to_rgb8();

    let total = (rw * rh) as usize;
    let mut data = vec![0.0f32; 3 * total];
    for y in 0..rh {
        for x in 0..rw {
            let px = rgb.get_pixel(x, y);
            let idx = (y * rw + x) as usize;
            data[idx]            = (px[2] as f32 / 255.0 - 0.485) / 0.229; // B
            data[total + idx]    = (px[1] as f32 / 255.0 - 0.456) / 0.224; // G
            data[2 * total + idx] = (px[0] as f32 / 255.0 - 0.406) / 0.225; // R
        }
    }
    (data, vec![1, 3, rh as i64, rw as i64])
}

fn box_confidence(
    output: &[f32], out_w: usize, out_h: usize,
    x1: usize, y1: usize, x2: usize, y2: usize,
) -> f32 {
    let x1 = x1.min(out_w - 1);
    let x2 = x2.min(out_w - 1);
    let y1 = y1.min(out_h - 1);
    let y2 = y2.min(out_h - 1);
    if x2 <= x1 || y2 <= y1 { return 0.0; }
    let mut sum = 0.0f32;
    let mut cnt = 0;
    for by in y1..=y2 {
        let row = by * out_w;
        for bx in x1..=x2 {
            sum += output[row + bx];
            cnt += 1;
        }
    }
    if cnt > 0 { sum / cnt as f32 } else { 0.0 }
}

// ── 检测后处理（DBNet 连通域 + Unclip）───────────────
fn dbnet_postprocess(
    output: &[f32], shape: &[usize],
    orig_w: u32, orig_h: u32,
    threshold: f32, unclip_ratio: f32, box_thresh: f32,
) -> Vec<[i32; 4]> {
    let (h, w) = if shape.len() == 4 && shape[1] == 1 {
        (shape[2], shape[3])
    } else {
        return vec![];
    };
    let sx = orig_w as f32 / w as f32;
    let sy = orig_h as f32 / h as f32;
    let mut visited = vec![false; h * w];
    let mut boxes: Vec<[i32; 4]> = Vec::new();

    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            if visited[idx] { continue; }
            if output[idx] < threshold { visited[idx] = true; continue; }

            let (mut x1, mut x2, mut y1, mut y2) = (x, x, y, y);
            let mut q = vec![(x, y)];
            visited[idx] = true;
            while let Some((cx, cy)) = q.pop() {
                x1 = x1.min(cx); x2 = x2.max(cx);
                y1 = y1.min(cy); y2 = y2.max(cy);
                for &(dx, dy) in &[(0i32,-1),(0,1),(-1,0),(1,0)] {
                    let nx = cx as i32 + dx;
                    let ny = cy as i32 + dy;
                    if nx < 0 || nx >= w as i32 || ny < 0 || ny >= h as i32 { continue; }
                    let ni = ny as usize * w + nx as usize;
                    if visited[ni] { continue; }
                    if output[ni] >= threshold {
                        visited[ni] = true;
                        q.push((nx as usize, ny as usize));
                    }
                }
            }

            let conf = box_confidence(output, w, h, x1, y1, x2, y2);
            if conf < box_thresh { continue; }

            let bw = (x2 - x1 + 1) as f32;
            let bh = (y2 - y1 + 1) as f32;
            let area = bw * bh;
            let perimeter = 2.0 * (bw + bh);
            let dist = if perimeter > 0.0 {
                (area * unclip_ratio / perimeter).round() as i32
            } else {
                0
            };

            let nx1 = (x1 as i32 - dist).max(0) as usize;
            let ny1 = (y1 as i32 - dist).max(0) as usize;
            let nx2 = (x2 as i32 + dist).min(w as i32 - 1) as usize;
            let ny2 = (y2 as i32 + dist).min(h as i32 - 1) as usize;

            let b = [
                (nx1 as f32 * sx).round() as i32,
                (ny1 as f32 * sy).round() as i32,
                (nx2 as f32 * sx).round() as i32,
                (ny2 as f32 * sy).round() as i32,
            ];
            let b = [
                b[0].max(0).min(orig_w as i32 - 1),
                b[1].max(0).min(orig_h as i32 - 1),
                b[2].max(0).min(orig_w as i32 - 1),
                b[3].max(0).min(orig_h as i32 - 1),
            ];
            if (b[2] - b[0]) >= 3 && (b[3] - b[1]) >= 3 {
                boxes.push(b);
            }
        }
    }

    // NMS
    boxes.sort_by(|a, b| ((b[2]-b[0])*(b[3]-b[1])).cmp(&((a[2]-a[0])*(a[3]-a[1]))));
    let mut keep: Vec<[i32; 4]> = Vec::new();
    for &b in &boxes {
        let mut sup = false;
        for &kb in &keep {
            let ix = b[0].max(kb[0]);
            let iy = b[1].max(kb[1]);
            let iw = (b[2].min(kb[2]) - ix).max(0);
            let ih = (b[3].min(kb[3]) - iy).max(0);
            let inter = (iw * ih) as f64;
            let union = ((b[2]-b[0])*(b[3]-b[1]) + (kb[2]-kb[0])*(kb[3]-kb[1])) as f64 - inter;
            if union > 0.0 && inter / union > 0.5 { sup = true; break; }
        }
        if !sup { keep.push(b); }
    }
    keep
}

// ── 识别预处理 ──────────────────────────────────────
const REC_H: u32 = 48;
const REC_MAX_W: u32 = 320;

fn preprocess_rec(img: &image::DynamicImage, box_: &[i32; 4]) -> Option<(Vec<f32>, Vec<i64>)> {
    let x = box_[0].max(0) as u32;
    let y = box_[1].max(0) as u32;
    let cw = (box_[2] - box_[0]).unsigned_abs().min(img.width() - x);
    let ch = (box_[3] - box_[1]).unsigned_abs().min(img.height() - y);
    if cw < 2 || ch < 2 { return None; }

    let cropped = img.crop_imm(x, y, cw.max(4), ch);
    let scale = REC_H as f64 / cropped.height() as f64;
    let tw = ((cropped.width() as f64 * scale).round() as u32).max(4).min(REC_MAX_W);
    let resized = cropped.resize_exact(tw, REC_H, image::imageops::FilterType::Lanczos3);
    let rgb = resized.to_rgb8();

    let mut data = vec![0.0f32; (3 * REC_H * REC_MAX_W) as usize];
    for y in 0..REC_H {
        for x in 0..tw {
            let px = rgb.get_pixel(x, y);
            let b = px[2] as f32 / 127.5 - 1.0;
            let g = px[1] as f32 / 127.5 - 1.0;
            let r = px[0] as f32 / 127.5 - 1.0;
            let ib = (0 * REC_H + y) * REC_MAX_W + x;
            let ig = (1 * REC_H + y) * REC_MAX_W + x;
            let ir = (2 * REC_H + y) * REC_MAX_W + x;
            data[ib as usize] = b;
            data[ig as usize] = g;
            data[ir as usize] = r;
        }
    }
    Some((data, vec![1, 3, REC_H as i64, REC_MAX_W as i64]))
}

// ── CTC 解码 ────────────────────────────────────────
fn ctc_decode(output: &[f32], shape: &[usize], keys: &[String]) -> String {
    // shape = [1, T, C] → (T, C)
    let (timesteps, classes) = if shape.len() == 3 && shape[0] == 1 {
        (shape[1], shape[2])
    } else {
        return String::new();
    };

    let mut result = Vec::new();
    let mut prev = 0usize;
    for t in 0..timesteps {
        let base = t * classes;
        let mut max_idx = 0usize;
        let mut max_v = -1e10f32;
        for c in 0..classes {
            let v = output[base + c];
            if v > max_v { max_v = v; max_idx = c; }
        }
        if max_idx != 0 && max_idx != prev {
            if let Some(s) = keys.get(max_idx) {
                if !s.is_empty() { result.push(s.clone()); }
            }
        }
        prev = max_idx;
    }
    result.concat()
}

// ── 主流程 ──────────────────────────────────────────
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("用法: test_ocr_rs.exe <图片路径>");
        std::process::exit(1);
    }
    let image_path = &args[1];

    println!("[*] 加载图片: {}", image_path);
    let img = image::open(image_path).expect("打开图片失败");
    println!("    尺寸: {}x{}", img.width(), img.height());

    // 加载 ONNX
    println!("[*] 加载检测模型: {}", DET_ONNX);
    let mut det = ort::session::Session::builder()
        .unwrap()
        .commit_from_file(DET_ONNX)
        .expect("检测模型加载失败");

    println!("[*] 加载识别模型: {}", REC_ONNX);
    let mut rec = ort::session::Session::builder()
        .unwrap()
        .commit_from_file(REC_ONNX)
        .expect("识别模型加载失败");

    println!("[*] 解析字符字典: {}", REC_YML);
    let keys = parse_char_dict(REC_YML);
    println!("    字符总数: {}", keys.len());

    // ── 检测 ──
    println!("[*] 检测文字区域...");
    let max_side = det_max_side(img.width().max(img.height()));
    println!("    自适应 max_side={}", max_side);
    let (det_data, det_shape) = preprocess_det(&img, max_side);
    let det_input = ort::value::Tensor::from_array((det_shape, det_data)).unwrap();
    let det_out = det.run(ort::inputs![det_input]).unwrap();
    let (det_out_shape, det_out_data) = det_out[0].try_extract_tensor::<f32>().unwrap();
    let shape: Vec<usize> = det_out_shape.iter().map(|&d| d as usize).collect();
    let data: Vec<f32> = det_out_data.to_vec();
    let boxes = dbnet_postprocess(&data, &shape, img.width(), img.height(), 0.18, 1.6, 0.6);
    println!("    找到 {} 个文本区域", boxes.len());

    if boxes.is_empty() {
        eprintln!("[!] 未检测到文字");
        std::process::exit(1);
    }

    // ── 识别 ──
    println!("[*] 识别文字...");
    let mut results: Vec<serde_json::Value> = Vec::new();
    let mut full_text = String::new();

    for (i, b) in boxes.iter().enumerate() {
        let Some((rec_data, rec_shape)) = preprocess_rec(&img, b) else { continue };
        let rec_input = ort::value::Tensor::from_array((rec_shape, rec_data)).unwrap();
        let rec_out = rec.run(ort::inputs![rec_input]).unwrap();
        let (rec_out_shape, rec_out_data) = rec_out[0].try_extract_tensor::<f32>().unwrap();
        let shape: Vec<usize> = rec_out_shape.iter().map(|&d| d as usize).collect();
        let data: Vec<f32> = rec_out_data.to_vec();
        let txt = ctc_decode(&data, &shape, &keys);
        if txt.is_empty() { continue; }

        if !full_text.is_empty() { full_text.push('\n'); }
        full_text.push_str(&txt);

        results.push(serde_json::json!({
            "text": txt,
            "x": b[0], "y": b[1],
            "w": b[2] - b[0], "h": b[3] - b[1],
        }));
        println!("    [{}] {} [{},{},{},{}]", i, txt, b[0], b[1], b[2], b[3]);
    }

    // ── 输出 JSON ──
    let output = serde_json::json!({
        "engine": "ppocr_v6",
        "tier": "PP-OCRv6_medium",
        "text": full_text,
        "count": results.len(),
        "words": results,
    });
    println!("\n{}", serde_json::to_string_pretty(&output).unwrap());
}
