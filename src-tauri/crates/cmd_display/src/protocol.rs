use std::collections::HashMap;

use buffer_databuffer::WaveformWindow;
use buffer_raw::RawDataBatch;
use can_types::{CanFrameBatch, CanFrameFilter, CanLoadSnapshot};
use dsp_fft::SpectrumResult;
use logic_types::{DecodedEventBatch, DecodedEventFilter, LogicSampleBatch, LogicSampleFilter};
use pipeline_data_plane::{CustomInputBatch, GraphOutputSnapshot, StringOutputSnapshot};
use serde::{Deserialize, Serialize};

/// RawData 可以来自传输节点或 FrameDecoder 节点旁路。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum RawDataOrigin {
    Transport(String),
    Decoder(String),
}

/// 显示订阅请求。过滤条件是数据源的一部分，由后端执行。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DisplayRequest {
    GraphOutputs,
    CustomInputs,
    StringOutputs,
    Spectrum,
    PortSamples {
        source_node_id: String,
        source_handle: String,
    },
    Waveform {
        source: String,
    },
    RawData {
        origin: RawDataOrigin,
        #[serde(default)]
        direction: String,
        #[serde(default)]
        search: String,
    },
    CanFrames {
        #[serde(default)]
        filter: Option<CanFrameFilter>,
    },
    LogicSamples {
        #[serde(default)]
        filter: Option<LogicSampleFilter>,
    },
    DecodedEvents {
        #[serde(default)]
        filter: Option<DecodedEventFilter>,
    },
    CanLoad {
        node_id: String,
        #[serde(default)]
        bitrate_bps: Option<u32>,
    },
}

/// 订阅建立结果。连续数值使用 binary，其余事件仍使用 json。
#[derive(Debug, Clone, Serialize)]
pub struct SubscriptionInfo {
    pub subscription_id: u32,
    pub schema_version: u16,
    pub mode: &'static str,
}

/// 单一 IPC 事件联合。serde 标签让 TypeScript 可穷尽分派。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum DisplayEvent {
    GraphOutputs(GraphOutputSnapshot),
    CustomInputs(CustomInputBatch),
    StringOutputs(StringOutputSnapshot),
    Spectrum(HashMap<String, SpectrumResult>),
    Waveform(WaveformWindow),
    RawData(RawDataBatch),
    CanFrames(CanFrameBatch),
    LogicSamples(LogicSampleBatch),
    DecodedEvents(DecodedEventBatch),
    CanLoad(CanLoadSnapshot),
}
