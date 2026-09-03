//! 传输层错误 — 携带结构化字段(port / host / addr / id),前端 IPC 可读。

use thiserror::Error;

use crate::Error;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("列举串口失败: {0}")]
    SerialEnumeration(#[source] std::io::Error),

    #[error("打开串口 '{port}' 失败: {source}")]
    SerialOpen {
        port: String,
        #[source]
        source: std::io::Error,
    },

    #[error("克隆串口失败: {0}")]
    SerialClone(#[source] std::io::Error),

    #[error("TCP 连接 {host}:{port} 失败: {source}")]
    TcpConnect {
        host: String,
        port: u16,
        #[source]
        source: std::io::Error,
    },

    #[error("TCP 监听 {addr} 失败: {source}")]
    TcpListen {
        addr: String,
        #[source]
        source: std::io::Error,
    },

    #[error("UDP 绑定 {addr} 失败: {source}")]
    UdpBind {
        addr: String,
        #[source]
        source: std::io::Error,
    },

    #[error("UDP 连接 {addr} 失败: {source}")]
    UdpConnect {
        addr: String,
        #[source]
        source: std::io::Error,
    },

    #[error("CAN 后端发送失败: {0}")]
    CanSend(#[source] std::io::Error),

    #[error("CAN 引擎无法编码 CanFrame (id=0x{id:X}): {details}")]
    CanEncode { id: u32, details: String },

    #[error("打开 slcan 串口 '{port}' 失败: {source}")]
    SlcanOpen {
        port: String,
        #[source]
        source: std::io::Error,
    },

    #[error("设置 slcan CAN 波特率失败: {0}")]
    SlcanBitrate(#[source] std::io::Error),

    #[error("列举 USB 设备失败: {0}")]
    CandleList(#[source] std::io::Error),

    #[error("打开 candleLight 设备 '{port}' 失败: {source}")]
    CandleOpen {
        port: String,
        #[source]
        source: std::io::Error,
    },

    #[error("claim candleLight 接口失败: {0}")]
    CandleClaim(#[source] std::io::Error),

    #[error("打开 candleLight OUT 端点失败: {0}")]
    CandleOutEndpoint(#[source] std::io::Error),

    #[error("打开 candleLight IN 端点失败: {0}")]
    CandleInEndpoint(#[source] std::io::Error),

    #[error("发送失败: {0}")]
    Send(#[source] std::io::Error),

    #[error("链路配置热更新失败: {0}")]
    LinkUpdate(#[source] std::io::Error),
}

impl Error for TransportError {
    fn kind(&self) -> &'static str {
        "Transport"
    }
}
