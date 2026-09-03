//! MCP server 配置与聚合工具信息类型 (serde,与前端对齐)。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 外部 MCP server 传输方式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpTransport {
    /// stdio 子进程传输 (本地命令,如 `npx some-mcp-server`)。
    Stdio {
        /// 可执行命令。
        command: String,
        /// 命令参数。
        #[serde(default)]
        args: Vec<String>,
        /// 附加环境变量。
        #[serde(default)]
        env: HashMap<String, String>,
    },
    /// streamable-http 传输 (远程 MCP server 的 HTTP 端点)。
    Http {
        /// MCP 端点 URL (如 `http://host:8000/mcp`)。
        url: String,
    },
}

/// 外部 MCP server 配置项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// 稳定唯一 id (新增时生成,重命名不变)。
    pub id: String,
    /// 展示名 (同时用于工具名前缀)。
    pub name: String,
    /// 传输方式。
    #[serde(flatten)]
    pub transport: McpTransport,
    /// 是否启用 (禁用时不断连,仅不参与聚合)。
    #[serde(default = "default_true")]
    pub enabled: bool,
}

const fn default_true() -> bool {
    true
}

/// 聚合后的工具信息 (含前缀名,直接作为 LLM 工具规格)。
#[derive(Debug, Clone, Serialize)]
pub struct McpToolInfo {
    /// 所属 server 配置 id。
    pub server_id: String,
    /// 所属 server 展示名。
    pub server_name: String,
    /// 前缀化工具名 (`mcp_{server}_{tool}`)。
    pub prefixed_name: String,
    /// server 侧原始工具名。
    pub name: String,
    /// 工具描述。
    pub description: String,
    /// 入参 JSON Schema。
    pub input_schema: Value,
}

/// server 名 → 工具名前缀安全段 (小写、非字母数字折叠为 `_`)。
pub fn sanitize_segment(name: &str) -> String {
    let folded: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = folded.trim_matches('_');
    if trimmed.is_empty() {
        "srv".to_string()
    } else {
        trimmed.to_string()
    }
}
