//! `ai_chat_sessions.json` 读写与 `SessionStore` — 会话持久化 (目录由命令层传入)。
//!
//! 持久化形态与 `mcp_client::store` 一致:全量 JSON 覆盖写, 无文件锁 —
//! 并发安全由 `SessionStore` 内部互斥锁保证。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use error::AiError;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use vofa_core::Result;

use crate::types::{ChatSession, SessionMeta, ViewItemDto, ViewRoleDto};

/// 会话文件名 (位于 app config dir)。
pub const CONFIG_FILE_NAME: &str = "ai_chat_sessions.json";

#[derive(Debug, Default, Serialize, Deserialize)]
struct SessionsFile {
    #[serde(default)]
    sessions: Vec<ChatSession>,
}

/// 会话文件完整路径。
pub fn config_path(dir: &Path) -> PathBuf {
    dir.join(CONFIG_FILE_NAME)
}

/// 读取全部会话;文件不存在视为空列表。
///
/// # Errors
/// 文件存在但解析失败时返回 [`AiError::Persist`]。
pub fn load_sessions(dir: &Path) -> Result<Vec<ChatSession>> {
    let path = config_path(dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(&path).map_err(|source| AiError::Persist { source })?;
    let file = serde_json::from_str::<SessionsFile>(&text).map_err(|source| AiError::Persist {
        source: std::io::Error::other(source.to_string()),
    })?;
    Ok(file.sessions)
}

/// 写入全部会话 (全量覆盖)。
///
/// # Errors
/// 序列化或写文件失败时返回 [`AiError::Persist`]。
pub fn save_sessions(dir: &Path, sessions: &[ChatSession]) -> Result<()> {
    let path = config_path(dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| AiError::Persist { source })?;
    }
    let file = SessionsFile {
        sessions: sessions.to_vec(),
    };
    let text = serde_json::to_string_pretty(&file).map_err(|source| AiError::Persist {
        source: std::io::Error::other(source.to_string()),
    })?;
    fs::write(&path, text).map_err(|source| AiError::Persist { source })?;
    Ok(())
}

/// 当前 unix 毫秒时间戳。
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or_default())
}

/// 会话存储 — 全部对话会话的内存 + 磁盘所有者。
///
/// 变更在锁内生效后立即落盘;网络 IO 与文件锁均不引入
/// (与 `McpManager` 同一并发模型)。
pub struct SessionStore {
    dir: PathBuf,
    inner: Mutex<Vec<ChatSession>>,
    seq: AtomicU64,
}

impl SessionStore {
    /// 从 app config dir 构造 (加载已有会话)。
    ///
    /// # Errors
    /// 文件存在但损坏时返回 [`AiError::Persist`] (由命令层决定按空启动)。
    pub fn load(dir: &Path) -> Result<Self> {
        Ok(Self {
            dir: dir.to_path_buf(),
            inner: Mutex::new(load_sessions(dir)?),
            seq: AtomicU64::new(0),
        })
    }

    /// 空存储构造 (配置损坏时的降级启动)。
    pub fn empty(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
            inner: Mutex::new(Vec::new()),
            seq: AtomicU64::new(0),
        }
    }

    /// 落盘当前列表。
    fn persist(&self, sessions: &[ChatSession]) -> Result<()> {
        save_sessions(&self.dir, sessions)
    }

    /// 全部会话摘要 (按存储顺序)。
    pub fn list_metas(&self) -> Vec<SessionMeta> {
        self.inner
            .lock()
            .iter()
            .map(|s| SessionMeta {
                id: s.id.clone(),
                title: s.title.clone(),
                created_at: s.created_at,
                updated_at: s.updated_at,
                item_count: s.items.len(),
            })
            .collect()
    }

    /// 读取单个会话 (含全部条目)。
    pub fn get(&self, session_id: &str) -> Option<ChatSession> {
        self.inner
            .lock()
            .iter()
            .find(|s| s.id == session_id)
            .cloned()
    }

    /// 新建会话。
    ///
    /// # Errors
    /// 落盘失败返回 [`AiError::Persist`]。
    pub fn create(&self, title: &str) -> Result<ChatSession> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let id = format!(
            "sess-{nanos:x}-{:x}",
            self.seq.fetch_add(1, Ordering::Relaxed)
        );
        let now = now_ms();
        let session = ChatSession {
            id,
            title: title.to_string(),
            created_at: now,
            updated_at: now,
            items: Vec::new(),
        };
        let mut sessions = self.inner.lock();
        sessions.push(session.clone());
        self.persist(&sessions)?;
        Ok(session)
    }

    /// 重命名会话。
    ///
    /// # Errors
    /// 会话不存在 ([`AiError::UnknownSession`]) 或落盘失败。
    pub fn rename(&self, session_id: &str, title: &str) -> Result<()> {
        let mut sessions = self.inner.lock();
        let Some(session) = sessions.iter_mut().find(|s| s.id == session_id) else {
            return Err(AiError::UnknownSession {
                id: session_id.to_string(),
            }
            .into());
        };
        session.title = title.to_string();
        self.persist(&sessions)
    }

    /// 删除会话 (不存在时静默)。
    ///
    /// # Errors
    /// 落盘失败返回 [`AiError::Persist`]。
    pub fn remove(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.inner.lock();
        let before = sessions.len();
        sessions.retain(|s| s.id != session_id);
        if sessions.len() == before {
            return Ok(());
        }
        self.persist(&sessions)
    }

    /// 清空会话条目 (保留会话本身;不存在时静默)。
    ///
    /// # Errors
    /// 落盘失败返回 [`AiError::Persist`]。
    pub fn clear_items(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.inner.lock();
        let Some(session) = sessions.iter_mut().find(|s| s.id == session_id) else {
            return Ok(());
        };
        session.items.clear();
        session.updated_at = now_ms();
        self.persist(&sessions)
    }

    /// 追加视图条目 (对话回合产物) 并刷新活动时间。
    /// 会话已被删除时静默丢弃 (流式中途删会话不复活数据)。
    ///
    /// # Errors
    /// 落盘失败返回 [`AiError::Persist`]。
    pub fn append_items(&self, session_id: &str, items: Vec<ViewItemDto>) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }
        let mut sessions = self.inner.lock();
        let Some(session) = sessions.iter_mut().find(|s| s.id == session_id) else {
            log::debug!("会话 {session_id} 已删除, 丢弃 {} 条条目", items.len());
            return Ok(());
        };
        session.updated_at = now_ms();
        session.items.extend(items);
        self.persist(&sessions)
    }

    /// 截掉最后一条用户条目之后的全部条目 (重新生成前清理)。
    /// 不存在用户条目或会话缺失时不做任何修改。
    ///
    /// # Errors
    /// 落盘失败返回 [`AiError::Persist`]。
    pub fn truncate_after_last_user(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.inner.lock();
        let Some(session) = sessions.iter_mut().find(|s| s.id == session_id) else {
            return Ok(());
        };
        if let Some(pos) = session
            .items
            .iter()
            .rposition(|item| item.role == ViewRoleDto::User)
        {
            session.items.truncate(pos + 1);
            session.updated_at = now_ms();
            self.persist(&sessions)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolRunDto;
    use serde_json::json;

    fn temp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("vofa-ai-session-{tag}-{}", std::process::id()))
    }

    fn user_item(text: &str) -> ViewItemDto {
        ViewItemDto {
            role: ViewRoleDto::User,
            text: text.to_string(),
            tools: None,
            error: None,
        }
    }

    fn assistant_item(text: &str) -> ViewItemDto {
        ViewItemDto {
            role: ViewRoleDto::Assistant,
            text: text.to_string(),
            tools: None,
            error: None,
        }
    }

    #[test]
    fn roundtrip_sessions_file() {
        let dir = temp_dir("roundtrip");
        let session = ChatSession {
            id: "s1".to_string(),
            title: "调试".to_string(),
            created_at: 1,
            updated_at: 2,
            items: vec![
                user_item("你好"),
                ViewItemDto {
                    role: ViewRoleDto::Assistant,
                    text: "回答".to_string(),
                    tools: Some(vec![ToolRunDto {
                        id: "c1".to_string(),
                        name: "probe".to_string(),
                        arguments: json!({"value": 7}),
                        content: "42".to_string(),
                        is_error: false,
                        done: true,
                    }]),
                    error: None,
                },
            ],
        };

        save_sessions(&dir, &[session.clone()]).expect("保存会话");
        let loaded = load_sessions(&dir).expect("读取会话");
        assert_eq!(loaded, vec![session]);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// 不存在的目录读取返回空列表 (首次启动零会话)。
    #[test]
    fn missing_file_is_empty() {
        let dir = temp_dir("missing");
        let loaded = load_sessions(&dir).expect("缺失文件应视为空");
        assert!(loaded.is_empty());
    }

    #[test]
    fn store_create_append_and_meta() {
        let dir = temp_dir("store");
        let store = SessionStore::empty(&dir);

        let session = store.create("新会话").expect("新建会话");
        store
            .append_items(&session.id, vec![user_item("hi"), assistant_item("hello")])
            .expect("追加条目");

        let metas = store.list_metas();
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].title, "新会话");
        assert_eq!(metas[0].item_count, 2);

        let loaded = store.get(&session.id).expect("读取会话");
        assert_eq!(loaded.items.len(), 2);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn store_rename_remove_clear_truncate() {
        let dir = temp_dir("mutate");
        let store = SessionStore::empty(&dir);
        let s1 = store.create("a").expect("新建");
        let s2 = store.create("b").expect("新建");
        store
            .append_items(&s1.id, vec![user_item("hi"), assistant_item("x"), assistant_item("y")])
            .expect("追加");

        store.rename(&s1.id, "改名").expect("重命名");
        assert_eq!(store.get(&s1.id).unwrap().title, "改名");
        assert!(store.rename("nope", "x").is_err(), "未知会话应报错");

        store.truncate_after_last_user(&s1.id).expect("截断");
        assert_eq!(store.get(&s1.id).unwrap().items.len(), 1);

        store.clear_items(&s1.id).expect("清空");
        assert!(store.get(&s1.id).unwrap().items.is_empty());

        store.remove(&s2.id).expect("删除");
        assert!(store.get(&s2.id).is_none());
        // 静默语义: 重复删除 / 对缺失会话追加与清空都不报错
        store.remove(&s2.id).expect("重复删除静默");
        store
            .append_items(&s2.id, vec![user_item("hi")])
            .expect("缺失会话追加静默");
        store.clear_items(&s2.id).expect("缺失会话清空静默");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// 损坏的会话文件 → load 报 Persist 错误 (命令层降级为空启动)。
    #[test]
    fn corrupt_file_errors() {
        let dir = temp_dir("corrupt");
        std::fs::create_dir_all(&dir).expect("建目录");
        std::fs::write(config_path(&dir), "{ not json").expect("写坏文件");

        assert!(load_sessions(&dir).is_err());

        std::fs::remove_dir_all(&dir).ok();
    }
}
