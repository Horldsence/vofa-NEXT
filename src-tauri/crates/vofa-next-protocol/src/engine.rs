use vofa_next_core::{CanFrame, DataFrame, DecodedEvent, LogicSample};

/// 协议引擎 trait — 解析接收数据 / 编码发送数据
pub trait ProtocolEngine: Send {
    /// 喂入原始字节流, 返回解析出的数据帧列表
    fn feed(&mut self, data: &[u8]) -> Vec<DataFrame>;

    /// 编码单通道值为字节流 (用于自动绑定模式发送)
    fn encode_channel(&mut self, channel: usize, value: f32) -> Vec<u8>;

    /// 编码多通道值 (一次性发送所有通道)
    fn encode_channels(&mut self, values: &[f32]) -> Vec<u8>;

    /// 协议名称
    fn name(&self) -> &str;

    /// 自动检测到的通道数 (自动模式下, 收到首帧后返回 Some(n))
    /// 手动模式或未检测到时返回 None
    fn detected_channels(&self) -> Option<usize> {
        None
    }

    /// 是否为自动检测模式
    fn is_auto_mode(&self) -> bool {
        false
    }

    /// 解析 CAN 帧 (仅 Slcan/CandleLight 引擎重写)
    fn feed_can(&mut self, _data: &[u8]) -> Vec<CanFrame> {
        Vec::new()
    }

    /// 编码 CAN 帧为传输字节 (仅 Slcan/CandleLight 引擎重写)
    fn encode_can(&mut self, _frame: &CanFrame) -> Vec<u8> {
        Vec::new()
    }

    /// 解析逻辑分析仪采样 (仅 LogicDecoder 引擎重写)
    fn feed_logic(&mut self, _data: &[u8]) -> Vec<LogicSample> {
        Vec::new()
    }

    /// 解析协议解码事件 (仅 LogicDecoder 引擎重写)
    /// 输入原始字节流, 输出 UART/I2C/SPI 解码事件
    fn feed_decoded(&mut self, _data: &[u8]) -> Vec<DecodedEvent> {
        Vec::new()
    }
}
