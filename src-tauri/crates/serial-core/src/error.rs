use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("传输错误: {0}")]
    Transport(String),

    #[error("协议错误: {0}")]
    Protocol(String),

    #[error("端口未找到: {0}")]
    PortNotFound(String),

    #[error("端口已打开: {0}")]
    PortAlreadyOpen(String),

    #[error("端口未打开: {0}")]
    PortNotOpen(String),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("配置错误: {0}")]
    Config(String),

    #[error("序列化错误: {0}")]
    Serde(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl serde::Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}
