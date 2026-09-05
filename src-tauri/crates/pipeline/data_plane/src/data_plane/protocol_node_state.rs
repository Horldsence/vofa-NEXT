//! Protocol 节点运行时状态 — 引擎持有 + 采样时钟权威 (不变量 1)

use parking_lot::Mutex;
use protocol_engine::ProtocolEngine;
use schema_types::{ProtocolConfig, ProtocolSchema};
use std::sync::Arc;

use crate::feed_parallel::ParallelFeeder;

use logic_decoder::LogicDecoderEngine;
use protocol_can_bridge::{CandleEngine as Candle, RawDataEngine as RawData, SlcanEngine as Slcan};
use protocol_float::{FireWaterEngine as FireWater, JustFloatEngine as JustFloat};

/// 根据配置创建协议引擎
fn create_protocol_engine(config: &ProtocolConfig) -> Box<dyn ProtocolEngine> {
    match config {
        ProtocolConfig::JustFloat { channels } => Box::new(JustFloat::new(*channels)),
        ProtocolConfig::FireWater { channels } => Box::new(FireWater::new(*channels)),
        ProtocolConfig::RawData => Box::new(RawData::new()),
        ProtocolConfig::Slcan => Box::new(Slcan::new()),
        ProtocolConfig::CandleLight => Box::new(Candle::new()),
        ProtocolConfig::LogicDecode { decoder } => {
            Box::new(LogicDecoderEngine::new(decoder.clone()))
        }
        ProtocolConfig::Diagnostic { .. } => Box::new(RawData::new()),
    }
}

/// Protocol 节点运行时状态 — 生命周期跟随全局节点表 (图重编译时增删)
pub struct ProtocolNodeState {
    /// 解析引擎 (feed 同步, 锁内无 await)
    pub engine: Arc<Mutex<Box<dyn ProtocolEngine>>>,
    /// convert_to 输出引擎 (encode_frame 重编码, 协议转换链)
    pub convert_engine: Option<Arc<Mutex<Box<dyn ProtocolEngine>>>>,
    /// 当前协议配置 (set_protocol 可运行时覆盖; 图重编译时与图配置比对, 不一致则重建)
    pub config: ProtocolConfig,
    /// convert_to 目标配置
    pub convert_config: Option<ProtocolConfig>,
    /// 帧 schema (协议引擎统一为 schema 模型; None = 旧前端, 引擎按 config 构造)
    pub schema: Option<ProtocolSchema>,
    /// 并行解析编排器 (feed 内含 spawn_blocking await, 用 tokio mutex 跨 await 持有)
    pub parallel: Arc<tokio::sync::Mutex<ParallelFeeder>>,
    /// 当前是否处于并行解析模式 (顺序↔并行切换时做 pending 交接)
    pub in_parallel: bool,
    /// 协议是否支持并行解析 (None = 未探测, 空数据 split_aligned 探测一次)
    pub parallel_supported: Option<bool>,
    /// 自动通道检测通知是否已发 (一次性, 系统通知)
    pub detection_notified: bool,
    /// 上次已推送前端的自动通道检测值 (变化即推 `protocol:channels-detected`; None = 尚未推送)
    pub last_detected_pushed: Option<usize>,
    /// 来源采样时钟。线缆协议按读取批次解析时，同批样本原本共享到达时间；
    /// 这里按来源提供的精确采样率或线速估算恢复逐样本时间戳。
    sample_clock: Option<SampleClock>,
    /// TestData 广播 Lagged 后，下一批之前必须跨过的逻辑缺口（微秒）。
    pending_exact_gap_us: f64,
}

/// 采样时钟域 (数据平面不变量 1: 每源单一权威时钟, 流内不切换不混叠)
///
/// 帧时间戳 = 逻辑时间, 由**字节平面**在解析后一次性定案, 数值平面与显示端
/// 不做任何时间戳加工。到达时间只允许进入 Arrival 域 (来源无时钟声明时),
/// 且一条流的生命周期内域不可变 — 杜绝"采样时钟段"与"到达摊开段"在同一
/// 缓冲里交错 (波形折叠/畸变的根源)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleClockBasis {
    /// TestData: 配置采样率, 帧域逐样本精确推进 (rate = 帧/s)
    ExactRate,
    /// 串口: 波特率名义线速, 字节域确定推进 (rate = 字节/s, 含起止/校验位)
    SerialNominal,
}

/// 每源逻辑时钟 — 首批锁定域, 之后与到达节奏彻底解耦
enum SampleClock {
    /// 来源声明时钟: TestData 采样率 / 串口波特率线速
    Source {
        basis: SampleClockBasis,
        /// 名义速率: ExactRate = 帧/s; SerialNominal = 字节/s
        rate: f64,
        /// 下一未消费位置的逻辑时间 (µs): 帧域 = 下一帧; 字节域 = 下一字节
        next_us: f64,
        /// 首批锚点尚未消费 (首批把批尾锚定到到达时刻)
        anchored: bool,
    },
    /// 无时钟声明 (网络等): 到达域, 批内按到达区间线性摊开
    Arrival { next_us: f64 },
}

impl ProtocolNodeState {
    pub fn new(
        config: &ProtocolConfig,
        convert_to: Option<&ProtocolConfig>,
        schema: Option<&ProtocolSchema>,
    ) -> Self {
        // 有 schema 时由 compile_schema 构造引擎 (预设走 legacy 引擎, Custom 走 SchemaEngine);
        // 无 schema (旧前端) 保持原有 create_engine 路径
        let engine = schema.map_or_else(
            || create_protocol_engine(config),
            schema_engine::compile_schema,
        );
        Self {
            engine: Arc::new(Mutex::new(engine)),
            convert_engine: convert_to.map(|c| Arc::new(Mutex::new(create_protocol_engine(c)))),
            config: config.clone(),
            convert_config: convert_to.cloned(),
            schema: schema.cloned(),
            parallel: Arc::new(tokio::sync::Mutex::new(ParallelFeeder::new())),
            in_parallel: false,
            parallel_supported: None,
            detection_notified: false,
            last_detected_pushed: None,
            sample_clock: None,
            pending_exact_gap_us: 0.0,
        }
    }

    /// 记录 TestData 在解析前丢失的帧。下一批时间戳跨过该时长，避免把缺口
    /// 两侧静默拼接后造成整组波形随持续丢包逐渐漂移。
    #[allow(clippy::cast_precision_loss)] // 丢帧计数换算为逻辑微秒；远低于 f64 精确整数上限
    pub(super) fn note_exact_frame_gap(&mut self, frames: u64, sample_rate: f64) {
        if frames == 0 || !sample_rate.is_finite() || sample_rate <= 0.0 {
            return;
        }
        if matches!(
            self.sample_clock,
            Some(SampleClock::Source {
                basis: SampleClockBasis::ExactRate,
                anchored: false,
                ..
            })
        ) {
            self.pending_exact_gap_us += frames as f64 * 1_000_000.0 / sample_rate;
        }
    }

    /// 为一批帧定案逻辑时间戳 (字节平面时间权威, 不变量 1)。
    ///
    /// - `hint`: `Some((basis, rate, batch_bytes))` = 来源声明的采样时钟
    ///   (TestData = 配置采样率; 串口 = 波特率线速 + 本批字节数); `None` = 无时钟声明。
    /// - `arrival_us`: 本批到达时刻 (仅用于首批锚点与 Arrival 域)。
    ///
    /// 域规则: **首批锁定时钟域, 流内不切换**。
    /// - Source 域: 时间由名义速率确定性推进; 采样率热更新只换步长保持相位;
    ///   hint 中途缺失 (运行态查询短暂失败) 沿用已锁定速率外推, 绝不落入到达域。
    /// - Arrival 域: 批尾 = 到达时刻, 批内在 (上一批尾, 本批尾] 区间线性摊开;
    ///   即使后续出现时钟声明也保持到达域 (不与历史段混写)。
    ///
    /// 任何情况下不重锚到到达时刻、不允许时间倒退。
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub(crate) fn restamp_frames(
        &mut self,
        hint: Option<(SampleClockBasis, f64, usize)>,
        arrival_us: u64,
        frames: &mut [vofa_core::DataFrame],
    ) {
        let n = frames.len();
        if n == 0 {
            // 无帧批次: 已锁定的串口字节时钟照常推进 (被消费字节的线速时间不丢失);
            // 域未锁定时不做任何决策, 留给首个非空批次
            if let (
                Some(SampleClock::Source {
                    basis: SampleClockBasis::SerialNominal,
                    rate,
                    next_us,
                    anchored,
                }),
                Some((SampleClockBasis::SerialNominal, _, batch_bytes)),
            ) = (&mut self.sample_clock, hint)
            {
                if !*anchored && batch_bytes > 0 {
                    *next_us += f64::from(u32::try_from(batch_bytes).unwrap_or(u32::MAX))
                        * 1_000_000.0
                        / *rate;
                }
            }
            return;
        }
        if self.sample_clock.is_none() {
            // 首批锁定时钟域; 锚点 = 本批尾到达时刻 (LIVE 窗口语义)
            self.sample_clock = Some(match hint {
                Some((basis, rate, _)) => SampleClock::Source {
                    basis,
                    rate,
                    next_us: arrival_us as f64,
                    anchored: true,
                },
                None => SampleClock::Arrival {
                    next_us: arrival_us as f64,
                },
            });
        }
        let arrival = arrival_us as f64;
        match (&mut self.sample_clock, hint) {
            (
                Some(SampleClock::Source {
                    basis,
                    rate,
                    next_us,
                    anchored,
                }),
                hint_now,
            ) => {
                // hint 缺失: 沿用已锁定域与速率外推 (串口按 0 字节 = 不推进)
                let (basis_now, rate_now, batch_bytes) = hint_now.unwrap_or((*basis, *rate, 0));
                // 域变更 (同节点换传输类型, 罕见): 保持时间连续, 只换推进公式
                *basis = basis_now;
                let step_us = match basis_now {
                    SampleClockBasis::ExactRate => {
                        // 采样率热更新: 相位保持, 只替换步长
                        if (*rate - rate_now).abs() > f64::EPSILON {
                            let previous_step_us = 1_000_000.0 / *rate;
                            *next_us += 1_000_000.0 / rate_now - previous_step_us;
                        }
                        *rate = rate_now;
                        1_000_000.0 / *rate
                    }
                    SampleClockBasis::SerialNominal => {
                        *rate = rate_now;
                        // 字节域: 本批字节的线速时间均摊到批内各帧。串口线以恒定
                        // 波特率送字节, 批内每帧步长 = 每帧字节数 × 位时间; 批间由
                        // next_us 连续累积吸收帧长波动 — 确定性推进, 与到达抖动/
                        // 合批大小无关。
                        f64::from(u32::try_from(batch_bytes).unwrap_or(u32::MAX)) * 1_000_000.0
                            / (*rate * n.max(1) as f64)
                    }
                };
                // 首批: 回退本批起点使末帧恰为到达锚点, 之后时间纯逻辑推进
                let mut ts = *next_us;
                if *anchored {
                    *anchored = false;
                    // 首批左侧没有已显示样本；此前的缺口已由批尾到达锚点吸收。
                    self.pending_exact_gap_us = 0.0;
                    let span = step_us * (n - 1).min(100_000_000) as f64;
                    ts -= span;
                } else if basis_now == SampleClockBasis::ExactRate {
                    ts += std::mem::take(&mut self.pending_exact_gap_us);
                }
                for frame in frames.iter_mut() {
                    frame.timestamp = ts.max(0.0).round() as u64;
                    ts += step_us;
                }
                *next_us = ts;
            }
            (Some(SampleClock::Arrival { next_us }), _) => {
                let end = arrival.max(*next_us);
                if end > *next_us && n > 1 {
                    let span = end - *next_us;
                    for (i, frame) in frames.iter_mut().enumerate() {
                        frame.timestamp =
                            (*next_us + span * (i + 1) as f64 / n as f64).round() as u64;
                    }
                } else {
                    for frame in frames.iter_mut() {
                        frame.timestamp = end.round() as u64;
                    }
                }
                *next_us = end;
            }
            (None, _) => unreachable!("上方已初始化 sample_clock"),
        }
    }

    /// 图配置与运行时配置是否一致 (ProtocolConfig 无 PartialEq, 用 serde 值比较)
    pub(super) fn matches(
        &self,
        config: &ProtocolConfig,
        convert_to: Option<&ProtocolConfig>,
        schema: Option<&ProtocolSchema>,
    ) -> bool {
        serde_json::to_value(&self.config).ok() == serde_json::to_value(config).ok()
            && serde_json::to_value(&self.convert_config).ok()
                == serde_json::to_value(convert_to).ok()
            && serde_json::to_value(&self.schema).ok() == serde_json::to_value(schema).ok()
    }
}

#[cfg(test)]
mod sample_clock_tests {
    use super::{ProtocolNodeState, SampleClockBasis};
    use schema_types::ProtocolConfig;
    use vofa_core::DataFrame;

    fn frames(count: usize) -> Vec<DataFrame> {
        (0..count)
            .map(|_| DataFrame::with_timestamp(0, vec![1.0]))
            .collect()
    }

    #[test]
    fn exact_clock_preserves_an_explicit_lagged_gap() {
        let mut state =
            ProtocolNodeState::new(&ProtocolConfig::JustFloat { channels: Some(1) }, None, None);
        let hint = Some((SampleClockBasis::ExactRate, 1_000.0, 0));
        let mut first = frames(2);
        state.restamp_frames(hint, 10_000, &mut first);
        assert_eq!(
            first
                .iter()
                .map(|frame| frame.timestamp)
                .collect::<Vec<_>>(),
            vec![9_000, 10_000]
        );

        // 三帧在解析前丢失；下一帧与上一帧应相隔四个采样周期。
        state.note_exact_frame_gap(3, 1_000.0);
        let mut second = frames(2);
        state.restamp_frames(hint, 99_000, &mut second);
        assert_eq!(
            second
                .iter()
                .map(|frame| frame.timestamp)
                .collect::<Vec<_>>(),
            vec![14_000, 15_000]
        );
    }

    #[test]
    fn gap_before_first_visible_batch_is_absorbed_by_arrival_anchor() {
        let mut state =
            ProtocolNodeState::new(&ProtocolConfig::JustFloat { channels: Some(1) }, None, None);
        state.note_exact_frame_gap(500, 1_000.0);
        let mut first = frames(2);
        state.restamp_frames(
            Some((SampleClockBasis::ExactRate, 1_000.0, 0)),
            10_000,
            &mut first,
        );
        assert_eq!(first[1].timestamp, 10_000);
    }
}
