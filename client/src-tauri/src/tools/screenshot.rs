use serde_json::Value;

use super::{Tool, ToolResult};
use crate::config::AppConfig;
use crate::file_handler;

pub struct ScreenshotTool {
    enabled: bool,
}

impl ScreenshotTool {
    pub fn new(config: &AppConfig) -> Self {
        let enabled = config
            .tool_permissions
            .get("take_screenshot")
            .copied()
            .unwrap_or(true);
        Self { enabled }
    }
}

impl Tool for ScreenshotTool {
    fn name(&self) -> &'static str {
        "take_screenshot"
    }

    fn description(&self) -> &'static str {
        "Capture the Windows desktop screen, optionally cropped to a region (x, y, width, height in pixels)"
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "region": {
                    "type": "string",
                    "enum": ["full"],
                    "description": "Screen region to capture (default: full)"
                },
                "x": { "type": "integer", "description": "Left coordinate for region crop" },
                "y": { "type": "integer", "description": "Top coordinate for region crop" },
                "width": { "type": "integer", "description": "Width for region crop" },
                "height": { "type": "integer", "description": "Height for region crop" }
            }
        })
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn execute(&self, args: &Value) -> ToolResult {
        let start = std::time::Instant::now();
        log::info!("[ScreenshotTool] execute begin: enabled={}", self.enabled);

        let crop_x = args.get("x").and_then(|v| v.as_u64()).map(|v| v as u32);
        let crop_y = args.get("y").and_then(|v| v.as_u64()).map(|v| v as u32);
        let crop_w = args.get("width").and_then(|v| v.as_u64()).map(|v| v as u32);
        let crop_h = args
            .get("height")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);

        let do_crop = crop_x.is_some() && crop_y.is_some() && crop_w.is_some() && crop_h.is_some();
        if do_crop {
            log::info!(
                "[ScreenshotTool] region crop: x={:?} y={:?} w={:?} h={:?}",
                crop_x,
                crop_y,
                crop_w,
                crop_h
            );
        }

        #[cfg(target_os = "windows")]
        let result = {
            use screenshots::image::{DynamicImage, ImageFormat};
            use screenshots::Screen;

            let screens = Screen::all().unwrap_or_default();
            let screen = match screens.first() {
                Some(s) => s,
                None => {
                    log::info!("[ScreenshotTool] no screens found");
                    return ToolResult::err("No screens found".to_string());
                }
            };

            log::info!(
                "[ScreenshotTool] screen count={}, using first screen {:?}",
                screens.len(),
                screen
            );

            match screen.capture() {
                Ok(image) => {
                    log::info!(
                        "[ScreenshotTool] capture succeeded: {}x{}",
                        image.width(),
                        image.height()
                    );
                    let dyn_img = DynamicImage::from(image);

                    let final_img = if do_crop {
                        let (x, y, w, h) = (
                            crop_x.unwrap(),
                            crop_y.unwrap(),
                            crop_w.unwrap(),
                            crop_h.unwrap(),
                        );
                        // 裁剪边界安全：不超出图像范围
                        let img_w = dyn_img.width();
                        let img_h = dyn_img.height();
                        let crop_x = x.min(img_w);
                        let crop_y = y.min(img_h);
                        let crop_w = w.min(img_w - crop_x);
                        let crop_h = h.min(img_h - crop_y);
                        log::info!(
                            "[ScreenshotTool] cropping to x={} y={} w={} h={}",
                            crop_x,
                            crop_y,
                            crop_w,
                            crop_h
                        );
                        dyn_img.crop_imm(crop_x, crop_y, crop_w, crop_h)
                    } else {
                        dyn_img
                    };

                    let mut png_buf = std::io::Cursor::new(Vec::new());
                    match final_img.write_to(&mut png_buf, ImageFormat::Png) {
                        Ok(_) => {
                            let png_data = png_buf.into_inner();
                            let filename = format!(
                                "screenshot_{}.png",
                                chrono::Local::now().format("%Y%m%d_%H%M%S")
                            );
                            log::info!(
                                "[ScreenshotTool] PNG encoded: {} bytes, filename={}",
                                png_data.len(),
                                filename
                            );
                            match file_handler::save_file(&filename, &png_data) {
                                Ok(path) => {
                                    let path_str = path.to_string_lossy().to_string();
                                    log::info!("[ScreenshotTool] saved to: {}", path_str);
                                    ToolResult::ok_with_upload(
                                        format!("截图已保存: {}", path_str),
                                        path_str,
                                    )
                                }
                                Err(e) => {
                                    log::info!("[ScreenshotTool] save failed: {}", e);
                                    ToolResult::err(format!("Save screenshot failed: {}", e))
                                }
                            }
                        }
                        Err(e) => {
                            log::info!("[ScreenshotTool] PNG encode failed: {}", e);
                            ToolResult::err(format!("PNG encoding failed: {}", e))
                        }
                    }
                }
                Err(e) => {
                    log::info!("[ScreenshotTool] capture failed: {}", e);
                    ToolResult::err(format!("Screenshot failed: {}", e))
                }
            }
        };

        #[cfg(not(target_os = "windows"))]
        let result = {
            let _ = args;
            log::info!("[ScreenshotTool] not available on this platform");
            ToolResult::err("take_screenshot is only available on Windows".to_string())
        };

        log::info!(
            "[ScreenshotTool] execute end: elapsed={:?}, is_error={}, upload={:?}",
            start.elapsed(),
            result.is_error,
            result.upload_path
        );
        result
    }
}
