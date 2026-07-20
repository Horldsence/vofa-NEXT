//! 应用设置持久化 — JSON 存于系统配置目录 (directories::ProjectDirs)

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::theme::ThemeKind;

/// 应用设置 (持久化到 settings.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// 主题: Catppuccin 风味 (反序列化兼容旧值 "system"/"dark"/"light")
    pub theme: ThemeKind,
    /// 界面语言: "zh-CN" / "en-US" (仅存储, i18n 后续接入)
    pub language: String,
    /// 是否在启动时显示新手引导
    pub show_onboarding: bool,
    /// 波形缓冲区最大点数
    pub waveform_buffer_capacity: usize,
    /// 原始数据收集器容量 (字节)
    pub rawdata_buffer_capacity: usize,
    /// CAN 帧缓冲区最大帧数
    pub can_buffer_capacity: usize,
    /// 逻辑采样缓冲区最大采样数
    pub logic_buffer_capacity: usize,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: ThemeKind::default(),
            language: "zh-CN".to_string(),
            show_onboarding: true,
            waveform_buffer_capacity: 100_000,
            rawdata_buffer_capacity: 1_048_576,
            can_buffer_capacity: 50_000,
            logic_buffer_capacity: 20_000,
        }
    }
}

impl AppSettings {
    /// 配置文件路径: 系统配置目录, 失败时回退到当前目录
    fn config_path() -> PathBuf {
        directories::ProjectDirs::from("com", "vofa", "vofa-next")
            .map(|dirs| dirs.config_dir().join("settings.json"))
            .unwrap_or_else(|| PathBuf::from("settings.json"))
    }

    /// 加载设置; 文件不存在或解析失败时回退默认值
    pub fn load() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                tracing::warn!("failed to parse settings {}: {e}", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// 保存设置 (尽力而为, 失败仅记录日志)
    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::warn!("failed to create config dir {}: {e}", parent.display());
                return;
            }
        }
        match serde_json::to_string_pretty(self) {
            Ok(text) => {
                if let Err(e) = std::fs::write(&path, text) {
                    tracing::warn!("failed to write settings {}: {e}", path.display());
                }
            }
            Err(e) => tracing::warn!("failed to serialize settings: {e}"),
        }
    }
}
