//! HEX 字符串解析工具。

/// 解析 HEX 字符串为字节切片
///
/// 输入格式: "AA BB" / "AABB" / "aa bb" / "0xAA 0xBB" 均可,
/// 空格/逗号/0x 前缀均会被忽略。
///
/// 解析失败 (奇数长度 / 非法字符) 返回空 Vec。
pub fn parse_hex(hex: &str) -> Vec<u8> {
    // 过滤空白与逗号, 并移除所有 "0x" 前缀 (允许 "0xAA 0xBB" 格式)
    let cleaned: String = hex
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',')
        .collect();
    let cleaned = cleaned.replace("0x", "");
    if !cleaned.len().is_multiple_of(2) {
        return Vec::new();
    }
    (0..cleaned.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&cleaned[i..i + 2], 16).ok())
        .collect::<Option<Vec<u8>>>()
        .unwrap_or_default()
}
