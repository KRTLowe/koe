use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use image::GenericImageView;
use serde_json::Value;

use super::{Tool, ToolResult};
use crate::config::AppConfig;

// ── 模型路径 ────────────────────────────────────────

const MODEL_BASE: &str = "models/PaddleOCR";

fn models_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    let exe_dir = exe.parent().unwrap_or(Path::new("."));
    exe_dir.join(MODEL_BASE)
}

fn det_path() -> PathBuf {
    models_dir().join("ch_PP-OCRv5_mobile_det.onnx")
}
fn cls_path() -> PathBuf {
    models_dir().join("ch_ppocr_mobile_v2.0_cls_infer.onnx")
}
fn rec_path() -> PathBuf {
    models_dir().join("ch_PP-OCRv5_rec_mobile_infer.onnx")
}
fn keys_path() -> PathBuf {
    models_dir().join("ppocr_keys_v1.txt")
}

// ── ONNX 引擎（Mutex 保护懒初始化） ─────────────────

struct OcrEngine {
    det: ort::session::Session,
    #[allow(dead_code)]
    cls: ort::session::Session,
    rec: ort::session::Session,
    keys: Vec<String>,
}

static ENGINE: Mutex<Option<OcrEngine>> = Mutex::new(None);

fn load_session(path: &Path) -> Result<ort::session::Session, String> {
    ort::session::Session::builder()
        .map_err(|e| format!("ort builder: {}", e))?
        .commit_from_file(path)
        .map_err(|e| format!("加载 {} 失败: {}", path.display(), e))
}

fn init_engine() -> Result<(), String> {
    let mut guard = ENGINE.lock().unwrap();
    if guard.is_some() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&keys_path())
        .map_err(|e| format!("读取 {} 失败: {}", keys_path().display(), e))?;
    let mut keys: Vec<String> =
        content.lines().map(|l| l.trim().to_string()).collect();
    if keys.is_empty() || !keys[0].is_empty() {
        keys.insert(0, String::new());
    }

    *guard = Some(OcrEngine {
        det: load_session(&det_path())?,
        cls: load_session(&cls_path())?,
        rec: load_session(&rec_path())?,
        keys,
    });

    Ok(())
}

fn with_engine_mut<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce(&mut OcrEngine) -> Result<T, String>,
{
    let mut guard = ENGINE.lock().map_err(|_| "Mutex poisoned".to_string())?;
    match guard.as_mut() {
        Some(eng) => f(eng),
        None => Err("OCR 引擎未初始化".to_string()),
    }
}

// ── 模型下载 ────────────────────────────────────────

fn download_models() -> Result<(), String> {
    let dir = models_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("创建模型目录失败: {}", e))?;

    let files = [
        ("ch_PP-OCRv5_mobile_det.onnx",
         "https://github.com/PaddlePaddle/PaddleOCR/releases/download/v3.0/ch_PP-OCRv5_mobile_det_infer.onnx"),
        ("ch_ppocr_mobile_v2.0_cls_infer.onnx",
         "https://github.com/PaddlePaddle/PaddleOCR/releases/download/v3.0/ch_ppocr_mobile_v2.0_cls_infer.onnx"),
        ("ch_PP-OCRv5_rec_mobile_infer.onnx",
         "https://github.com/PaddlePaddle/PaddleOCR/releases/download/v3.0/ch_PP-OCRv5_mobile_rec_infer.onnx"),
        ("ppocr_keys_v1.txt",
         "https://raw.githubusercontent.com/PaddlePaddle/PaddleOCR/main/ppocr/utils/ppocr_keys_v1.txt"),
    ];

    for (name, url) in &files {
        let path = dir.join(name);
        if path.exists() {
            continue;
        }
        log::info!("[PaddleOCR] downloading {} ...", name);
        let resp = ureq::get(*url)
            .call()
            .map_err(|e| format!("下载 {} 失败: {}", name, e))?;
        let mut bytes: Vec<u8> = Vec::new();
        resp.into_body()
            .into_reader()
            .read_to_end(&mut bytes)
            .map_err(|e| format!("读取 {} 失败: {}", name, e))?;
        std::fs::write(&path, &bytes)
            .map_err(|e| format!("保存 {} 失败: {}", name, e))?;
        log::info!("[PaddleOCR] downloaded {} ({} bytes)", name, bytes.len());
    }
    Ok(())
}

fn auto_download_or_guide() -> Result<(), String> {
    let det = det_path();
    let cls = cls_path();
    let rec = rec_path();
    let keys = keys_path();

    if det.exists() && cls.exists() && rec.exists() && keys.exists() {
        return Ok(());
    }

    log::info!("[PaddleOCR] models not found, attempting auto-download...");
    if let Err(e) = download_models() {
        log::warn!("[PaddleOCR] auto-download failed: {}", e);
        return Err(format!(
            "模型自动下载失败。请手动下载模型文件到:\n  {}\n\n\
             需要的文件:\n  - ch_PP-OCRv5_mobile_det.onnx\n  \
             - ch_ppocr_mobile_v2.0_cls_infer.onnx\n  \
             - ch_PP-OCRv5_rec_mobile_infer.onnx\n  \
             - ppocr_keys_v1.txt\n\n\
             PaddleOCR: https://github.com/PaddlePaddle/PaddleOCR/releases\n\
             keys: https://github.com/PaddlePaddle/PaddleOCR/blob/main/ppocr/utils/ppocr_keys_v1.txt",
            models_dir().display()
        ));
    }
    Ok(())
}

// ── 检测预处理 ──────────────────────────────────────

fn preprocess_det(img: &image::DynamicImage, max_side: u32)
    -> Result<(Vec<f32>, Vec<i64>), String>
{
    let (w, h) = (img.width(), img.height());
    let scale = (max_side as f64) / (w.max(h) as f64).min(1.0);
    let new_w = ((w as f64 * scale).round() as u32).max(32);
    let new_h = ((h as f64 * scale).round() as u32).max(32);
    let rw = ((new_w + 31) / 32) * 32;
    let rh = ((new_h + 31) / 32) * 32;

    let resized = img.resize_exact(rw, rh, image::imageops::FilterType::Lanczos3);
    let rgb = resized.to_rgb8();

    let mut data = Vec::with_capacity((3 * rw * rh) as usize);
    for pixel in rgb.pixels() {
        data.push((pixel[0] as f32 / 255.0 - 0.485) / 0.229);
        data.push((pixel[1] as f32 / 255.0 - 0.456) / 0.224);
        data.push((pixel[2] as f32 / 255.0 - 0.406) / 0.225);
    }

    Ok((data, vec![1, 3, rh as i64, rw as i64]))
}

// ── 检测后处理 ──────────────────────────────────────

fn dbnet_postprocess(
    output: &[f32],
    out_shape: &[usize],
    orig_size: (u32, u32),
    threshold: f32,
) -> Vec<[i32; 4]> {
    let (h, w) = if out_shape.len() == 4 && out_shape[1] == 1 {
        (out_shape[2], out_shape[3])
    } else if out_shape.len() == 4 {
        (out_shape[1], out_shape[2])
    } else {
        return vec![];
    };

    let sx = orig_size.0 as f32 / w as f32;
    let sy = orig_size.1 as f32 / h as f32;
    let mut visited = vec![false; h * w];
    let mut boxes: Vec<[i32; 4]> = Vec::new();

    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            if visited[idx] { continue; }
            let p = 1.0 / (1.0 + (-output[idx]).exp());
            if p < threshold { visited[idx] = true; continue; }

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
                    if 1.0 / (1.0 + (-output[ni]).exp()) >= threshold {
                        visited[ni] = true;
                        q.push((nx as usize, ny as usize));
                    }
                }
            }

            let b = [
                (x1 as f32 * sx).round() as i32,
                (y1 as f32 * sy).round() as i32,
                (x2 as f32 * sx).round() as i32,
                (y2 as f32 * sy).round() as i32,
            ];
            if (b[2] - b[0]) >= 3 && (b[3] - b[1]) >= 3 {
                boxes.push(b);
            }
        }
    }

    boxes.sort_by(|a, b| {
        ((b[2]-b[0])*(b[3]-b[1])).cmp(&((a[2]-a[0])*(a[3]-a[1])))
    });

    let mut keep: Vec<[i32; 4]> = Vec::new();
    for &b in &boxes {
        let mut sup = false;
        for &kb in &keep {
            let ix = b[0].max(kb[0]);
            let iy = b[1].max(kb[1]);
            let iw = (b[2].min(kb[2]) - ix).max(0);
            let ih = (b[3].min(kb[3]) - iy).max(0);
            let inter = (iw * ih) as f64;
            let area_b = ((b[2]-b[0])*(b[3]-b[1])) as f64;
            let area_kb = ((kb[2]-kb[0])*(kb[3]-kb[1])) as f64;
            let union = area_b + area_kb - inter;
            if union > 0.0 && inter / union > 0.5 { sup = true; break; }
        }
        if !sup { keep.push(b); }
    }
    keep
}

// ── 识别预处理 ──────────────────────────────────────

fn preprocess_rec(img: &image::DynamicImage, box_: &[i32; 4]) -> Option<(Vec<f32>, Vec<i64>)> {
    let x = box_[0].max(0) as u32;
    let y = box_[1].max(0) as u32;
    let cw = (box_[2] - box_[0]).unsigned_abs().min(img.width() - x);
    let ch = (box_[3] - box_[1]).unsigned_abs().min(img.height() - y);
    if cw < 2 || ch < 2 { return None; }

    let cropped = img.crop_imm(x, y, cw.max(4), ch);
    let gray = cropped.to_luma8();
    let target_h = 32u32;
    let scale = target_h as f64 / gray.height() as f64;
    let target_w = ((gray.width() as f64 * scale).round() as u32).max(4).min(320);
    let resized = image::imageops::resize(
        &gray, target_w, target_h, image::imageops::FilterType::Triangle,
    );

    let mut data = Vec::with_capacity((target_w * target_h) as usize);
    for p in resized.pixels() {
        data.push(p[0] as f32 / 255.0);
    }
    Some((data, vec![1, 1, 32, target_w as i64]))
}

// ── CTC 解码 ────────────────────────────────────────

fn ctc_decode(output: &[f32], shape: &[usize], keys: &[String]) -> String {
    let (classes, timesteps) = if shape.len() == 3 && shape[0] == 1 {
        (shape[1], shape[2])
    } else if shape.len() == 3 && shape[2] == 1 {
        (shape[1], shape[0])
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

// ── OcrWord ────────────────────────────────────────

#[derive(serde::Serialize)]
struct OcrWord {
    text: String,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    #[serde(skip_serializing_if = "is_zero")]
    confidence: f64,
}

fn is_zero(f: &f64) -> bool { f64::abs(*f) < f64::EPSILON }

// ── Tool ────────────────────────────────────────────

pub struct OcrTool {
    enabled: bool,
}

impl OcrTool {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            enabled: config
                .tool_permissions
                .get("ocr_region")
                .copied()
                .unwrap_or(true),
        }
    }
}

impl Tool for OcrTool {
    fn name(&self) -> &'static str { "ocr_region" }
    fn description(&self) -> &'static str {
        "OCR recognize text from an image file using PaddleOCR (ONNX). \
         Auto-downloads models on first use. Returns words with bounding boxes."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to image file." },
                "lang": { "type": "string", "description": "Language (unused, auto-detect)." }
            },
            "required": ["path"]
        })
    }
    fn is_enabled(&self) -> bool { self.enabled }
    fn set_enabled(&mut self, enabled: bool) { self.enabled = enabled; }

    fn execute(&self, args: &Value) -> ToolResult {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        if path.is_empty() { return ToolResult::err("path is required".to_string()); }

        if let Err(e) = auto_download_or_guide() { return ToolResult::err(e); }
        if let Err(e) = init_engine() { return ToolResult::err(e); }

        let img = match image::open(path) {
            Ok(img) => img,
            Err(e) => return ToolResult::err(format!("读取图片失败: {}", e)),
        };

        // 检测
        let (det_data, det_shape) = match preprocess_det(&img, 1024) {
            Ok(v) => v,
            Err(e) => return ToolResult::err(e),
        };

        let boxes = match with_engine_mut(|eng| {
            let input = ort::value::Tensor::from_array(
                (det_shape.clone(), det_data.clone()),
            ).map_err(|e| format!("构建检测输入: {}", e))?;

            let out = eng.det.run(ort::inputs![input])
                .map_err(|e| format!("检测推理: {}", e))?;

            let (out_shape, out_data) = out[0].try_extract_tensor::<f32>()
                .map_err(|e| format!("读检测输出: {}", e))?;
            let shape: Vec<usize> = out_shape.iter().map(|&d| d as usize).collect();
            let data: Vec<f32> = out_data.to_vec();

            Ok(dbnet_postprocess(&data, &shape, (img.width(), img.height()), 0.3))
        }) {
            Ok(b) => b,
            Err(e) => return ToolResult::err(e),
        };

        // 识别
        let (words, full_text) = match with_engine_mut(|eng| {
            let mut words: Vec<OcrWord> = Vec::new();
            let mut text = String::new();

            for b in &boxes {
                let (rec_data, rec_shape) = match preprocess_rec(&img, b) {
                    Some(v) => v,
                    None => continue,
                };

                let input = ort::value::Tensor::from_array(
                    (rec_shape.clone(), rec_data.clone()),
                ).map_err(|e| format!("构建识别输入: {}", e))?;

                let out = eng.rec.run(ort::inputs![input])
                    .map_err(|e| format!("识别推理: {}", e))?;

                let (out_shape, out_data) = out[0].try_extract_tensor::<f32>()
                    .map_err(|e| format!("读识别输出: {}", e))?;
                let shape: Vec<usize> = out_shape.iter().map(|&d| d as usize).collect();
                let data: Vec<f32> = out_data.to_vec();

                let txt = ctc_decode(&data, &shape, &eng.keys);
                if txt.is_empty() { continue; }

                if !text.is_empty() { text.push('\n'); }
                text.push_str(&txt);

                words.push(OcrWord {
                    text: txt,
                    x: b[0], y: b[1],
                    w: b[2] - b[0], h: b[3] - b[1],
                    confidence: 0.0,
                });
            }

            log::info!("[PaddleOCR] {} words, preview={:?}", words.len(),
                if text.len() > 80 { &text[..80] } else { &text });
            Ok((words, text))
        }) {
            Ok(v) => v,
            Err(e) => return ToolResult::err(e),
        };

        match serde_json::to_string(&serde_json::json!({
            "engine": "paddle_ocr",
            "text": full_text,
            "words": words,
        })) {
            Ok(json) => ToolResult::ok(json),
            Err(e) => ToolResult::err(format!("JSON: {}", e)),
        }
    }
}
