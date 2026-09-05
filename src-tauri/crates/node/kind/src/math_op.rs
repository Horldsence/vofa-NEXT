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
    #[allow(clippy::cast_precision_loss)]
    pub fn evaluate(&self, inputs: &[f32]) -> f32 {
        let values = inputs.iter().copied().filter(|v| !v.is_nan());
        let Some(first) = values.clone().next() else {
            return 0.0;
        };
        match self {
            Self::Add => values.sum(),
            Self::Sub => values.reduce(|a, b| a - b).unwrap_or(0.0),
            Self::Mul => values.reduce(|a, b| a * b).unwrap_or(1.0),
            Self::Div => values
                .skip(1)
                .fold(first, |a, b| if b == 0.0 { 0.0 } else { a / b }),
            Self::Avg => values.clone().sum::<f32>() / values.count() as f32,
            Self::Min => values.fold(f32::INFINITY, f32::min),
            Self::Max => values.fold(f32::NEG_INFINITY, f32::max),
            Self::Abs => first.abs(),
            Self::Neg => -first,
            Self::Square => first * first,
            Self::Sqrt => {
                if first < 0.0 {
                    0.0
                } else {
                    first.sqrt()
                }
            }
            Self::Sin => first.sin(),
            Self::Cos => first.cos(),
            Self::Tan => first.tan(),
            Self::Log => {
                if first <= 0.0 {
                    0.0
                } else {
                    first.ln()
                }
            }
        }
    }
}

#[cfg(test)]
// 测试结果均为精确可表示的值 (小整数 f32), 直接断言相等
#[allow(clippy::float_cmp)]
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
