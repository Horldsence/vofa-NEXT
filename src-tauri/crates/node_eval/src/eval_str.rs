//! 字符串端口 helpers — evaluate 慢路径/快照物化共用,稳态低分配覆盖写
//!
//! 与 [`crate::eval_ports`] 中的 f32 端口 helpers 对偶,按 PortDomain::String / Bytes 分域拆分

use std::collections::HashMap;

use rustc_hash::FxBuildHasher;

use crate::StringValuesMap;

/// 取节点的字符串输出 map (不存在则创建) — 仿 [`crate::eval_ports::node_out_entry`]
pub fn node_out_str_entry<'a>(
    out: &'a mut StringValuesMap,
    node_id: &str,
) -> &'a mut HashMap<String, String, FxBuildHasher> {
    if out.get_mut(node_id).is_none() {
        out.insert(node_id.to_string(), HashMap::default());
    }
    out.get_mut(node_id).unwrap()
}

/// 写字符串端口值 — 键已存在时原位写 (复用缓冲,稳态低分配),不存在才插入
pub fn set_str_port(m: &mut HashMap<String, String, FxBuildHasher>, port: &str, value: &str) {
    if let Some(slot) = m.get_mut(port) {
        slot.clear();
        slot.push_str(value);
    } else {
        m.insert(port.to_string(), value.to_owned());
    }
}
