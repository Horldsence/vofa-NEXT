//! 工具参数解析辅助与结果字符串化

use can_types::{CanDirection, CanFrame};
use error::{AppError, McpError, Result};
use serde_json::Value;

/// 构造工具失败错误。
pub(super) fn tool_failed(tool: &str, details: impl Into<String>) -> AppError {
    McpError::ToolFailed {
        tool: tool.to_string(),
        details: details.into(),
    }
    .into()
}

/// 共享实现层 `Result<_, String>` → 工具失败。
pub(super) fn shared<T>(tool: &str, r: std::result::Result<T, String>) -> Result<T> {
    r.map_err(|e| tool_failed(tool, e))
}

/// Value → 工具结果字符串 (对象序列化, 字符串原样)。
pub(super) fn value_to_content(v: Value) -> String {
    match v {
        Value::String(s) => s,
        other => other.to_string(),
    }
}

// ============ 参数解析辅助 ============

pub(super) fn arg_str<'a>(tool: &str, args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| tool_failed(tool, format!("缺少字符串参数 {key}")))
}

pub(super) fn arg_opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

pub(super) fn arg_f64(tool: &str, args: &Value, key: &str) -> Result<f64> {
    args.get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| tool_failed(tool, format!("缺少数值参数 {key}")))
}

#[allow(clippy::cast_possible_truncation)] // MCP 工具参数按低位截断语义 (与 JS 位运算一致)
pub(super) fn arg_opt_u32(args: &Value, key: &str) -> Option<u32> {
    args.get(key).and_then(Value::as_u64).map(|v| v as u32)
}

pub(super) fn arg_i64(tool: &str, args: &Value, key: &str) -> Result<i64> {
    args.get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| tool_failed(tool, format!("缺少整数参数 {key}")))
}

// MCP 工具参数按低位截断语义 (与 JS 位运算一致)
#[allow(clippy::cast_possible_truncation)]
pub(super) fn arg_vec_u8(tool: &str, args: &Value, key: &str) -> Result<Vec<u8>> {
    args.get(key)
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_u64)
                .map(|v| v as u8)
                .collect()
        })
        .ok_or_else(|| tool_failed(tool, format!("缺少字节数组参数 {key}")))
}

/// 解析 CAN 帧参数。
#[allow(clippy::cast_possible_truncation)] // CAN id 超范围按低位截断 (与工具层约定一致)
pub(super) fn parse_can_frame(tool: &str, args: &Value) -> Result<CanFrame> {
    let frame = args
        .get("frame")
        .ok_or_else(|| tool_failed(tool, "缺少 frame 参数"))?;
    let id = frame
        .get("id")
        .and_then(Value::as_u64)
        .ok_or_else(|| tool_failed(tool, "frame.id 缺失"))? as u32;
    let data = arg_vec_u8(tool, frame, "data")?;
    let extended = frame
        .get("extended")
        .and_then(Value::as_bool)
        .unwrap_or(id > 0x7FF);
    let direction = match frame.get("direction").and_then(Value::as_str) {
        Some("tx" | "Tx" | "TX") => CanDirection::Tx,
        _ => CanDirection::Rx,
    };
    Ok(CanFrame {
        timestamp: vofa_core::now_us(),
        id,
        extended,
        rtr: frame.get("rtr").and_then(Value::as_bool).unwrap_or(false),
        dlc: u8::try_from(data.len().min(8)).unwrap_or(8),
        data: data.into_iter().take(8).collect(),
        direction,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 参数解析: 缺参报错, 数组截断/类型收窄正确。
    #[test]
    fn arg_helpers_validate() {
        let args = json!({"node_id": "transport-1", "data": [1, 300, 7]});
        assert_eq!(arg_str("t", &args, "node_id").unwrap(), "transport-1");
        let bytes = arg_vec_u8("t", &args, "data").unwrap();
        assert_eq!(bytes, vec![1, 44, 7]); // 300 截断为 u8
        assert!(arg_str("t", &args, "missing").is_err());
        assert!(arg_vec_u8("t", &args, "node_id").is_err()); // 类型不匹配
    }

    /// CAN 帧解析: id/extended 推断/方向/8 字节截断。
    #[test]
    fn can_frame_parses() {
        let args =
            json!({"frame": {"id": 0x123, "data": [9, 8, 7, 6, 5, 4, 3, 2, 1], "direction": "tx"}});
        let f = parse_can_frame("t", &args).unwrap();
        assert_eq!(f.id, 0x123);
        assert!(!f.extended);
        assert_eq!(f.direction, CanDirection::Tx);
        assert_eq!(f.data.len(), 8);
        assert_eq!(f.dlc, 8);

        let ext = json!({"frame": {"id": 0x1ABCDEF0, "data": [1]}});
        let f = parse_can_frame("t", &ext).unwrap();
        assert!(f.extended, "29 位 id 应推断为扩展帧");
        assert_eq!(f.direction, CanDirection::Rx);
    }

    /// 工具结果字符串化: 字符串去引号, 对象序列化。
    #[test]
    fn value_content_roundtrip() {
        assert_eq!(value_to_content(Value::String("ok".into())), "ok");
        assert_eq!(value_to_content(json!({"a": 1})), r#"{"a":1}"#);
    }
}
