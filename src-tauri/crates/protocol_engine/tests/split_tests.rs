//! 集成测试: `split_at_boundaries`

use protocol_engine::split_at_boundaries;

#[test]
fn split_at_boundaries_even_split() {
    // 均分对齐: 4 个边界分 2 段, 均分点 8 恰好落在边界上
    let ranges = split_at_boundaries(&[4, 8, 12, 16], 2);
    assert_eq!(ranges, vec![0..8, 8..16]);
}

#[test]
fn split_at_boundaries_aligns_down() {
    // 均分点不对齐边界时向下对齐: last=15, workers=2, 均分点 7 → 段结束对齐到 5
    let ranges = split_at_boundaries(&[5, 10, 15], 2);
    assert_eq!(ranges, vec![0..5, 5..15]);
}

#[test]
fn split_at_boundaries_fewer_boundaries_than_workers() {
    // 边界数少于 workers: 空段自动合并
    let ranges = split_at_boundaries(&[4, 8], 4);
    assert_eq!(ranges, vec![0..4, 4..8]);
}

#[test]
fn split_at_boundaries_empty_returns_empty() {
    // 空 boundaries → 空 Vec (无完整帧)
    assert!(split_at_boundaries(&[], 4).is_empty());
}

#[test]
fn split_at_boundaries_single_worker_returns_full_block() {
    // workers=1 → 整块
    let ranges = split_at_boundaries(&[4, 8, 12], 1);
    assert_eq!(ranges, vec![0..12]);
}

#[test]
fn split_at_boundaries_workers_zero() {
    // workers=0 等价于 <=1 → 整块 (行为与 1 相同, 不 panic)
    let ranges = split_at_boundaries(&[3, 6, 9], 0);
    assert_eq!(ranges, vec![0..9]);
}

#[test]
fn split_at_boundaries_3_workers() {
    // last=12, workers=3 → 均分点 4, 8
    // 边界 [3,6,9,12]: 4 之前最大边界=3; 8 之前最大边界=6; 末段 12
    // 向下对齐到 3 / 6 后空段合并产生 [0..3, 3..6, 6..12]
    let ranges = split_at_boundaries(&[3, 6, 9, 12], 3);
    assert_eq!(ranges, vec![0..3, 3..6, 6..12]);
}