//! 算术运算 — MathOp 枚举 + evaluate 方法
//!
//! 与前端 types/index.ts 中的 MathOp 保持一致 (snake_case)

use serde::{Deserialize, Serialize};

/// 算术运算种类
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MathOp {
    Add,
    Sub,
    Mul,
    Div,
    Avg,
    Min,
    Max,
    Abs,
    Neg,
    Square,
    Sqrt,
    Sin,
    Cos,
    Tan,
    Log,
}

impl MathOp {
    /// 评估算术运算 — 输入数组, 输出单值
    /// 与前端 computeMathResult 保持一致的语义
    pub fn evaluate(&self, inputs: &[f32]) -> f32 {
        // 过滤 NaN
        let vals: Vec<f32> = inputs.iter().copied().filter(|v| !v.is_nan()).collect();
        if vals.is_empty() {
            return 0.0;
        }
        match self {
            MathOp::Add => vals.iter().sum(),
            MathOp::Sub => vals.iter().copied().reduce(|a, b| a - b).unwrap_or(0.0),
            MathOp::Mul => vals.iter().copied().reduce(|a, b| a * b).unwrap_or(1.0),
            MathOp::Div => vals
                .iter()
                .copied()
                .skip(1)
                .fold(vals[0], |a, b| if b == 0.0 { 0.0 } else { a / b }),
            MathOp::Avg => vals.iter().sum::<f32>() / vals.len() as f32,
            MathOp::Min => vals.iter().copied().fold(f32::INFINITY, f32::min),
            MathOp::Max => vals.iter().copied().fold(f32::NEG_INFINITY, f32::max),
            MathOp::Abs => vals[0].abs(),
            MathOp::Neg => -vals[0],
            MathOp::Square => vals[0] * vals[0],
            MathOp::Sqrt => {
                if vals[0] < 0.0 {
                    0.0
                } else {
                    vals[0].sqrt()
                }
            }
            MathOp::Sin => vals[0].sin(),
            MathOp::Cos => vals[0].cos(),
            MathOp::Tan => vals[0].tan(),
            MathOp::Log => {
                if vals[0] <= 0.0 {
                    0.0
                } else {
                    vals[0].ln()
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        assert!((MathOp::Add.evaluate(&[1.0, 2.0, 3.0]) - 6.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_sub() {
        assert!((MathOp::Sub.evaluate(&[10.0, 3.0, 2.0]) - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_mul() {
        assert!((MathOp::Mul.evaluate(&[2.0, 3.0, 4.0]) - 24.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_div_zero_returns_zero() {
        assert_eq!(MathOp::Div.evaluate(&[10.0, 0.0]), 0.0);
    }

    #[test]
    fn test_div_chain() {
        assert!((MathOp::Div.evaluate(&[100.0, 5.0, 2.0]) - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_avg() {
        assert!((MathOp::Avg.evaluate(&[1.0, 2.0, 3.0]) - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_min_max() {
        assert_eq!(MathOp::Min.evaluate(&[3.0, 1.0, 2.0]), 1.0);
        assert_eq!(MathOp::Max.evaluate(&[3.0, 1.0, 2.0]), 3.0);
    }

    #[test]
    fn test_unary() {
        assert_eq!(MathOp::Abs.evaluate(&[-5.0]), 5.0);
        assert_eq!(MathOp::Neg.evaluate(&[5.0]), -5.0);
        assert_eq!(MathOp::Square.evaluate(&[3.0]), 9.0);
        assert!((MathOp::Sqrt.evaluate(&[9.0]) - 3.0).abs() < f32::EPSILON);
        assert_eq!(MathOp::Sqrt.evaluate(&[-1.0]), 0.0);
    }

    #[test]
    fn test_trig() {
        assert!((MathOp::Sin.evaluate(&[0.0])).abs() < f32::EPSILON);
        assert!((MathOp::Cos.evaluate(&[0.0]) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_log() {
        assert!((MathOp::Log.evaluate(&[1.0])).abs() < f32::EPSILON);
        assert_eq!(MathOp::Log.evaluate(&[-1.0]), 0.0);
        assert_eq!(MathOp::Log.evaluate(&[0.0]), 0.0);
    }

    #[test]
    fn test_empty_inputs() {
        assert_eq!(MathOp::Add.evaluate(&[]), 0.0);
    }

    #[test]
    fn test_nan_filtered() {
        assert_eq!(MathOp::Add.evaluate(&[1.0, f32::NAN, 2.0]), 3.0);
    }
}
