//! 所有运行时显示数据的唯一 Tauri 订阅入口。

mod protocol;
mod snapshot;
mod stream;
mod waveform_binary;

pub use protocol::{DisplayEvent, DisplayRequest, RawDataOrigin, SubscriptionInfo};
pub use stream::{ack_data, get_data_health, subscribe_data, unsubscribe_data};
