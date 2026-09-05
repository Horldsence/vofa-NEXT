//! f32 端口 helpers — evaluate_into 热路径用,稳态零分配覆盖写
//!
//! 与 [`crate::eval_str`] 中的字符串端口 helpers 对偶,按 PortDomain::F32 分域拆分

use std::collections::HashMap;

use rustc_hash::FxBuildHasher;

use crate::ValuesMap;

/// 取节点的输出 map (不存在则创建) — 不做 clear,端口覆盖写
pub fn node_out_entry<'a>(
    out: &'a mut ValuesMap,
    node_id: &str,
) -> &'a mut HashMap<String, f32, FxBuildHasher> {
    if out.get_mut(node_id).is_none() {
        out.insert(node_id.to_string(), HashMap::default());
    }
    out.get_mut(node_id).expect("键已在上方缺失时插入")
}

/// 写端口值 — 键已存在时原位写 (零分配),不存在才插入
pub fn set_port(m: &mut HashMap<String, f32, FxBuildHasher>, port: &str, value: f32) {
    if let Some(slot) = m.get_mut(port) {
        *slot = value;
    } else {
        m.insert(port.to_string(), value);
    }
}
