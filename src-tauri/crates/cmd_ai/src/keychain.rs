//! API key 系统钥匙串存取 — 密钥不入 settings.json 明文。
//!
//! 每个适配器一个条目 (`service = "vofa-next"`, `user = "ai-api-key-{adapter}"`),
//! 切换服务商互不影响。仅存取, 不做缓存 — 状态一致性由前端 settings store 负责。

use keyring::Entry;
use vofa_core::Result;

use error::AiError;

/// 钥匙串 service 标识。
const SERVICE: &str = "vofa-next";

/// 适配器对应的钥匙串条目。
fn entry(adapter: &str) -> Result<Entry> {
    // adapter 来自白名单注册表, 但作为账户名仍防御非控制字符
    let sanitized: String = adapter
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    if sanitized.is_empty() {
        return Err(AiError::UnknownAdapter {
            adapter: adapter.to_string(),
        }
        .into());
    }
    Entry::new(SERVICE, &format!("ai-api-key-{sanitized}"))
        .map_err(|e| AiError::Keyring {
            details: e.to_string(),
        })
        .map_err(vofa_core::Error::from)
}

/// 读取适配器的 API key;未设置返回 `None`。
///
/// # Errors
/// 钥匙串访问失败 ([`AiError::Keyring`])。
pub fn get_key(adapter: &str) -> Result<Option<String>> {
    match entry(adapter)?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AiError::Keyring {
            details: e.to_string(),
        }
        .into()),
    }
}

/// 写入适配器的 API key (已存在则覆盖)。
///
/// # Errors
/// 钥匙串访问失败 ([`AiError::Keyring`])。
pub fn set_key(adapter: &str, key: &str) -> Result<()> {
    entry(adapter)?
        .set_password(key)
        .map_err(|e| AiError::Keyring {
            details: e.to_string(),
        })
        .map_err(vofa_core::Error::from)
}

/// 删除适配器的 API key (不存在时静默)。
///
/// # Errors
/// 钥匙串访问失败 ([`AiError::Keyring`])。
pub fn delete_key(adapter: &str) -> Result<()> {
    match entry(adapter)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AiError::Keyring {
            details: e.to_string(),
        }
        .into()),
    }
}
