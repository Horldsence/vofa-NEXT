//! 所有运行时显示数据的唯一 Tauri 订阅入口。

mod protocol;
mod snapshot;
mod stream;

pub use protocol::{DisplayEvent, DisplayRequest, RawDataOrigin};
pub use stream::{subscribe_display, unsubscribe_display};
