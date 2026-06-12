use std::path::{Path, PathBuf};

pub struct PathGuard {
    pub allowed_reads: Vec<PathBuf>,
    pub allowed_writes: Vec<PathBuf>,
    pub denied_exts: Vec<String>,
}

/// Expand `~` prefix to the user's home directory.
fn expand_tilde(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        let home = home_dir().unwrap_or_else(|| {
            log::warn!("[PathGuard] HOME/USERPROFILE not set, falling back to current directory");
            ".".into()
        });
        home.join(rest)
    } else {
        PathBuf::from(s)
    }
}

/// Get the user's home directory, cross-platform.
fn home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .ok()
            .map(PathBuf::from)
            .or_else(|| std::env::var("HOME").ok().map(PathBuf::from))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

/// Strip `\\?\` prefix from Windows verbatim paths so `starts_with` works.
#[cfg(target_os = "windows")]
fn strip_verbatim_prefix(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        p.to_path_buf()
    }
}

impl PathGuard {
    pub fn new(config: &crate::config::AppConfig) -> Self {
        Self {
            allowed_reads: config
                .allowed_read_paths
                .iter()
                .map(|s| expand_tilde(s))
                .collect(),
            allowed_writes: config
                .allowed_write_paths
                .iter()
                .map(|s| expand_tilde(s))
                .collect(),
            denied_exts: config.denied_extensions.clone(),
        }
    }

    /// Validate path is readable. Returns canonicalized path or error.
    pub fn check_read(&self, path: &str) -> Result<PathBuf, String> {
        let p = Path::new(path);
        log::info!("[PathGuard::check_read] input path='{}'", path);
        log::info!(
            "[PathGuard::check_read] allowed_reads={:?}",
            self.allowed_reads
        );

        if !p.is_absolute() {
            log::warn!("[PathGuard::check_read] NOT_ABSOLUTE: path='{}'", path);
            return Err("路径必须是绝对路径".to_string());
        }

        let canonical = match p.canonicalize() {
            Ok(c) => {
                log::info!("[PathGuard::check_read] canonical='{}'", c.display());
                c
            }
            Err(e) => {
                log::warn!(
                    "[PathGuard::check_read] CANONICALIZE_FAILED: path='{}' err={}",
                    path,
                    e
                );
                return Err(format!("路径不存在或无权限访问: {}", e));
            }
        };

        // Strip \\?\ prefix on Windows so starts_with matches normally
        #[cfg(target_os = "windows")]
        let canonical = strip_verbatim_prefix(&canonical);

        let ext = canonical
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase());
        if let Some(ref e) = ext {
            if self.denied_exts.contains(e) {
                log::warn!("[PathGuard::check_read] BLOCKED_EXT: .{}", e);
                return Err(format!("不允许读取 .{} 类型的文件", e));
            }
        }

        let allowed_display: Vec<String> = self
            .allowed_reads
            .iter()
            .map(|d| d.display().to_string())
            .collect();
        let in_whitelist = self.allowed_reads.iter().any(|d| canonical.starts_with(d));
        log::info!(
            "[PathGuard::check_read] whitelist_check: canonical='{}' in_whitelist={}",
            canonical.display(),
            in_whitelist
        );
        if !in_whitelist {
            for d in &self.allowed_reads {
                log::info!(
                    "[PathGuard::check_read]   compare: starts_with('{}') = {}",
                    d.display(),
                    canonical.starts_with(d)
                );
            }
            return Err(format!(
                "路径不在允许的读取范围内。允许的目录: {}",
                allowed_display.join(", "),
            ));
        }

        Ok(canonical)
    }

    /// Validate path is writable. Returns canonicalized path or error.
    pub fn check_write(&self, path: &str) -> Result<PathBuf, String> {
        let p = Path::new(path);
        log::info!("[PathGuard::check_write] input path='{}'", path);
        log::info!(
            "[PathGuard::check_write] allowed_writes={:?}",
            self.allowed_writes
        );

        if !p.is_absolute() {
            log::warn!("[PathGuard::check_write] NOT_ABSOLUTE: path='{}'", path);
            return Err("路径必须是绝对路径".to_string());
        }

        // Check extension blacklist before any filesystem access
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase());
        if let Some(ref e) = ext {
            if self.denied_exts.contains(e) {
                log::warn!("[PathGuard::check_write] BLOCKED_EXT: .{}", e);
                return Err(format!("不允许写入 .{} 类型的文件", e));
            }
        }

        let exists = p.try_exists().unwrap_or(false);
        log::info!("[PathGuard::check_write] try_exists={}", exists);

        // If the file already exists, canonicalize the full path to prevent
        // symlink-based bypass (e.g., /whitelist/link -> /malicious/target)
        if exists {
            let canonical = match p.canonicalize() {
                Ok(c) => c,
                Err(e) => {
                    log::warn!(
                        "[PathGuard::check_write] CANONICALIZE_FAILED on existing file: err={}",
                        e
                    );
                    return Err(format!("无法解析路径: {}", e));
                }
            };
            #[cfg(target_os = "windows")]
            let canonical = strip_verbatim_prefix(&canonical);
            let in_whitelist = self.allowed_writes.iter().any(|d| canonical.starts_with(d));
            log::info!(
                "[PathGuard::check_write] canonical='{}' in_whitelist={}",
                canonical.display(),
                in_whitelist
            );
            if !in_whitelist {
                let allowed_display: Vec<String> = self
                    .allowed_writes
                    .iter()
                    .map(|d| d.display().to_string())
                    .collect();
                return Err(format!(
                    "路径不在允许的写入范围内。允许的目录: {}",
                    allowed_display.join(", "),
                ));
            }
            return Ok(canonical);
        }

        // For new files: parent must exist and be in whitelist
        let parent = match p.parent() {
            Some(pr) => pr,
            None => return Err("路径无效".to_string()),
        };
        let canonical_parent = match parent.canonicalize() {
            Ok(c) => c,
            Err(e) => {
                log::warn!(
                    "[PathGuard::check_write] PARENT_CANONICALIZE_FAILED: parent='{}' err={}",
                    parent.display(),
                    e
                );
                return Err(format!("父目录不存在: {}", e));
            }
        };
        #[cfg(target_os = "windows")]
        let canonical_parent = strip_verbatim_prefix(&canonical_parent);
        let in_whitelist = self
            .allowed_writes
            .iter()
            .any(|d| canonical_parent.starts_with(d));
        log::info!(
            "[PathGuard::check_write] parent_canonical='{}' in_whitelist={}",
            canonical_parent.display(),
            in_whitelist
        );
        if !in_whitelist {
            let allowed_display: Vec<String> = self
                .allowed_writes
                .iter()
                .map(|d| d.display().to_string())
                .collect();
            return Err(format!(
                "路径不在允许的写入范围内。允许的目录: {}",
                allowed_display.join(", "),
            ));
        }

        let file_name = match p.file_name() {
            Some(f) => f,
            None => return Err("路径无效".to_string()),
        };
        Ok(canonical_parent.join(file_name))
    }
}
