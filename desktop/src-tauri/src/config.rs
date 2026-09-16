use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const CONFIG_FILE: &str = "config.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppConfig {
    /// MSIX package family of Claude Desktop. The hash part is derived from
    /// Anthropic's signing certificate, so it is identical on every machine.
    #[serde(default = "AppConfig::default_family")]
    pub package_family: String,
    /// Application id inside the package (AUMID = family!app_id).
    #[serde(default = "AppConfig::default_app_id")]
    pub app_id: String,
    /// Repair automatically when a 0x80070020 launch failure is seen. When
    /// false the UI asks first (and a notification is shown).
    #[serde(default = "AppConfig::default_true")]
    pub auto_repair: bool,
    /// Wait this long after the failure event before acting, so the failing
    /// launch has fully unwound (the scheduled task uses PT3S for the same).
    #[serde(default = "AppConfig::default_repair_delay_ms")]
    pub repair_delay_ms: u64,
    /// A failure older than this is not acted upon by the poll fallback.
    #[serde(default = "AppConfig::default_recent_window_minutes")]
    pub recent_window_minutes: u64,
    /// Loop guard: at most this many repair attempts per 15 minutes.
    #[serde(default = "AppConfig::default_max_attempts")]
    pub max_attempts_per_15min: u32,
    /// Status refresh interval (seconds). 0 disables the poll.
    #[serde(default = "AppConfig::default_poll_seconds")]
    pub poll_seconds: u64,
    #[serde(default = "AppConfig::default_true")]
    pub notify: bool,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default = "AppConfig::default_true")]
    pub close_to_tray: bool,
    #[serde(default = "AppConfig::default_true")]
    pub confirm_exit: bool,
    #[serde(default = "AppConfig::default_log_level")]
    pub log_level: String,
    /// Container-level failure remedy: restart the AppX Deployment Service.
    /// Only used when there is no Claude process left to stop, which means the
    /// container job is wedged and terminating processes cannot help. AppXSvc
    /// is the most likely holder of the stale handle. Requires admin.
    #[serde(default = "AppConfig::default_true")]
    pub restart_appxsvc_on_container_failure: bool,
    /// Last-resort remedy: re-register the package for the current user via
    /// `Add-AppxPackage -Register`. This rebuilds the per-user registration and
    /// does NOT delete login, session or settings data (unlike Reset-AppxPackage).
    /// Off by default: if it fails midway the package can be left unregistered.
    #[serde(default)]
    pub reregister_on_container_failure: bool,
}

impl AppConfig {
    fn default_family() -> String {
        "Claude_pzs8sxrjxfjjc".into()
    }
    fn default_app_id() -> String {
        "Claude".into()
    }
    fn default_true() -> bool {
        true
    }
    fn default_repair_delay_ms() -> u64 {
        3_000
    }
    fn default_recent_window_minutes() -> u64 {
        5
    }
    fn default_max_attempts() -> u32 {
        3
    }
    fn default_poll_seconds() -> u64 {
        5
    }
    fn default_log_level() -> String {
        "info".into()
    }

    pub fn aumid(&self) -> String {
        format!("{}!{}", self.package_family, self.app_id)
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            package_family: Self::default_family(),
            app_id: Self::default_app_id(),
            auto_repair: true,
            repair_delay_ms: Self::default_repair_delay_ms(),
            recent_window_minutes: Self::default_recent_window_minutes(),
            max_attempts_per_15min: Self::default_max_attempts(),
            poll_seconds: Self::default_poll_seconds(),
            notify: true,
            autostart: false,
            close_to_tray: true,
            confirm_exit: true,
            log_level: "info".into(),
            restart_appxsvc_on_container_failure: true,
            reregister_on_container_failure: false,
        }
    }
}

/// Same directories Tauri resolves for `app_config_dir` / `app_log_dir` on
/// Windows, computed without a running app (headless modes).
pub fn default_dirs() -> (PathBuf, PathBuf) {
    let appdata = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| appdata.clone());
    (
        appdata.join(crate::IDENTIFIER),
        local.join(crate::IDENTIFIER).join("logs"),
    )
}

pub fn config_path(app_dir: &PathBuf) -> PathBuf {
    app_dir.join(CONFIG_FILE)
}

pub fn load(app_dir: &PathBuf) -> Result<AppConfig> {
    std::fs::create_dir_all(app_dir).with_context(|| format!("creating {}", app_dir.display()))?;
    let path = config_path(app_dir);
    if !path.exists() {
        let cfg = AppConfig::default();
        save(app_dir, &cfg)?;
        return Ok(cfg);
    }
    let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let cfg: AppConfig = serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing config at {}", path.display()))?;
    Ok(cfg)
}

pub fn save(app_dir: &PathBuf, cfg: &AppConfig) -> Result<()> {
    std::fs::create_dir_all(app_dir).with_context(|| format!("creating {}", app_dir.display()))?;
    let path = config_path(app_dir);
    let tmp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(cfg).context("serializing config")?;
    std::fs::write(&tmp, &bytes).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("renaming into {}", path.display()))?;
    Ok(())
}
