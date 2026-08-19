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
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("kind", self.kind())?;
        map.serialize_entry("message", &self.to_string())?;
        map.end()
    }
}

impl Error {
    /// 枚举变体名 — 跨 IPC 传递, 前端 NodeErrorKind 与之对应
    pub fn kind(&self) -> &'static str {
        match self {
            Error::Transport(_) => "Transport",
            Error::Protocol(_) => "Protocol",
            Error::PortNotFound(_) => "PortNotFound",
            Error::PortAlreadyOpen(_) => "PortAlreadyOpen",
            Error::PortNotOpen(_) => "PortNotOpen",
            Error::Io(_) => "Io",
            Error::Config(_) => "Config",
            Error::Serde(_) => "Serde",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_tagged_enum() {
        let v = serde_json::to_value(Error::PortNotFound("/dev/ttyUSB0".into())).unwrap();
        assert_eq!(v["kind"], "PortNotFound");
        assert_eq!(v["message"], "端口未找到: /dev/ttyUSB0");
    }

    #[test]
    fn kind_matches_variant() {
        assert_eq!(Error::Transport("x".into()).kind(), "Transport");
        assert_eq!(Error::Config("x".into()).kind(), "Config");
        assert_eq!(
            Error::Io(std::io::Error::new(std::io::ErrorKind::Other, "x")).kind(),
            "Io"
        );
    }
}
