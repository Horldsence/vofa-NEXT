//! # update — 更新检查与下载命令

use error::{AppError, ConfigError, PluginError, Result};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::Emitter;
use tauri_plugin_updater::{Update, UpdaterExt};

/// stable 通道 manifest: 最新正式 release 的 latest.json。
const STABLE_MANIFEST: &str =
    "https://github.com/Horldsence/vofa-NEXT/releases/latest/download/latest.json";

/// beta 通道 manifest: 滚动 tag `beta` 上的 beta-latest.json (release CI 维护)。
const BETA_MANIFEST: &str =
    "https://github.com/Horldsence/vofa-NEXT/releases/download/beta/beta-latest.json";

/// 更新通道: 稳定版跟踪正式 release, 测试版同时跟踪 prerelease。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Beta,
}

/// `check_update` 命令的返回结果 (camelCase 与前端约定)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckUpdateResult {
    pub available: bool,
    pub current_version: String,
    pub version: Option<String>,
    pub notes: Option<String>,
    pub date: Option<String>,
}

/// 暂存已检测到、等待用户确认下载的更新。
pub struct PendingUpdate(pub Mutex<Option<Update>>);

/// 通道对应的更新 manifest URL。
const fn manifest_url(channel: Channel) -> &'static str {
    match channel {
        Channel::Stable => STABLE_MANIFEST,
        Channel::Beta => BETA_MANIFEST,
    }
}

/// 把 tauri-plugin-updater 错误包成 `Error::Plugin`,统一 IPC 协议。
fn wrap_updater<E: std::error::Error + Send + Sync + 'static>(e: E) -> AppError {
    AppError::Plugin(PluginError {
        plugin: "tauri-plugin-updater",
        source: Box::new(e),
    })
}

/// 按通道检查更新。命中时把 `Update` 存入 `PendingUpdate` 供下载命令使用。
#[tauri::command]
pub async fn check_update(
    app: tauri::AppHandle,
    pending: tauri::State<'_, PendingUpdate>,
    channel: Channel,
) -> Result<CheckUpdateResult> {
    let current_version = app.package_info().version.to_string();
    let unavailable = || CheckUpdateResult {
        available: false,
        current_version: current_version.clone(),
        version: None,
        notes: None,
        date: None,
    };

    // 以通道对应的静态 manifest 为 endpoint, 交给 updater 插件做版本比较
    let endpoint = reqwest::Url::parse(manifest_url(channel)).map_err(|e| {
        AppError::Config(ConfigError::UrlParse {
            url: manifest_url(channel).to_string(),
            source: std::io::Error::other(e.to_string()),
        })
    })?;

    let maybe_update = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(wrap_updater)?
        .build()
        .map_err(wrap_updater)?
        .check()
        .await
        .map_err(wrap_updater)?;

    // 有更新则暂存, 无更新则清空暂存; notes/date 取自 manifest 自身字段
    if let Some(update) = maybe_update {
        let result = CheckUpdateResult {
            available: true,
            current_version,
            version: Some(update.version.trim_start_matches('v').to_string()),
            notes: update.body.clone(),
            date: update.date.map(|d| d.to_string()),
        };
        *pending
            .0
            .lock()
            .map_err(|e| AppError::Config(ConfigError::MutexPoisoned(e.to_string())))? =
            Some(update);
        Ok(result)
    } else {
        *pending
            .0
            .lock()
            .map_err(|e| AppError::Config(ConfigError::MutexPoisoned(e.to_string())))? = None;
        Ok(unavailable())
    }
}

/// 下载并安装此前 `check_update` 暂存的更新, 通过事件向前端汇报进度。
#[tauri::command]
pub async fn download_and_install_update(
    app: tauri::AppHandle,
    pending: tauri::State<'_, PendingUpdate>,
) -> Result<()> {
    // 先取出 Update 并立刻释放锁, 避免 MutexGuard 跨 await (不 Send)
    let update = pending
        .0
        .lock()
        .map_err(|e| AppError::Config(ConfigError::MutexPoisoned(e.to_string())))?
        .take()
        .ok_or_else(|| {
            AppError::Plugin(PluginError {
                plugin: "tauri-plugin-updater",
                source: Box::new(std::io::Error::other("no pending update")),
            })
        })?;

    update
        .download_and_install(
            |received, total| {
                let _ = app.emit(
                    "update://progress",
                    serde_json::json!({"received": received, "total": total}),
                );
            },
            || {
                let _ = app.emit("update://ready", ());
            },
        )
        .await
        .map_err(wrap_updater)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_channel_uses_latest_release_manifest() {
        assert_eq!(
            manifest_url(Channel::Stable),
            "https://github.com/Horldsence/vofa-NEXT/releases/latest/download/latest.json"
        );
    }

    #[test]
    fn beta_channel_uses_rolling_beta_tag_manifest() {
        assert_eq!(
            manifest_url(Channel::Beta),
            "https://github.com/Horldsence/vofa-NEXT/releases/download/beta/beta-latest.json"
        );
    }

    #[test]
    fn manifests_are_valid_urls_and_never_hit_github_api() {
        // 回归保护: manifest 必须走 release 资产服务, 不得回到 api.github.com
        // (未认证限额 60 次/小时/IP, 超额 403)
        for url in [STABLE_MANIFEST, BETA_MANIFEST] {
            let parsed = reqwest::Url::parse(url).expect("manifest 必须是合法 URL");
            assert_ne!(parsed.host_str(), Some("api.github.com"));
        }
    }
}
