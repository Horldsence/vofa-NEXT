//! # 原始数据过滤 — 用于后端按方向/搜索词过滤后再推送到前端
//!
//! 目标: 在 Rust 后端完成方向过滤与内容搜索, 让前端只接收需要显示的数据,
//! 从而支持 20MB/s 以上的高码率场景。

use crate::raw::StoredChunk;
use crate::RawDataDirection;

/// 方向过滤条件
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DirectionFilter {
    /// 同时显示接收与发送
    #[default]
    All,
    /// 仅接收
    Rx,
    /// 仅发送
    Tx,
}

impl DirectionFilter {
    /// 判断指定方向是否命中本过滤条件
    pub fn matches(self, dir: RawDataDirection) -> bool {
        match self {
            DirectionFilter::All => true,
            DirectionFilter::Rx => dir == RawDataDirection::Rx,
            DirectionFilter::Tx => dir == RawDataDirection::Tx,
        }
    }
}

/// 搜索模式 — 已解析为字节数组
#[derive(Debug, Clone)]
pub struct SearchPattern(Vec<u8>);

impl SearchPattern {
    /// 解析用户输入:
    /// - 空串或纯空白 -> None (不过滤)
    /// - 只含十六进制字符与空白 -> 按 hex 解析 (支持 `31 32` 或 `3132`)
    /// - 其他 -> 按 UTF-8 字符串解析
    pub fn parse(input: &str) -> Option<Self> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return None;
        }

        let is_hex = trimmed
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c.is_whitespace());

        if is_hex {
            let mut bytes = Vec::new();
            for token in trimmed.split_whitespace() {
                let token = token.as_bytes();
                let mut i = 0;
                while i < token.len() {
                    let end = (i + 2).min(token.len());
                    let s = std::str::from_utf8(&token[i..end]).ok()?;
                    bytes.push(u8::from_str_radix(s, 16).ok()?);
                    i += 2;
                }
            }
            if !bytes.is_empty() {
                return Some(Self(bytes));
            }
        }

        Some(Self(trimmed.as_bytes().to_vec()))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// 在数据中查找是否包含本模式
    pub fn matches(&self, data: &[u8]) -> bool {
        if self.0.is_empty() {
            return true;
        }
        if self.0.len() == 1 {
            return data.contains(&self.0[0]);
        }
        data.windows(self.0.len()).any(|w| w == self.0.as_slice())
    }
}

/// 判断一个 chunk 是否通过方向与搜索过滤
///
/// `prev_tail` 是上一个通过方向过滤的 chunk 的末尾 `pattern.len() - 1` 字节,
/// 用于处理搜索模式跨 chunk 边界的情况。
pub(crate) fn chunk_matches(
    chunk: &StoredChunk,
    direction: DirectionFilter,
    pattern: Option<&SearchPattern>,
    prev_tail: &[u8],
) -> (bool, Vec<u8>) {
    let dir_ok = direction.matches(chunk.direction);
    if !dir_ok {
        // 方向不匹配时不输出, 也不应作为跨 chunk tail
        return (false, Vec::new());
    }

    let search_ok = match pattern {
        None => true,
        Some(p) => {
            if p.is_empty() {
                true
            } else {
                let combined = [prev_tail, &chunk.bytes].concat();
                p.matches(&combined)
            }
        }
    };

    let new_tail = chunk
        .bytes
        .iter()
        .rev()
        .take(pattern.map(|p| p.len().saturating_sub(1)).unwrap_or(0))
        .rev()
        .copied()
        .collect();

    (search_ok, new_tail)
}
