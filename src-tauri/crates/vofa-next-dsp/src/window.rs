//! 窗函数 — 用于 FFT 前的加窗处理, 减少频谱泄漏
//!
//! 支持 4 种常用窗:
//! - Rect (矩形窗): 等同不加窗, 适用于瞬态信号
//! - Hann (汉宁窗): 通用, 主瓣宽旁瓣低
//! - Hamming (汉明窗): 类似 Hann 但端点不为零
//! - Blackman (布莱克曼窗): 主瓣更宽但旁瓣更低

use serde::{Deserialize, Serialize};

/// 窗函数类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WindowType {
    /// 矩形窗 (不加窗)
    Rect,
    /// 汉宁窗 — 通用, 主瓣宽旁瓣低
    #[default]
    Hann,
    /// 汉明窗 — 端点不为零
    Hamming,
    /// 布莱克曼窗 — 主瓣最宽, 旁瓣最低
    Blackman,
}

impl WindowType {
    /// 返回窗函数在归一化索引 [0, 1) 处的系数 (periodic 形式, 适用于 FFT)
    /// n: 当前样本索引 (0..N)
    /// n_total: 窗长度 N
    ///
    /// 使用 periodic 形式 (t = n/N) 而非 symmetric (t = n/(N-1)),
    /// 因为 FFT 假设信号是周期性的, 且 periodic 形式的相干增益为精确值
    /// (Hann=0.5, Hamming=0.54, Blackman=0.42)。
    fn coeff(&self, n: usize, n_total: usize) -> f32 {
        if n_total <= 1 {
            return 1.0;
        }
        let t = n as f32 / n_total as f32; // 0..1 (periodic: 不含 1.0)
        let two_pi = 2.0 * std::f32::consts::PI;
        match self {
            WindowType::Rect => 1.0,
            WindowType::Hann => 0.5 * (1.0 - (two_pi * t).cos()),
            WindowType::Hamming => 0.54 - 0.46 * (two_pi * t).cos(),
            WindowType::Blackman => 0.42 - 0.5 * (two_pi * t).cos() + 0.08 * (4.0 * std::f32::consts::PI * t).cos(),
        }
    }

    /// 窗的相干增益 (rect=1, Hann=0.5, Hamming=0.54, Blackman=0.42)
    /// 用于 PSD 计算时的归一化
    pub fn coherent_gain(&self, n_total: usize) -> f32 {
        if n_total == 0 {
            return 1.0;
        }
        let mut sum = 0.0;
        for n in 0..n_total {
            sum += self.coeff(n, n_total);
        }
        sum / n_total as f32
    }
}

/// 对数据原地应用窗函数 (data.len() 决定窗长度)
///
/// # 示例
/// ```
/// use vofa_next_dsp::{apply_window, WindowType};
/// let mut data = vec![1.0; 1024];
/// apply_window(&WindowType::Hann, &mut data);
/// // 端点接近 0, 中间接近 1
/// assert!(data[0] < 0.01);
/// assert!((data[512] - 1.0).abs() < 0.01);
/// ```
pub fn apply_window(window: &WindowType, data: &mut [f32]) {
    let n = data.len();
    for (i, sample) in data.iter_mut().enumerate() {
        *sample *= window.coeff(i, n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_window_no_change() {
        let mut data = vec![1.0, 2.0, 3.0, 4.0];
        apply_window(&WindowType::Rect, &mut data);
        assert_eq!(data, vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_hann_window_endpoints_zero() {
        let mut data = vec![1.0; 100];
        apply_window(&WindowType::Hann, &mut data);
        // 端点应接近 0
        assert!(data[0].abs() < 0.01);
        assert!(data[99].abs() < 0.01);
        // 中间应接近 1
        assert!((data[50] - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_hamming_endpoints_not_zero() {
        let mut data = vec![1.0; 100];
        apply_window(&WindowType::Hamming, &mut data);
        // Hamming 端点不为 0 (约 0.08)
        assert!(data[0] > 0.05);
        assert!(data[0] < 0.12);
    }

    #[test]
    fn test_blackman_endpoints_near_zero() {
        let mut data = vec![1.0; 100];
        apply_window(&WindowType::Blackman, &mut data);
        // Blackman 端点应非常接近 0
        assert!(data[0].abs() < 0.001);
    }

    #[test]
    fn test_coherent_gain() {
        // Rect 增益 = 1
        assert!((WindowType::Rect.coherent_gain(100) - 1.0).abs() < 1e-6);
        // Hann 增益 = 0.5
        assert!((WindowType::Hann.coherent_gain(100) - 0.5).abs() < 1e-3);
        // Hamming 增益 = 0.54
        assert!((WindowType::Hamming.coherent_gain(100) - 0.54).abs() < 1e-3);
        // Blackman 增益 = 0.42
        assert!((WindowType::Blackman.coherent_gain(100) - 0.42).abs() < 1e-3);
    }

    #[test]
    fn test_window_length_one() {
        let mut data = vec![5.0];
        apply_window(&WindowType::Hann, &mut data);
        assert_eq!(data[0], 5.0); // 长度 1 时返回 1.0 系数
    }

    #[test]
    fn test_window_empty() {
        let mut data: Vec<f32> = vec![];
        apply_window(&WindowType::Hann, &mut data);
        assert!(data.is_empty());
    }
}
