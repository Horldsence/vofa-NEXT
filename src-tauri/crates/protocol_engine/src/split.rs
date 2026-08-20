//! 帧边界并行切分算法

/// 给定升序的帧结束边界 boundaries, 把 [0, *last) 均分为 workers 段,
/// 每段尾部对齐到不超过均分点的最后一个边界; 空段自动合并 (返回块数 ≤ workers)
///
/// - boundaries 为空 → 返回空 Vec (无完整帧可切)
/// - workers <= 1 → 返回整块 [0, *last)
/// - 均分点 = last * i / workers, 每段结束 = 不超过均分点的最大边界 (最后一段恒为 *last)
pub fn split_at_boundaries(boundaries: &[usize], workers: usize) -> Vec<std::ops::Range<usize>> {
    let Some(&last) = boundaries.last() else {
        return Vec::new();
    };
    if workers <= 1 {
        return vec![0..last];
    }
    let mut ranges = Vec::with_capacity(workers);
    let mut start = 0usize;
    for i in 1..workers {
        let target = last * i / workers;
        // 不超过均分点的最大边界
        let end = match boundaries.binary_search(&target) {
            Ok(idx) => boundaries[idx],
            Err(idx) => {
                if idx == 0 {
                    continue; // 均分点之前无边界 → 空段, 与下一段合并
                }
                boundaries[idx - 1]
            }
        };
        // 空段自动合并 (相邻段结束位置相同则跳过)
        if end > start {
            ranges.push(start..end);
            start = end;
        }
    }
    if start < last {
        ranges.push(start..last);
    }
    ranges
}