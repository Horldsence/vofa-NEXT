//! 按通道 (stable/beta) 检查应用更新。
//!
//! 流程: 从 GitHub Releases API 拉取最近的 release 列表, 按通道过滤
//! (stable 通道排除 prerelease, beta 通道全部接受), 选出 semver 最大的
//! release, 再将其 `latest.json` asset 作为 endpoint 交给
//! tauri-plugin-updater 完成实际的版本比较与更新下载。

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::Emitter;
use tauri_plugin_updater::{Update, UpdaterExt};

/// GitHub Releases API 地址 (列出最近 20 个 release)。
const RELEASES_API: &str =
    "https://api.github.com/repos/Horldsence/vofa-NEXT/releases?per_page=20";

/// 更新通道: 稳定版只接收正式 release, 测试版同时接收 prerelease。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Beta,
}

/// GitHub Releases API 返回的单个 release (仅保留用到的字段)。
#[derive(Debug, Clone, Deserialize)]
pub struct GithubRelease {
    pub tag_name: String,
    pub prerelease: bool,
    pub draft: bool,
    pub body: Option<String>,
    pub published_at: Option<String>,
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

/// 从 release 列表中按通道选出目标版本, 返回其在列表中的索引。
///
/// 过滤规则: 排除 draft; stable 通道额外排除 prerelease; tag 剥掉前导
/// 'v' 后必须能解析为 semver。返回 semver 最大者的索引, 无候选返回 None。
pub fn select_release(releases: &[GithubRelease], channel: Channel) -> Option<usize> {
    releases
        .iter()
        .enumerate()
        .filter(|(_, r)| !r.draft)
        .filter(|(_, r)| channel == Channel::Beta || !r.prerelease)
        .filter_map(|(i, r)| {
            semver::Version::parse(r.tag_name.trim_start_matches('v'))
                .ok()
                .map(|v| (i, v))
        })
        .max_by(|(_, a), (_, b)| a.cmp(b))
        .map(|(i, _)| i)
}

/// 按通道检查更新。命中时把 `Update` 存入 `PendingUpdate` 供下载命令使用。
#[tauri::command]
pub async fn check_update(
    app: tauri::AppHandle,
    pending: tauri::State<'_, PendingUpdate>,
    channel: Channel,
) -> Result<CheckUpdateResult, String> {
    let current_version = app.package_info().version.to_string();
    let unavailable = || CheckUpdateResult {
        available: false,
        current_version: current_version.clone(),
        version: None,
        notes: None,
        date: None,
    };

    // 1. 拉取 GitHub release 列表并按通道选出目标
    let releases: Vec<GithubRelease> = reqwest::Client::new()
        .get(RELEASES_API)
        .header(reqwest::header::USER_AGENT, "vofa-next-updater")
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let Some(idx) = select_release(&releases, channel) else {
        return Ok(unavailable());
    };
    let release = &releases[idx];

    // 2. 以该 release 的 latest.json 为 endpoint 交给 updater 插件做版本比较
    let endpoint = reqwest::Url::parse(&format!(
        "https://github.com/Horldsence/vofa-NEXT/releases/download/{}/latest.json",
        release.tag_name
    ))
    .map_err(|e| e.to_string())?;

    let maybe_update = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;

    // 3. 有更新则暂存, 无更新则清空暂存
    if let Some(update) = maybe_update {
        let version = update.version.trim_start_matches('v').to_string();
        *pending.0.lock().map_err(|e| e.to_string())? = Some(update);
        Ok(CheckUpdateResult {
            available: true,
            current_version,
            version: Some(version),
            notes: release.body.clone(),
            date: release.published_at.clone(),
        })
    } else {
        *pending.0.lock().map_err(|e| e.to_string())? = None;
        Ok(unavailable())
    }
}

/// 下载并安装此前 `check_update` 暂存的更新, 通过事件向前端汇报进度。
#[tauri::command]
pub async fn download_and_install_update(
    app: tauri::AppHandle,
    pending: tauri::State<'_, PendingUpdate>,
) -> Result<(), String> {
    // 先取出 Update 并立刻释放锁, 避免 MutexGuard 跨 await (不 Send)
    let update = pending
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .take()
        .ok_or_else(|| "no pending update".to_string())?;

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
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造测试用 release 的便捷函数。
    fn release(tag: &str, prerelease: bool, draft: bool) -> GithubRelease {
        GithubRelease {
            tag_name: tag.to_string(),
            prerelease,
            draft,
            body: None,
            published_at: None,
        }
    }

    #[test]
    fn stable_channel_excludes_prereleases() {
        let releases = vec![
            release("v0.1.2", false, false),
            release("v0.2.0-beta.1", true, false),
        ];
        let idx = select_release(&releases, Channel::Stable).expect("应选中稳定版");
        assert_eq!(releases[idx].tag_name, "v0.1.2");
    }

    #[test]
    fn beta_channel_includes_prereleases_and_picks_max_semver() {
        let releases = vec![
            release("v0.1.2", false, false),
            release("v0.2.0-beta.1", true, false),
            release("v0.2.0-beta.3", true, false),
        ];
        let idx = select_release(&releases, Channel::Beta).expect("应选中最高版本");
        assert_eq!(releases[idx].tag_name, "v0.2.0-beta.3");
    }

    #[test]
    fn drafts_are_excluded() {
        let releases = vec![
            release("v0.1.2", false, false),
            release("v0.9.9", false, true),
        ];
        let idx = select_release(&releases, Channel::Stable).expect("draft 不应参与");
        assert_eq!(releases[idx].tag_name, "v0.1.2");
    }

    #[test]
    fn unparseable_tags_are_excluded() {
        let releases = vec![
            release("v0.1.2", false, false),
            release("not-a-version", false, false),
            release("release-nightly", true, false),
        ];
        let idx = select_release(&releases, Channel::Beta).expect("非法 tag 不应参与");
        assert_eq!(releases[idx].tag_name, "v0.1.2");
    }

    #[test]
    fn empty_or_all_excluded_returns_none() {
        assert!(select_release(&[], Channel::Stable).is_none());
        assert!(select_release(&[], Channel::Beta).is_none());

        // beta 通道下全部是 draft / 非法 tag 时也为 None
        let releases = vec![release("v0.9.9", false, true), release("nightly", true, false)];
        assert!(select_release(&releases, Channel::Beta).is_none());

        // stable 通道下只有 prerelease 时为 None
        let releases = vec![release("v0.2.0-beta.1", true, false)];
        assert!(select_release(&releases, Channel::Stable).is_none());
    }

    #[test]
    fn release_beats_prerelease_of_same_version() {
        let releases = vec![
            release("v0.2.0-beta.3", true, false),
            release("v0.2.0", false, false),
        ];
        let idx = select_release(&releases, Channel::Beta).expect("正式版应大于同版本 prerelease");
        assert_eq!(releases[idx].tag_name, "v0.2.0");
    }
}
