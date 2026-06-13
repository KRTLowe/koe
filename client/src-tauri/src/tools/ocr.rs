use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;

use super::{Tool, ToolResult};
use crate::config::AppConfig;

// ── 模型配置 ────────────────────────────────────────

const MODEL_TIER: &str = "PP-OCRv6_medium";

const MODEL_BASE: &str = "models/PaddleOCR";
const DET_SUBDIR: &str = "PP-OCRv6_medium_det";
const REC_SUBDIR: &str = "PP-OCRv6_medium_rec";

// Hugging Face 模型仓库
const HF_DET_REPO: &str = "PaddlePaddle/PP-OCRv6_medium_det_onnx";
const HF_REC_REPO: &str = "PaddlePaddle/PP-OCRv6_medium_rec_onnx";

fn models_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    let exe_dir = exe.parent().unwrap_or(Path::new("."));
    exe_dir.join(MODEL_BASE)
}

fn det_model_path() -> PathBuf {
    models_dir().join(DET_SUBDIR).join("inference.onnx")
}
fn det_yml_path() -> PathBuf {
    models_dir().join(DET_SUBDIR).join("inference.yml")
}
fn rec_model_path() -> PathBuf {
    models_dir().join(REC_SUBDIR).join("inference.onnx")
}
fn rec_yml_path() -> PathBuf {
    models_dir().join(REC_SUBDIR).join("inference.yml")
}

// ── 字符字典解析（从 inference.yml） ────────────────
//
// inference.yml 中的 character_dict 是 YAML 缩进列表：
//   character_dict:
//   - '!'
//   - '"'
//   ...
// 我们解析 '...' 之间的内容，索引 0 保留给 CTC blank。

fn parse_char_dict(yml_path: &Path) -> Result<Vec<String>, String> {
    let content =
        std::fs::read_to_string(yml_path).map_err(|e| format!("读取 {} 失败: {}", yml_path.display(), e))?;

    // 用 serde_yaml 完整解析 inference.yml
    let value: serde_yaml::Value =
        serde_yaml::from_str(&content).map_err(|e| format!("解析 {} 失败: {}", yml_path.display(), e))?;

    let raw = value
        .get("PostProcess")
        .and_then(|p| p.get("character_dict"))
        .and_then(|d| d.as_sequence())
        .ok_or_else(|| format!("{} 中未找到 PostProcess.character_dict", yml_path.display()))?;

    // 索引 0 = CTC blank
    let mut keys: Vec<String> = vec![String::new()];
    for v in raw {
        if let Some(s) = v.as_str() {
            keys.push(s.to_string());
        } else {
            keys.push(String::new());
        }
    }

    // PP-OCRv6_medium_rec ONNX 输出 18710 类，补齐防止越界
    keys.resize(18710, String::new());

    log::info!(
        "[PaddleOCR] loaded {} character entries from {}",
        keys.len() - 1,
        yml_path.display()
    );
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_char_dict_count() {
        let yml = r#"PostProcess:
  name: CTCLabelDecode
  character_dict:
  - '!'
  - '"'
  - '#'
"#;
        let dir = std::env::temp_dir().join("ocr_test_parse");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.yml");
        std::fs::write(&path, yml).unwrap();
        let keys = parse_char_dict(&path).unwrap();
        // blank at 0 + 3 chars
        assert_eq!(keys.len(), 18710, "should pad to 18710");
        assert_eq!(keys[1], "!");
        assert_eq!(keys[2], "\"");
        assert_eq!(keys[3], "#");
        // padding entries are empty
        for i in 4..keys.len() {
            assert_eq!(keys[i], "", "index {} should be empty padding", i);
        }
        std::fs::remove_dir_all(dir).ok();
    }
}

// ── ONNX 引擎（Mutex 保护懒初始化） ─────────────────

struct OcrEngine {
    det: ort::session::Session,
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

    let keys = parse_char_dict(&rec_yml_path())?;

    *guard = Some(OcrEngine {
        det: load_session(&det_model_path())?,
        rec: load_session(&rec_model_path())?,
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

/// 读取系统代理：优先环境变量（HTTPS_PROXY/HTTP_PROXY），
/// 再尝试 netsh winhttp 获取 Windows 系统代理。
fn system_proxy() -> Option<String> {
    // 1. 环境变量（覆盖 VPN/Clash/v2rayN 等手动配置）
    for var in &["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"] {
        if let Ok(val) = std::env::var(var) {
            let val = val.trim().to_string();
            if !val.is_empty() {
                return Some(val);
            }
        }
    }

    // 2. Windows 系统代理（通过 netsh winhttp 查询）
    #[cfg(windows)]
    {
        use std::process::Command;
        if let Ok(output) = Command::new("netsh")
            .args(["winhttp", "show", "proxy"])
            .output()
        {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout);
                // 输出格式: "当前的 WinHTTP 代理服务器:  http://proxy:8080"
                for line in text.lines() {
                    if line.contains("代理服务器") || line.contains("Proxy Server") {
                        if let Some(pos) = line.find(':') {
                            let proxy = line[pos + 1..].trim().to_string();
                            if !proxy.is_empty() && proxy != "直接连接" && proxy != "Direct" {
                                return Some(proxy);
        }
    }
}

fn safe_preview(s: &str, max_bytes: usize) -> &str {
    if max_bytes >= s.len() {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}
                }
            }
        }
    }

    None
}

/// 从 Hugging Face 下载文件，跳过已存在的。支持系统代理。
fn hf_download(repo: &str, filename: &str, dest: &Path) -> Result<(), String> {
    if dest.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dest.parent().unwrap())
        .map_err(|e| format!("创建目录 {} 失败: {}", dest.parent().unwrap().display(), e))?;

    let url = format!("https://huggingface.co/{repo}/resolve/main/{filename}");
    log::info!("[PaddleOCR] downloading {} ...", url);

    let resp = if let Some(proxy_url) = system_proxy() {
        log::info!("[PaddleOCR] using proxy: {}", proxy_url);
        let proxy = ureq::Proxy::new(&proxy_url)
            .map_err(|e| format!("代理配置失败 ({}): {}", proxy_url, e))?;
        ureq::config::Config::builder()
            .proxy(Some(proxy))
            .build()
            .new_agent()
            .get(&url)
            .call()
            .map_err(|e| format!("下载 {} 失败 (通过代理 {}): {}", filename, proxy_url, e))?
    } else {
        ureq::get(&url)
            .call()
            .map_err(|e| format!("下载 {} 失败: {}", filename, e))?
    };

    let mut bytes: Vec<u8> = Vec::new();
    resp.into_body()
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| format!("读取 {} 失败: {}", filename, e))?;
    std::fs::write(dest, &bytes)
        .map_err(|e| format!("保存 {} 失败: {}", dest.display(), e))?;
    log::info!("[PaddleOCR] downloaded {} ({} bytes)", filename, bytes.len());
    Ok(())
}

fn download_models() -> Result<(), String> {
    // 检测模型
    hf_download(HF_DET_REPO, "inference.onnx", &det_model_path())?;
    hf_download(HF_DET_REPO, "inference.yml", &det_yml_path())?;
    // 识别模型
    hf_download(HF_REC_REPO, "inference.onnx", &rec_model_path())?;
    hf_download(HF_REC_REPO, "inference.yml", &rec_yml_path())?;
    Ok(())
}

fn auto_download_or_guide() -> Result<(), String> {
    let det = det_model_path();
    let rec = rec_model_path();
    let rec_yml = rec_yml_path();

    if det.exists() && rec.exists() && rec_yml.exists() {
        return Ok(());
    }

    log::info!("[PaddleOCR] models not found, attempting auto-download...");
    if let Err(e) = download_models() {
        log::warn!("[PaddleOCR] auto-download failed: {}", e);
        return Err(format!(
            "模型自动下载失败。请手动下载以下文件:\n  - {det}\n  - {det_yml}\n  - {rec}\n  - {rec_yml}\n\n\
             下载源:\n  https://huggingface.co/{HF_DET_REPO}\n  https://huggingface.co/{HF_REC_REPO}",
            det = det.display(),
            det_yml = det_yml_path().display(),
            rec = rec.display(),
            rec_yml = rec_yml_path().display(),
        ));
    }
    Ok(())
}

// ── 检测预处理（PP-OCRv6: NCHW 通道优先布局） ────────

fn det_max_side(img_max: u32) -> u32 {
    match img_max {
        0..=1200 => img_max,
        1201..=2000 => 1536,
        _ => 1920,
    }
}

fn preprocess_det(img: &image::DynamicImage, max_side: u32) -> Result<(Vec<f32>, Vec<i64>), String> {
    let (w, h) = (img.width(), img.height());
    let scale = ((max_side as f64) / (w.max(h) as f64)).min(1.0);
    let new_w = ((w as f64 * scale).round() as u32).max(32);
    let new_h = ((h as f64 * scale).round() as u32).max(32);
    let rw = ((new_w + 31) / 32) * 32;
    let rh = ((new_h + 31) / 32) * 32;

    let resized = img.resize_exact(rw, rh, image::imageops::FilterType::Lanczos3);
    // PP-OCRv6 使用 BGR 通道顺序，而 image::RgbImage 是 RGB，需要交换
    let rgb = resized.to_rgb8();

    // NCHW 布局 + BGR 顺序: [B..., G..., R...]
    // mean=[0.485,0.456,0.406] std=[0.229,0.224,0.225] 对应 B/G/R
    let total = (rw * rh) as usize;
    let mut data = vec![0.0f32; 3 * total];
    for y in 0..rh {
        for x in 0..rw {
            let pixel = rgb.get_pixel(x, y);
            let idx = (y * rw + x) as usize;
            data[idx] = (pixel[2] as f32 / 255.0 - 0.485) / 0.229;       // B
            data[total + idx] = (pixel[1] as f32 / 255.0 - 0.456) / 0.224; // G
            data[2 * total + idx] = (pixel[0] as f32 / 255.0 - 0.406) / 0.225; // R
        }
    }

    Ok((data, vec![1, 3, rh as i64, rw as i64]))
}

// ── 检测后处理 ────────────────────────────────────

/// 计算连通域在原图上的平均概率（框置信度）
fn box_confidence(
    output: &[f32], out_w: usize, out_h: usize,
    x1: usize, y1: usize, x2: usize, y2: usize,
) -> f32 {
    let x1 = x1.min(out_w - 1);
    let x2 = x2.min(out_w - 1);
    let y1 = y1.min(out_h - 1);
    let y2 = y2.min(out_h - 1);
    if x2 <= x1 || y2 <= y1 {
        return 0.0;
    }
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

fn dbnet_postprocess(
    output: &[f32],
    out_shape: &[usize],
    orig_size: (u32, u32),
    threshold: f32,
    unclip_ratio: f32,
    box_thresh: f32,
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
            if visited[idx] {
                continue;
            }
            if output[idx] < threshold {
                visited[idx] = true;
                continue;
            }

            let (mut x1, mut x2, mut y1, mut y2) = (x, x, y, y);
            let mut q = vec![(x, y)];
            visited[idx] = true;

            while let Some((cx, cy)) = q.pop() {
                x1 = x1.min(cx);
                x2 = x2.max(cx);
                y1 = y1.min(cy);
                y2 = y2.max(cy);
                for &(dx, dy) in &[(0i32, -1), (0, 1), (-1, 0), (1, 0)] {
                    let nx = cx as i32 + dx;
                    let ny = cy as i32 + dy;
                    if nx < 0 || nx >= w as i32 || ny < 0 || ny >= h as i32 {
                        continue;
                    }
                    let ni = ny as usize * w + nx as usize;
                    if visited[ni] {
                        continue;
                    }
                    if output[ni] >= threshold {
                        visited[ni] = true;
                        q.push((nx as usize, ny as usize));
        }
    }
}

            let conf = box_confidence(output, w, h, x1, y1, x2, y2);
            if conf < box_thresh {
                continue;
            }

            // Unclip：在输出坐标空间按面积/周长比例扩展
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
                b[0].max(0).min(orig_size.0 as i32 - 1),
                b[1].max(0).min(orig_size.1 as i32 - 1),
                b[2].max(0).min(orig_size.0 as i32 - 1),
                b[3].max(0).min(orig_size.1 as i32 - 1),
            ];
            if (b[2] - b[0]) >= 3 && (b[3] - b[1]) >= 3 {
                boxes.push(b);
            }
        }
    }

    boxes.sort_by(|a, b| {
        ((b[2] - b[0]) * (b[3] - b[1])).cmp(&((a[2] - a[0]) * (a[3] - a[1])))
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
            let area_b = ((b[2] - b[0]) * (b[3] - b[1])) as f64;
            let area_kb = ((kb[2] - kb[0]) * (kb[3] - kb[1])) as f64;
            let union = area_b + area_kb - inter;
            if union > 0.0 && inter / union > 0.5 {
                sup = true;
                break;
            }
        }
        if !sup {
            keep.push(b);
        }
    }
    keep
}

// ── 识别预处理（PP-OCRv6: H=48, 3ch, [-1,1]） ──────

const REC_H: u32 = 48;
const REC_MAX_W: u32 = 320;

fn preprocess_rec(img: &image::DynamicImage, box_: &[i32; 4]) -> Option<(Vec<f32>, Vec<i64>)> {
    let x = box_[0].max(0) as u32;
    let y = box_[1].max(0) as u32;
    let cw = (box_[2] - box_[0]).unsigned_abs().min(img.width() - x);
    let ch = (box_[3] - box_[1]).unsigned_abs().min(img.height() - y);
    if cw < 2 || ch < 2 {
        return None;
    }

    let cropped = img.crop_imm(x, y, cw.max(4), ch);

    // PP-OCRv6 rec: 3-channel, H=48, aspect-ratio W (max REC_MAX_W)
    // Normalize: (value/255 - 0.5)/0.5 = value/127.5 - 1.0 → [-1, 1]
    // Layout: NCHW (channel-first), padding to REC_MAX_W with zeros
    let scale = REC_H as f64 / cropped.height() as f64;
    let target_w = ((cropped.width() as f64 * scale).round() as u32)
        .max(4)
        .min(REC_MAX_W);

    let resized = cropped.resize_exact(
        target_w,
        REC_H,
        image::imageops::FilterType::Lanczos3,
    );
    let rgb = resized.to_rgb8();

    // 预分配 NCHW 缓冲区，默认全零（padding 区域自动为零）
    let mut data = vec![0.0f32; (3 * REC_H * REC_MAX_W) as usize];

    for y in 0..REC_H {
        for x in 0..target_w {
            let pixel = rgb.get_pixel(x, y);
            // BGR 顺序: channel 0=B, 1=G, 2=R
            let b_norm = pixel[2] as f32 / 127.5 - 1.0;
            let g_norm = pixel[1] as f32 / 127.5 - 1.0;
            let r_norm = pixel[0] as f32 / 127.5 - 1.0;

            let idx_b = (0 * REC_H + y) * REC_MAX_W + x;
            let idx_g = (1 * REC_H + y) * REC_MAX_W + x;
            let idx_r = (2 * REC_H + y) * REC_MAX_W + x;
            data[idx_b as usize] = b_norm;
            data[idx_g as usize] = g_norm;
            data[idx_r as usize] = r_norm;
        }
    }

    Some((data, vec![1, 3, REC_H as i64, REC_MAX_W as i64]))
}

// ── CTC 解码 ────────────────────────────────────────

fn ctc_decode(output: &[f32], shape: &[usize], keys: &[String]) -> String {
    // 模型输出 shape 为 [1, T, C]（batch=1, timesteps=T, classes=C=18710）
    let (timesteps, classes) = if shape.len() == 3 && shape[0] == 1 {
        (shape[1], shape[2])
    } else if shape.len() == 3 && shape[2] == 1 {
        (shape[0], shape[1])
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
            if v > max_v {
                max_v = v;
                max_idx = c;
            }
        }
        if max_idx != 0 && max_idx != prev {
            if let Some(s) = keys.get(max_idx) {
                if !s.is_empty() {
                    result.push(s.clone());
                }
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

fn is_zero(f: &f64) -> bool {
    f64::abs(*f) < f64::EPSILON
}

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
    fn name(&self) -> &'static str {
        "ocr_region"
    }
    fn description(&self) -> &'static str {
        "OCR recognize text from an image file using PP-OCRv6 (ONNX). \
         Auto-downloads medium models on first use. Returns words with bounding boxes."
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
    fn is_enabled(&self) -> bool {
        self.enabled
    }
    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn execute(&self, args: &Value) -> ToolResult {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        if path.is_empty() {
            return ToolResult::err("path is required".to_string());
        }

        if let Err(e) = auto_download_or_guide() {
            return ToolResult::err(e);
        }
        if let Err(e) = init_engine() {
            return ToolResult::err(e);
        }

        let img = match image::open(path) {
            Ok(img) => img,
            Err(e) => return ToolResult::err(format!("读取图片失败: {}", e)),
        };

        // ── 检测 ──
        let max_side = det_max_side(img.width().max(img.height()));
        let (det_data, det_shape) = match preprocess_det(&img, max_side) {
            Ok(v) => v,
            Err(e) => return ToolResult::err(e),
        };

        let boxes = match with_engine_mut(|eng| {
            let input = ort::value::Tensor::from_array((det_shape.clone(), det_data.clone()))
                .map_err(|e| format!("构建检测输入: {}", e))?;

            let out = eng
                .det
                .run(ort::inputs![input])
                .map_err(|e| format!("检测推理: {}", e))?;

            let (out_shape, out_data) = out[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| format!("读检测输出: {}", e))?;
            let shape: Vec<usize> = out_shape.iter().map(|&d| d as usize).collect();
            let data: Vec<f32> = out_data.to_vec();

            Ok(dbnet_postprocess(&data, &shape, (img.width(), img.height()), 0.3, 1.5, 0.6))
        }) {
            Ok(b) => b,
            Err(e) => return ToolResult::err(e),
        };

        // ── 识别 ──
        let (words, full_text) = match with_engine_mut(|eng| {
            let mut words: Vec<OcrWord> = Vec::new();
            let mut text = String::new();

            for b in &boxes {
                let (rec_data, rec_shape) = match preprocess_rec(&img, b) {
                    Some(v) => v,
                    None => continue,
                };

                let input = ort::value::Tensor::from_array((rec_shape.clone(), rec_data.clone()))
                    .map_err(|e| format!("构建识别输入: {}", e))?;

                let out = eng
                    .rec
                    .run(ort::inputs![input])
                    .map_err(|e| format!("识别推理: {}", e))?;

                let (out_shape, out_data) = out[0]
                    .try_extract_tensor::<f32>()
                    .map_err(|e| format!("读识别输出: {}", e))?;
                let shape: Vec<usize> = out_shape.iter().map(|&d| d as usize).collect();
                let data: Vec<f32> = out_data.to_vec();

                let txt = ctc_decode(&data, &shape, &eng.keys);
                if txt.is_empty() {
                    continue;
                }

                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&txt);

                words.push(OcrWord {
                    text: txt,
                    x: b[0],
                    y: b[1],
                    w: b[2] - b[0],
                    h: b[3] - b[1],
                    confidence: 0.0,
                });
            }

            log::info!(
                "[PaddleOCR] {} words, preview={}",
                words.len(),
                safe_preview(&text, 80)
            );
            Ok((words, text))
        }) {
            Ok(v) => v,
            Err(e) => return ToolResult::err(e),
        };

        match serde_json::to_string(&serde_json::json!({
            "engine": "ppocr_v6",
            "tier": MODEL_TIER,
            "text": full_text,
            "words": words,
        })) {
            Ok(json) => ToolResult::ok(json),
            Err(e) => ToolResult::err(format!("JSON: {}", e)),
        }
    }
}
