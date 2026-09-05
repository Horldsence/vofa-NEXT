//! 显示订阅命令测试 — display_rate 语义 + data_bus 端到端

use super::*;
use app_state::AppState;
use data_bus::{SampleStatus, TopicKey};
use std::time::Duration;
use vofa_core::DataFrame;

#[test]
fn continuous_overview_updates_respect_requested_interval() {
    let mut rate = display_rate(Duration::from_millis(100), 60);
    let mut elapsed = Duration::ZERO;
    for _ in 0..100 {
        elapsed += rate.current();
        rate.on_send();
    }
    assert_eq!(elapsed, Duration::from_secs(10));
    rate.on_idle();
    rate.on_send();
    assert_eq!(rate.current(), Duration::from_millis(100));
}

#[test]
fn display_rate_respects_detail_interval_and_preview_limit() {
    assert_eq!(
        display_rate(Duration::from_millis(33), 60).current(),
        Duration::from_millis(33)
    );
    assert_eq!(
        display_rate(Duration::ZERO, 60).current(),
        Duration::from_millis(17)
    );
    assert_eq!(
        display_rate(Duration::from_millis(16), 10).current(),
        Duration::from_millis(100)
    );
    assert_eq!(
        display_rate(Duration::ZERO, 0).current(),
        Duration::from_secs(1)
    );
    assert_eq!(
        display_rate(Duration::from_secs(2), 60).current(),
        Duration::from_secs(2)
    );
}

#[tokio::test]
async fn firewater_ch3_reaches_sample_topic_within_preview_budget() {
    let state = AppState::new();
    let key = TopicKey::new("firewater", "ch3");
    let mut receiver = state
        .data_plane
        .eval
        .data_bus
        .subscribe(key, 500)
        .await
        .unwrap();
    let frames = [
        DataFrame {
            timestamp: 10,
            channels: vec![1.0, 2.0, 3.0, 4.0],
        },
        DataFrame {
            timestamp: 11,
            channels: vec![2.0, 3.0, 4.0, 5.0],
        },
    ];
    data_plane::data_plane::frame_dispatch::on_frames(&state.data_plane, "firewater", &frames);
    let batch = tokio::time::timeout(Duration::from_millis(250), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(batch.status, SampleStatus::Live);
    assert_eq!(
        batch
            .samples
            .iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>(),
        vec![4.0, 5.0]
    );
}

#[tokio::test]
async fn out_of_range_topic_reports_status_without_zero_sample() {
    let state = AppState::new();
    let key = TopicKey::new("firewater", "ch9");
    let mut receiver = state
        .data_plane
        .eval
        .data_bus
        .subscribe(key, 500)
        .await
        .unwrap();
    data_plane::data_plane::frame_dispatch::on_frames(
        &state.data_plane,
        "firewater",
        &[DataFrame {
            timestamp: 10,
            channels: vec![1.0, 2.0, 3.0, 4.0],
        }],
    );
    let batch = receiver.recv().await.unwrap();
    assert!(batch.samples.is_empty());
    assert_eq!(
        batch.status,
        SampleStatus::ChannelOutOfRange {
            requested: 9,
            available: 4,
        }
    );
}
