//! 逻辑分析仪解码引擎核心实现

use logic_types::{DecodedEvent, LogicDecoderConfig, LogicSample};
use protocol_engine::{FeedOutput, ProtocolEngine};

/// 逻辑分析仪解码引擎
///
/// 把接收字节流当作数字采样 (每字节 = 1 sample, bit i = 通道 i 电平),
/// 然后根据配置 (UART/I2C/SPI) 解码出协议事件。
pub struct LogicDecoderEngine {
    pub(crate) config: LogicDecoderConfig,
    /// I2C/SPI 解码用的内部采样缓冲 (跨数据包保持状态)
    pub(crate) sample_buf: Vec<LogicSample>,
    /// UART 解码状态
    pub(crate) uart_state: UartState,
    /// I2C 解码状态
    pub(crate) i2c_state: I2cState,
    /// SPI 解码状态
    pub(crate) spi_state: SpiState,
}

/// UART 解码状态
pub(crate) struct UartState {
    /// 上一次的字节时间戳 (用于去重)
    pub(crate) last_ts: u64,
}

/// I2C 解码状态机
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct I2cState {
    /// 当前 SDA 电平
    pub(crate) sda_prev: bool,
    /// 当前 SCL 电平
    pub(crate) scl_prev: bool,
    /// 移位寄存器 (8 位)
    pub(crate) shift: u8,
    /// 已接收位数
    pub(crate) bit_count: u8,
    /// 是否在传输中 (START 后, STOP 前)
    pub(crate) in_transaction: bool,
    /// 是否正在接收地址字节
    pub(crate) is_address_phase: bool,
}

/// SPI 解码状态机
pub(crate) struct SpiState {
    /// 上一次 SCLK 电平
    pub(crate) sclk_prev: bool,
    /// 上一次 CS 电平
    pub(crate) cs_prev: bool,
    /// MOSI 移位寄存器
    pub(crate) mosi_shift: u8,
    /// MISO 移位寄存器
    pub(crate) miso_shift: u8,
    /// 已接收位数
    pub(crate) bit_count: u8,
    /// 是否在传输中 (CS 低)
    pub(crate) in_transaction: bool,
}

impl LogicDecoderEngine {
    pub fn new(config: LogicDecoderConfig) -> Self {
        Self {
            config,
            sample_buf: Vec::with_capacity(4096),
            uart_state: UartState { last_ts: 0 },
            i2c_state: I2cState {
                sda_prev: true,
                scl_prev: true,
                shift: 0,
                bit_count: 0,
                in_transaction: false,
                is_address_phase: false,
            },
            spi_state: SpiState {
                sclk_prev: false,
                cs_prev: true,
                mosi_shift: 0,
                miso_shift: 0,
                bit_count: 0,
                in_transaction: false,
            },
        }
    }

    /// 获取通道位电平
    #[inline]
    pub(crate) const fn channel_bit(sample: &LogicSample, channel: u8) -> bool {
        (sample.channels >> channel) & 1 == 1
    }

    /// 当前时间戳 (微秒)
    pub(crate) fn now_us() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_micros()).unwrap_or(0))
    }
}

impl ProtocolEngine for LogicDecoderEngine {
    fn feed(&mut self, data: &[u8]) -> FeedOutput {
        let now = Self::now_us();
        let samples: Vec<LogicSample> = data
            .iter()
            .enumerate()
            .map(|(i, &b)| LogicSample {
                timestamp: now.saturating_add(i as u64),
                channels: u32::from(b),
                channel_count: 8,
            })
            .collect();
        self.sample_buf.extend(samples.iter().cloned());
        if self.sample_buf.len() > 16384 {
            let drop = self.sample_buf.len() - 8192;
            self.sample_buf.drain(..drop);
        }
        let decoded_events = match &self.config {
            LogicDecoderConfig::Uart { .. } => self.decode_uart(data),
            LogicDecoderConfig::I2c { .. } => self.decode_i2c(&samples),
            LogicDecoderConfig::Spi { .. } => self.decode_spi(&samples),
        };
        FeedOutput {
            logic_samples: samples,
            decoded_events,
            ..Default::default()
        }
    }

    fn encode_channel(&mut self, _channel: usize, _value: f32) -> Vec<u8> {
        Vec::new()
    }
    fn encode_channels(&mut self, _values: &[f32]) -> Vec<u8> {
        Vec::new()
    }
    fn name(&self) -> &'static str {
        "LogicDecoder"
    }

    fn new_worker(&self) -> Box<dyn ProtocolEngine> {
        Box::new(Self::new(self.config.clone()))
    }
}

/// 仅在 UART 配置下编译; 其他配置下返回空
pub(crate) const fn _ensure_decode_uart_method_used(_e: &LogicDecoderEngine, _d: &[u8]) -> Vec<DecodedEvent> {
    Vec::new()
}