//! # pipeline — Stage H 后 thin facade
//!
//! 旧子模块文件已迁至独立 crate (`pipeline_data_plane` / `pipeline_stream` /
//! `pipeline_dispatcher`)。本文件保留模块命名空间, 给 src-tauri 旧 `use crate::pipeline::*`
//! 调用路径做兼容 re-export, 新代码直接 `use pipeline_stream::*` /
//! `use pipeline_data_plane::*` 拿到具体类型。

pub mod data_plane {
    pub use pipeline_data_plane::{
        byte_router, frame_dispatch, DataPlaneMetrics, DataPlaneState, ProtocolNodeState,
        RouteSummary, METRICS_REPORT_INTERVAL, STATS_THROTTLE_MS,
    };
}

pub mod decoder_feed {
    pub use pipeline_data_plane::{
        ensure_decoder, feed_decoder_by_id, feed_one_decoder, sync_decoders_now, DecoderFeedCache,
        DecoderParseConfig,
    };
}

pub mod feed_parallel {
    pub use pipeline_data_plane::{
        workers_needed, ParallelFeeder, ParallelTiming, FEED_PARALLEL_UNIT, MAX_FEED_WORKERS,
        MIN_WORKER_BYTES,
    };
}

pub mod graph_eval {
    pub use pipeline_data_plane::{evaluate_snapshot_now, process_source_batch, EvalBreakdown};
}

pub mod stream {
    pub use pipeline_stream::{
        join_or_create_group, leave_group, sharded_stream_loop, CanStreamSource,
        DecodedStreamSource, GroupMembership, LogicStreamSource, RawDataSource, StreamSource,
        WaveformSource, MAX_STREAM_SHARDS,
    };
    pub use pipeline_stream::{adaptive_channel_loop, AdaptiveRate};
}

pub mod dispatcher {
    pub use pipeline_stream::dispatcher::*;
}

pub mod filtered_sources {
    pub use pipeline_dispatcher::filtered_sources::*;
}

pub mod spectrum_sync {
    pub use pipeline_dispatcher::{sync_ifft_buffers, sync_spectrum_analyzers};
}
