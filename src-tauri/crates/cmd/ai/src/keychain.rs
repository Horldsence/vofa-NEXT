//! API key 系统钥匙串存取 — 密钥不入 settings.json 明文。
//!
//! 每个适配器一个条目 (`service = "vofa-next"`, `user = "ai-api-key-{adapter}"`),
//! 切换服务商互不影响。仅存取, 不做缓存 — 状态一致性由前端 settings store 负责。

use keyring::Entry;
use vofa_core::Result;

use error::AiError;

/// 钥匙串 service 标识。
const SERVICE: &str = "vofa-next";

/// 将跨平台 keyring 错误映射为稳定的 AI 错误契约。
fn map_keyring_error(error: keyring::Error) -> AiError {
    let details = error.to_string();
    if is_access_denied(&error) {
        AiError::KeyringAccessDenied { details }
    } else {
        AiError::Keyring { details }
    }
}

/// macOS Security.framework 的认证失败与用户取消是明确的授权拒绝信号。
#[cfg(target_os = "macos")]
fn is_access_denied(error: &keyring::Error) -> bool {
    const ERR_SEC_AUTH_FAILED: i32 = -25_293;
    const ERR_SEC_USER_CANCELED: i32 = -128;

    let platform_error = match error {
        keyring::Error::PlatformFailure(source) | keyring::Error::NoStorageAccess(source) => {
            source.downcast_ref::<security_framework::base::Error>()
        }
        _ => None,
    };
    platform_error
        .is_some_and(|error| matches!(error.code(), ERR_SEC_AUTH_FAILED | ERR_SEC_USER_CANCELED))
}

#[cfg(not(target_os = "macos"))]
const fn is_access_denied(_error: &keyring::Error) -> bool {
    false
}

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
        .map_err(map_keyring_error)
        .map_err(vofa_core::Error::from)
}

/// 读取适配器的 API key;未设置返回 `None`。
///
/// # Errors
/// 钥匙串访问失败 ([`AiError::Keyring`]) 或用户拒绝授权
/// ([`AiError::KeyringAccessDenied`])。
pub fn get_key(adapter: &str) -> Result<Option<String>> {
    match entry(adapter)?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(map_keyring_error(e).into()),
    }
}

/// 写入适配器的 API key (已存在则覆盖)。
///
/// # Errors
/// 钥匙串访问失败 ([`AiError::Keyring`]) 或用户拒绝授权
/// ([`AiError::KeyringAccessDenied`])。
pub fn set_key(adapter: &str, key: &str) -> Result<()> {
    entry(adapter)?
        .set_password(key)
        .map_err(map_keyring_error)
        .map_err(vofa_core::Error::from)
}

/// 删除适配器的 API key (不存在时静默)。
///
/// # Errors
/// 钥匙串访问失败 ([`AiError::Keyring`]) 或用户拒绝授权
/// ([`AiError::KeyringAccessDenied`])。
pub fn delete_key(adapter: &str) -> Result<()> {
    match entry(adapter)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(map_keyring_error(e).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::map_keyring_error;
    use error::{AiError, Error as _};

    #[test]
    fn ordinary_keyring_failure_stays_generic() {
        let error = keyring::Error::NoStorageAccess(Box::new(std::io::Error::other("locked")));
        assert_eq!(map_keyring_error(error).kind(), "AiKeyring");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_auth_failure_and_cancel_are_access_denied() {
        for code in [-25_293, -128] {
            let source = security_framework::base::Error::from_code(code);
            let error = keyring::Error::PlatformFailure(Box::new(source));
            assert!(matches!(
                map_keyring_error(error),
                AiError::KeyringAccessDenied { .. }
            ));
        }
    }
}
