use serde_json::Value;
use std::process::Command;

use super::{Tool, ToolResult};
use crate::config::AppConfig;

pub struct OcrTool {
    enabled: bool,
}

impl OcrTool {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            enabled: config.tool_permissions.get("ocr_region").copied().unwrap_or(true),
        }
    }
}

// ── Tesseract ───────────────────────────────────────

/// 尝试用 Tesseract CLI 做 OCR，返回词级别结果。
/// 失败时返回 None（Tesseract 未安装或出错）。
fn ocr_tesseract(path: &str, lang: &str) -> Option<Vec<OcrWord>> {
    let output = Command::new("tesseract")
        .args([path, "stdout", "-l", lang, "--psm", "6", "tsv"])
        .output()
        .ok()?;

    if !output.status.success() {
        log::info!("[OCR] Tesseract exited with non-zero");
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_tesseract_tsv(&stdout)
}

fn parse_tesseract_tsv(tsv: &str) -> Option<Vec<OcrWord>> {
    let mut words = Vec::new();
    for line in tsv.lines().skip(1) {
        // level	page_num	block_num	par_num	line_num	word_num	left	top	width	height	conf	text
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 12 {
            continue;
        }
        let level: i32 = fields[0].parse().ok()?;
        if level != 5 {
            continue; // 只取词级别
        }
        let conf: f64 = fields[10].parse().unwrap_or(0.0);
        let text = fields[11].trim().to_string();
        if text.is_empty() {
            continue;
        }
        words.push(OcrWord {
            text,
            x: fields[6].parse().unwrap_or(0),
            y: fields[7].parse().unwrap_or(0),
            w: fields[8].parse().unwrap_or(0),
            h: fields[9].parse().unwrap_or(0),
            confidence: conf,
        });
    }
    if words.is_empty() { None } else { Some(words) }
}

// ── Windows OCR fallback ────────────────────────────

#[cfg(target_os = "windows")]
fn ocr_windows(path: &str) -> Option<Vec<OcrWord>> {
    use std::path::Path;

    let path = Path::new(path);
    if !path.exists() {
        log::info!("[OCR] Windows: file not found {}", path.display());
        return None;
    }

    match windows_ocr_inner(path) {
        Ok(words) => {
            if words.is_empty() { None } else { Some(words) }
        }
        Err(e) => {
            log::info!("[OCR] Windows OCR failed: {}", e);
            None
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_ocr_inner(path: &std::path::Path) -> Result<Vec<OcrWord>, String> {
    use windows::core::HSTRING;
    use windows::Graphics::Imaging::BitmapDecoder;
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::FileAccessMode;

    let hpath = HSTRING::from(path.to_string_lossy().as_ref());

    let file = windows::Storage::StorageFile::GetFileFromPathAsync(&hpath)
        .map_err(|e| format!("GetFileFromPathAsync: {}", e))?
        .get()
        .map_err(|e| format!("StorageFile get: {}", e))?;

    let stream = file
        .OpenAsync(FileAccessMode::Read)
        .map_err(|e| format!("OpenAsync: {}", e))?
        .get()
        .map_err(|e| format!("stream get: {}", e))?;

    let decoder = BitmapDecoder::CreateAsync(&stream)
        .map_err(|e| format!("CreateAsync: {}", e))?
        .get()
        .map_err(|e| format!("decoder get: {}", e))?;

    let bitmap = decoder
        .GetSoftwareBitmapAsync()
        .map_err(|e| format!("GetSoftwareBitmapAsync: {}", e))?
        .get()
        .map_err(|e| format!("bitmap get: {}", e))?;

    let engine = OcrEngine::TryCreateFromUserProfileLanguages()
        .map_err(|e| format!("TryCreateFromUserProfileLanguages: {}", e))?;

    let result = engine
        .RecognizeAsync(&bitmap)
        .map_err(|e| format!("RecognizeAsync: {}", e))?
        .get()
        .map_err(|e| format!("result get: {}", e))?;

    let full_text = result
        .Text()
        .map_err(|e| format!("Text: {}", e))?
        .to_string();

    if full_text.trim().is_empty() {
        return Ok(Vec::new());
    }

    Ok(vec![OcrWord {
        text: full_text,
        x: 0,
        y: 0,
        w: 0,
        h: 0,
        confidence: 0.0,
    }])
}

#[cfg(not(target_os = "windows"))]
fn ocr_windows(_path: &str) -> Option<Vec<OcrWord>> {
    None
}

// ── Data ────────────────────────────────────────────

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

// ── Tool impl ───────────────────────────────────────

impl Tool for OcrTool {
    fn name(&self) -> &'static str {
        "ocr_region"
    }

    fn description(&self) -> &'static str {
        "OCR recognize text from an image file. Returns words with bounding boxes. \
         First tries Tesseract (if installed), falls back to Windows built-in OCR. \
         Use with take_screenshot to read text from any window."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to image file (PNG, JPG). Usually from take_screenshot."
                },
                "lang": {
                    "type": "string",
                    "description": "Tesseract language code (default: chi_sim+eng). Ignored by Windows OCR."
                }
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
        let lang = args.get("lang").and_then(|v| v.as_str()).unwrap_or("chi_sim+eng");

        if path.is_empty() {
            return ToolResult::err("path is required".to_string());
        }

        log::info!("[OCR] path={} lang={}", path, lang);

        // 1. 尝试 Tesseract
        let words = if let Some(w) = ocr_tesseract(path, lang) {
            log::info!("[OCR] Tesseract: {} words", w.len());
            Some((w, "tesseract"))
        }
        // 2. 回退到 Windows OCR
        else if let Some(w) = ocr_windows(path) {
            log::info!("[OCR] Windows: {} words", w.len());
            Some((w, "windows"))
        } else {
            log::info!("[OCR] both engines failed");
            None
        };

        match words {
            Some((words, engine)) => {
                let text = words.iter()
                    .map(|w| w.text.as_str())
                    .collect::<Vec<_>>()
                    .join("");
                let preview = if text.len() > 80 {
                    format!("{}...", &text[..80])
                } else {
                    text.clone()
                };
                log::info!("[OCR] engine={} text_preview={}", engine, preview);

                match serde_json::to_string(&serde_json::json!({
                    "engine": engine,
                    "text": text,
                    "words": words,
                })) {
                    Ok(json) => ToolResult::ok(json),
                    Err(e) => ToolResult::err(format!("JSON serialization failed: {}", e)),
                }
            }
            None => ToolResult::err(
                "OCR failed: neither Tesseract nor Windows OCR available. \
                 Install Tesseract from https://github.com/UB-Mannheim/tesseract/wiki \
                 or ensure Windows 10+ language pack for Chinese is installed."
                    .to_string(),
            ),
        }
    }
}
