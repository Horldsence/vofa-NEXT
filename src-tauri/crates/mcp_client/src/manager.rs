//! MCP 连接管理器 — 连接生命周期、工具聚合与调用。
//!
//! 并发约定:锁仅保护连接表与配置表,所有网络 IO 均在锁外执行;
//! 连接以 `Arc<RunningService>` 存储,调用时克隆句柄后立即释放锁。
//! 断连时先移出连接表再 await 取消,避免跨 await 持锁。

use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use error::{AppError, McpError};
use parking_lot::Mutex;
use rmcp::model::{CallToolRequestParams, ContentBlock};
use rmcp::service::{RunningService, RoleClient};
use rmcp::transport::{StreamableHttpClientTransport, TokioChildProcess};
use rmcp::ServiceExt;
use serde_json::Value;
use tokio::process::Command;
use vofa_core::Result;

use crate::store;
use crate::types::{McpServerConfig, McpToolInfo, McpTransport, sanitize_segment};

/// 连接建立 (含 initialize 握手) 超时秒数。
pub const CONNECTION_TIMEOUT_SECS: u64 = 10;

/// 一条已建立的外部 MCP 连接。
type Connection = RunningService<RoleClient, ()>;

/// 前缀工具名 → (server_id, 原始工具名) 的解析表。
type ToolRoute = HashMap<String, (String, String)>;

#[derive(Default)]
struct Inner {
    servers: Vec<McpServerConfig>,
    routes: ToolRoute,
}

/// 外部 MCP server 连接管理器。
pub struct McpManager {
    config_file: PathBuf,
    inner: Mutex<Inner>,
    connections: Mutex<HashMap<String, Arc<Connection>>>,
}

impl McpManager {
    /// 从指定目录加载配置 (文件不存在即为空配置)。
    pub fn load(config_dir: &Path) -> Result<Self> {
        let servers = store::load_servers(config_dir)?;
        Ok(Self::with_servers(store::config_path(config_dir), servers))
    }

    /// 空配置管理器 — 配置文件损坏时的兜底 (写盘仍指向原路径)。
    pub fn empty(config_dir: &Path) -> Self {
        Self::with_servers(store::config_path(config_dir), Vec::new())
    }

    fn with_servers(config_file: PathBuf, servers: Vec<McpServerConfig>) -> Self {
        Self {
            config_file,
            inner: Mutex::new(Inner {
                servers,
                routes: ToolRoute::default(),
            }),
            connections: Mutex::new(HashMap::new()),
        }
    }

    /// 全部 server 配置。
    pub fn list_servers(&self) -> Vec<McpServerConfig> {
        self.inner.lock().servers.clone()
    }

    /// 新增 server 配置并持久化 (id 相同则报错)。
    ///
    /// # Errors
    /// id 重复、配置非法 ([`McpError::InvalidConfig`]) 或写盘失败。
    pub fn add_server(&self, config: McpServerConfig) -> Result<()> {
        validate_config(&config)?;
        let mut inner = self.inner.lock();
        if inner.servers.iter().any(|s| s.id == config.id) {
            return Err(McpError::InvalidConfig {
                name: config.name.clone(),
                details: format!("id 已存在: {}", config.id),
            }
            .into());
        }
        inner.servers.push(config);
        self.persist(&inner)
    }

    /// 删除 server 配置 (同时断开连接) 并持久化;id 不存在时静默。
    pub fn remove_server(&self, id: &str) {
        let taken = self.connections.lock().remove(id);
        if let Some(conn) = taken {
            conn.cancellation_token().cancel();
        }
        let mut inner = self.inner.lock();
        inner.servers.retain(|s| s.id != id);
        self.persist(&inner).ok();
    }

    /// 启用 / 禁用 server 并持久化;禁用时断开现有连接。
    ///
    /// # Errors
    /// id 不存在 ([`McpError::UnknownServer`]) 或写盘失败。
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        let mut inner = self.inner.lock();
        let server = inner
            .servers
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| AppError::from(McpError::UnknownServer { id: id.to_string() }))?;
        server.enabled = enabled;
        let persisted = self.persist(&inner);
        drop(inner);

        if !enabled {
            let taken = self.connections.lock().remove(id);
            if let Some(conn) = taken {
                conn.cancellation_token().cancel();
            }
        }
        persisted
    }

    /// 连接指定 server (已连接则跳过)。
    ///
    /// # Errors
    /// 配置不存在、传输参数无效或握手失败/超时。
    pub async fn connect(&self, id: &str) -> Result<()> {
        let config = {
            let inner = self.inner.lock();
            inner
                .servers
                .iter()
                .find(|s| s.id == id)
                .cloned()
                .ok_or_else(|| AppError::from(McpError::UnknownServer { id: id.to_string() }))?
        };
        if self.connections.lock().contains_key(id) {
            return Ok(());
        }
        let conn = open_connection(&config).await?;
        self.connections.lock().insert(id.to_string(), Arc::new(conn));
        Ok(())
    }

    /// 断开指定 server 连接 (未连接时静默)。
    pub fn disconnect(&self, id: &str) {
        let taken = self.connections.lock().remove(id);
        if let Some(conn) = taken {
            conn.cancellation_token().cancel();
        }
    }

    /// 聚合所有已启用 server 的工具 (未连接的自动连接;单 server 失败跳过)。
    ///
    /// 同时重建 前缀名 → (server_id, 原始名) 路由表,供
    /// [`McpManager::call_by_prefixed`] 使用。
    pub async fn list_tools(&self) -> Vec<McpToolInfo> {
        let enabled: Vec<McpServerConfig> = {
            let inner = self.inner.lock();
            inner.servers.iter().filter(|s| s.enabled).cloned().collect()
        };

        let mut infos: Vec<McpToolInfo> = Vec::new();
        for server in enabled {
            match self.tools_of(&server).await {
                Ok(mut tools) => infos.append(&mut tools),
                Err(e) => log::warn!("MCP server [{}] 工具列举失败: {e}", server.name),
            }
        }

        // 重建路由表 (前缀冲突时追加序号保证唯一)
        let mut routes = ToolRoute::new();
        let mut used: HashMap<String, usize> = HashMap::new();
        for info in &mut infos {
            let base = format!(
                "mcp_{}_{}",
                sanitize_segment(&info.server_name),
                sanitize_segment(&info.name)
            );
            let count = used.entry(base.clone()).or_insert(0);
            *count += 1;
            info.prefixed_name = if *count == 1 {
                base.clone()
            } else {
                format!("{base}_{count}")
            };
            routes.insert(
                info.prefixed_name.clone(),
                (info.server_id.clone(), info.name.clone()),
            );
        }
        self.inner.lock().routes = routes;
        infos
    }

    /// 按前缀工具名调用 (路由到对应 server 的原始工具)。
    ///
    /// # Errors
    /// 前缀名未注册、server 未连接或远端调用失败。
    pub async fn call_by_prefixed(&self, prefixed: &str, arguments: Value) -> Result<String> {
        let (server_id, name) = {
            let inner = self.inner.lock();
            inner.routes.get(prefixed).cloned().ok_or_else(|| {
                AppError::from(McpError::ToolFailed {
                    tool: prefixed.to_string(),
                    details: "工具未注册 (server 可能刚被移除), 请刷新工具列表".to_string(),
                })
            })?
        };
        self.call_tool(&server_id, &name, arguments).await
    }

    /// 调用指定 server 的原始工具,返回文本化结果。
    ///
    /// # Errors
    /// 连接失败或远端返回 `is_error` ([`McpError::ToolFailed`])。
    pub async fn call_tool(&self, server_id: &str, name: &str, arguments: Value) -> Result<String> {
        let conn = self.ready_connection(server_id).await?;
        let mut params = CallToolRequestParams::new(name.to_string());
        params.arguments = arguments.as_object().cloned();
        let result = conn.peer().call_tool(params).await.map_err(|e| {
            McpError::ToolFailed {
                tool: name.to_string(),
                details: e.to_string(),
            }
        })?;

        let text = extract_result_text(&result);
        if result.is_error.unwrap_or(false) {
            return Err(McpError::ToolFailed {
                tool: name.to_string(),
                details: text,
            }
            .into());
        }
        Ok(text)
    }

    /// 已启用 server 的连接状态 (id → 是否已连接)。
    pub fn connection_states(&self) -> Vec<(String, bool)> {
        let inner = self.inner.lock();
        let conns = self.connections.lock();
        inner
            .servers
            .iter()
            .filter(|s| s.enabled)
            .map(|s| (s.id.clone(), conns.contains_key(&s.id)))
            .collect()
    }

    /// 持久化当前配置列表。
    fn persist(&self, inner: &Inner) -> Result<()> {
        let dir = self
            .config_file
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        store::save_servers(&dir, &inner.servers)
    }

    /// 确保连接后查询单 server 工具并转成聚合信息。
    async fn tools_of(&self, server: &McpServerConfig) -> Result<Vec<McpToolInfo>> {
        let conn = self.ready_connection(&server.id).await?;
        let tools = conn
            .peer()
            .list_all_tools()
            .await
            .map_err(|e| {
                McpError::ToolFailed {
                    tool: "*list*".to_string(),
                    details: format!("{}: {e}", server.name),
                }
            })?;
        Ok(tools
            .into_iter()
            .map(|t| McpToolInfo {
                server_id: server.id.clone(),
                server_name: server.name.clone(),
                prefixed_name: String::new(),
                name: t.name.to_string(),
                description: t.description.map(String::from).unwrap_or_default(),
                input_schema: Value::Object((*t.input_schema).clone()),
            })
            .collect())
    }

    /// 获取可用连接;不存在则按需建立并返回。
    async fn ready_connection(&self, server_id: &str) -> Result<Arc<Connection>> {
        {
            let conns = self.connections.lock();
            if let Some(conn) = conns.get(server_id) {
                return Ok(Arc::clone(conn));
            }
        }
        self.connect(server_id).await?;
        let conns = self.connections.lock();
        conns.get(server_id).cloned().ok_or_else(|| {
            AppError::from(McpError::NotConnected {
                id: server_id.to_string(),
            })
        })
    }
}

/// 配置校验 (stdio 必须有命令;http 必须是合法 URL)。
fn validate_config(config: &McpServerConfig) -> Result<()> {
    match &config.transport {
        McpTransport::Stdio { command, .. } if command.trim().is_empty() => {
            Err(McpError::InvalidConfig {
                name: config.name.clone(),
                details: "stdio 传输缺少 command".to_string(),
            }
            .into())
        }
        McpTransport::Http { url }
            if !url.starts_with("http://") && !url.starts_with("https://") =>
        {
            Err(McpError::InvalidConfig {
                name: config.name.clone(),
                details: format!("http 传输 URL 非法: {url}"),
            }
            .into())
        }
        _ => Ok(()),
    }
}

/// 按传输方式建立连接 (带握手超时)。
async fn open_connection(config: &McpServerConfig) -> Result<Connection> {
    let connect = async {
        match &config.transport {
            McpTransport::Stdio { command, args, env } => {
                let mut cmd = Command::new(command);
                cmd.args(args).envs(env);
                let transport = TokioChildProcess::new(cmd)
                    .map_err(|e| connect_error(&config.name, Box::new(e)))?;
                ().serve(transport)
                    .await
                    .map_err(|e| connect_error(&config.name, Box::new(e)))
            }
            McpTransport::Http { url } => {
                let transport = StreamableHttpClientTransport::from_uri(url.clone());
                ().serve(transport)
                    .await
                    .map_err(|e| connect_error(&config.name, Box::new(e)))
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(CONNECTION_TIMEOUT_SECS), connect)
        .await
        .map_err(|_| {
            connect_error(
                &config.name,
                Box::new(std::io::Error::new(
                    ErrorKind::TimedOut,
                    format!("握手超过 {CONNECTION_TIMEOUT_SECS}s"),
                )),
            )
        })?
}

/// 构造连接失败错误 (统一错误链)。
fn connect_error(name: &str, source: Box<dyn std::error::Error + Send + Sync>) -> AppError {
    AppError::from(McpError::Connect {
        name: name.to_string(),
        source,
    })
}

/// 工具结果 → 文本 (优先 structured_content, 否则拼接 Text 块)。
fn extract_result_text(result: &rmcp::model::CallToolResult) -> String {
    if let Some(sc) = &result.structured_content {
        if let Ok(s) = serde_json::to_string(sc) {
            return s;
        }
    }
    result
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 合法/非法配置校验。
    #[test]
    fn validate_config_checks_fields() {
        let stdio_ok = McpServerConfig {
            id: "a".to_string(),
            name: "A".to_string(),
            transport: McpTransport::Stdio {
                command: "uvx".to_string(),
                args: vec![],
                env: HashMap::default(),
            },
            enabled: true,
        };
        assert!(validate_config(&stdio_ok).is_ok());

        let http_bad = McpServerConfig {
            id: "b".to_string(),
            name: "B".to_string(),
            transport: McpTransport::Http {
                url: "ftp://nope".to_string(),
            },
            enabled: true,
        };
        assert!(validate_config(&http_bad).is_err());
    }

    /// 前缀安全段折叠。
    #[test]
    fn sanitize_segment_folds() {
        assert_eq!(sanitize_segment("My Server #1"), "my_server__1");
        assert_eq!(sanitize_segment("中文 server"), "server");
        assert_eq!(sanitize_segment("///"), "srv");
    }
}
