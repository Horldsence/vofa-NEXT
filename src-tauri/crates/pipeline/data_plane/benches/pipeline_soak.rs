//! 墙钟长跑：限速字节摄入、异步图求值、30 Hz 主图/概览查询和样本订阅并发。
//! 不包含 Tauri IPC 与 WebView 绘制；输出 JSON 供性能审计，不冒充桌面 FPS。
#![allow(clippy::cast_precision_loss)]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use app_state::AppState;
use buffer_databuffer::{DerivedSeriesSelector, WaveformSeriesSelection};
use data_bus::TopicKey;
use data_plane::{byte_router::route_bytes, DataPlaneState, DecoderFeedCache};
use engine::{BytePlan, CompiledGraph};
use kind::{NodeDef, NodeKind};
use schema_types::{ProtocolConfig, TestDataLink};
use testkit::{edge, make_math, make_protocol_source, make_sink};
use vofa_core::{TestDataConfig, TestSignal, TransportConfig};

fn setup(computed: bool) -> DataPlaneState {
    let plane = AppState::new().data_plane;
    let nodes = vec![
        NodeDef {
            id: "tp".into(),
            tab_id: "t".into(),
            kind: NodeKind::Transport {
                config: TransportConfig::TestData(config()),
            },
        },
        NodeDef {
            id: "pt".into(),
            tab_id: "t".into(),
            kind: NodeKind::Protocol {
                config: protocol(),
                convert_to: None,
                schema: None,
            },
        },
    ];
    let typed =
        engine::TypedGraph::build(nodes.clone(), vec![edge("wire", "tp", "rx", "pt", "in")])
            .unwrap();
    *plane.byte_plan.lock() = BytePlan::build(&typed).unwrap();
    plane
        .global_nodes
        .lock()
        .extend(nodes.into_iter().map(|n| (n.id.clone(), n)));
    plane.sync_protocol_states();
    let mut nodes = vec![
        make_protocol_source("ps", "t", "pt", 4),
        make_sink("wave", "t"),
    ];
    let mut edges = Vec::new();
    for ch in 0..4 {
        if computed {
            let id = format!("math{ch}");
            nodes.push(make_math(&id, "t", kind::MathOp::Add, 1));
            edges.push(edge(
                &format!("in{ch}"),
                "ps",
                &format!("ch{ch}"),
                &id,
                "in0",
            ));
            edges.push(edge(
                &format!("out{ch}"),
                &id,
                "result",
                "wave",
                &format!("CH{ch}"),
            ));
        } else {
            edges.push(edge(
                &format!("out{ch}"),
                "ps",
                &format!("ch{ch}"),
                "wave",
                &format!("CH{ch}"),
            ));
        }
    }
    plane.eval.graphs.lock().insert(
        "t".into(),
        CompiledGraph::compile("t".into(), nodes, edges).unwrap(),
    );
    plane
}

const fn config() -> TestDataConfig {
    TestDataConfig {
        channels: 4,
        sample_rate: 700_000.0,
        signal: TestSignal::Sine,
    }
}

const fn protocol() -> ProtocolConfig {
    ProtocolConfig::JustFloat { channels: Some(4) }
}

fn summary(mut samples: Vec<f64>) -> serde_json::Value {
    samples.sort_by(f64::total_cmp);
    let at = |percent: usize| {
        samples
            .get(samples.len().saturating_sub(1) * percent / 100)
            .copied()
            .unwrap_or(0.0)
    };
    serde_json::json!({"count": samples.len(), "p50_ms": at(50), "p95_ms": at(95), "p99_ms": at(99), "max_ms": at(100)})
}

fn main() {
    let seconds: u64 =
        std::env::var("VOFA_SOAK_SECONDS").map_or(60, |s| s.parse().expect("seconds"));
    let computed = std::env::var("VOFA_SOAK_GRAPH").is_ok_and(|s| s == "math");
    let generator = std::env::var("VOFA_SOAK_GENERATOR").is_ok_and(|s| s == "1");
    let stall_ms: u64 = std::env::var("VOFA_SOAK_EVAL_STALL_MS")
        .map_or(0, |s| s.parse().expect("stall milliseconds"));
    let ingest_stall_ms: u64 = std::env::var("VOFA_SOAK_INGEST_STALL_MS")
        .map_or(0, |s| s.parse().expect("ingest stall milliseconds"));
    assert!(seconds > 0, "长跑时间必须大于零");
    assert!(stall_ms <= 1_000, "人为图锁停顿最多 1 秒");
    assert!(ingest_stall_ms <= 1_000, "人为摄入停顿最多 1 秒");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    let plane = setup(computed);
    if let Ok(workers) = std::env::var("VOFA_SOAK_EVAL_WORKERS") {
        let workers: usize = workers.parse().expect("eval workers");
        assert!((1..=16).contains(&workers), "评估线程数范围为 1..=16");
        plane.pipeline_config.write().eval_workers = workers;
    }
    println!(
        "{}",
        serde_json::json!({"benchmark_config": {
            "sample_rate":config().sample_rate, "channels":config().channels,
        "pipeline":*plane.pipeline_config.read(), "injected_stall_ms":stall_ms,
        "injected_ingest_stall_ms":ingest_stall_ms,
            "seconds":seconds, "generator":generator, "computed":computed,
        }})
    );
    let stop = Arc::new(AtomicBool::new(false));
    // 单独报告的干扰场景，不改变正常长跑或生产代码。每 5 秒模拟一次图锁被占用。
    let stalls = if stall_ms > 0 {
        let stall_plane = plane.clone();
        let stall_stop = stop.clone();
        Some(std::thread::spawn(move || {
            let mut count = 0;
            loop {
                std::thread::park_timeout(Duration::from_secs(5));
                if stall_stop.load(Ordering::Relaxed) {
                    return count;
                }
                let _graphs = stall_plane.eval.graphs.lock();
                std::thread::sleep(Duration::from_millis(stall_ms));
                count += 1;
            }
        }))
    } else {
        None
    };
    let query_plane = plane.clone();
    let query_stop = stop.clone();
    let queries = std::thread::spawn(move || {
        let buffer = query_plane.buffer_for("pt");
        let mut times = Vec::new();
        let mut latest = 0;
        let mut next = Instant::now();
        while !query_stop.load(Ordering::Relaxed) {
            let start = Instant::now();
            let (detail, overview) = {
                let b = buffer.lock();
                let selection = WaveformSeriesSelection {
                    channels: (0..4).collect(),
                    derived: if computed {
                        (0..4)
                            .map(|ch| DerivedSeriesSelector {
                                sink_id: "wave".into(),
                                source_id: format!("math{ch}"),
                                source_handle: "result".into(),
                            })
                            .collect()
                    } else {
                        vec![]
                    },
                };
                (
                    b.snapshot_window_budget(-2_000.0, 0.0, &selection, 12_000),
                    b.snapshot_all_budget(2_000),
                )
            };
            let detail = detail.into_min_max(12_000);
            let overview = overview.into_min_max(2_000);
            assert!(detail.latest_timestamp_us >= latest, "原始时间不得倒退");
            latest = detail.latest_timestamp_us;
            assert_eq!(
                detail.latest_timestamp_us, overview.latest_timestamp_us,
                "主图和概览必须共享原始锚点"
            );
            for window in [&detail, &overview] {
                assert!(window.timestamps.windows(2).all(|w| w[0] <= w[1]));
                assert!(window
                    .channels
                    .iter()
                    .all(|c| c.is_empty() || c.len() == window.timestamps.len()));
                if latest > 0 {
                    // 空数组也满足 windows().all()；必须独立验证窗口没有塌缩或消失。
                    assert!(window.timestamps.len() >= 2, "已有数据的窗口不得消失");
                    assert_eq!(window.channels.len(), 4);
                    assert!(
                        window
                            .channels
                            .iter()
                            .all(|c| c.len() == window.timestamps.len()
                                && c.iter().all(|v| v.is_finite())),
                        "原始通道不得缺列或出现非有限值"
                    );
                }
            }
            std::hint::black_box((detail, overview));
            times.push(start.elapsed().as_secs_f64() * 1_000.0);
            next += Duration::from_nanos(1_000_000_000 / 30);
            std::thread::sleep(next.saturating_duration_since(Instant::now()));
        }
        times
    });
    let (lagged, diag, throughput, preview_lagged) = runtime.block_on(async {
        // 真实订阅触发端口 staging、DataBus 批处理和消费路径。
        let key = TopicKey::new(if computed { "math0" } else { "pt" }, if computed { "result" } else { "ch0" });
        let mut preview = plane.eval.data_bus.subscribe(key.clone(), 0).await.unwrap();
        let preview_batches = Arc::new(AtomicU64::new(0));
        let preview_lagged = Arc::new(AtomicU64::new(0));
        let batch_count = preview_batches.clone();
        let skipped_count = preview_lagged.clone();
        let bus = plane.eval.data_bus.clone();
        let preview_key = key.clone();
        let preview_task = tokio::spawn(async move {
            loop {
                match preview.recv().await {
                    Ok(batch) => {
                        bus.ack(&preview_key, batch.sequence, 0, 0.0);
                        std::hint::black_box(batch);
                        batch_count.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => { skipped_count.fetch_add(n, Ordering::Relaxed); }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        // 连接即生成: generator 场景 spawn 后数据流即持续产出;
        // 非 generator 场景不启动生成器 (其 CPU 负载会污染注入基线), 由本地节拍合成 chunk。
        let (mut rx, cancel) = if generator {
            let (_write, tx, cancel, _watch) = transport_core::test_data::spawn(config(), TestDataLink { protocol: protocol(), schema: None }).unwrap();
            (Some(tx.subscribe()), Some(cancel))
        } else {
            (None, None)
        };
        let chunk: Vec<u8> = (0..3_500_u64).flat_map(|i| transport_core::test_data::generate_bytes(4, TestSignal::Sine, i as f64 / 700_000.0, &protocol(), i)).collect();
        let start = Instant::now();
        let mut cache = DecoderFeedCache::new();
        let mut bytes = 0_u64;
        let mut frames = 0_u64;
        let mut lagged = 0_u64;
        let mut latencies = Vec::new();
        let mut max_queue_frames = 0;
        let mut max_queue_bytes = 0;
        let mut tick_bytes = 0;
        let mut tick = Instant::now();
        let mut next = tokio::time::Instant::now();
        let mut last_ingest_stall = Instant::now();
        let mut ingest_stalls = 0;
        while start.elapsed() < Duration::from_secs(seconds) {
            if ingest_stall_ms > 0 && last_ingest_stall.elapsed() >= Duration::from_secs(5) {
                // 生成器继续工作，恢复后真实接收广播积压，检验求值对突发补交的承载。
                tokio::time::sleep(Duration::from_millis(ingest_stall_ms)).await;
                last_ingest_stall = Instant::now();
                ingest_stalls += 1;
            }
            let data = if let Some(rx) = &mut rx {
                match tokio::time::timeout(Duration::from_secs(2), rx.recv()).await.expect("生成器超时") {
                    Ok(data) => data,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => { lagged += n; continue; }
                    Err(e) => panic!("{e}"),
                }
            } else {
                tokio::time::sleep_until(next).await;
                next += Duration::from_millis(5);
                chunk.clone()
            };
            let call = Instant::now();
            plane.raw_collector_for("tp").lock().push_chunk(vofa_core::now_us(), buffer_raw::RawDataDirection::Rx, &data);
            let result = route_bytes(&plane, None, "tp", &data, rx.as_ref().map_or(0, tokio::sync::broadcast::Receiver::len), &mut cache, None).await;
            latencies.push(call.elapsed().as_secs_f64() * 1_000.0);
            bytes += data.len() as u64;
            frames += result.frames as u64;
            let diag = plane.eval_diagnostics();
            max_queue_frames = max_queue_frames.max(diag.queued_frames);
            max_queue_bytes = max_queue_bytes.max(diag.queued_estimated_bytes);
            if tick.elapsed() >= Duration::from_secs(5) {
                println!("{}", serde_json::json!({"elapsed_s":start.elapsed().as_secs_f64(), "rx_MB_s":(bytes-tick_bytes) as f64 / tick.elapsed().as_secs_f64() / 1e6, "queue_frames":diag.queued_frames, "eval_dropped":diag.dropped_frames, "queue_wait_max_ms":diag.queue_wait_max_ns as f64/1e6, "dispatch_wait_max_ms":diag.dispatch_wait_max_ns as f64/1e6, "eval_service_max_ms":diag.eval_service_max_ns as f64/1e6}));
                tick = Instant::now(); tick_bytes = bytes;
            }
        }
        let elapsed = start.elapsed().as_secs_f64();
        if let Some(cancel) = &cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        let drain = Instant::now();
        loop {
            let diag = plane.eval_diagnostics();
            if diag.completed_frames + diag.dropped_frames >= frames { break; }
            assert!(drain.elapsed() < Duration::from_secs(10), "求值未能排空");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        stop.store(true, Ordering::Relaxed);
        plane.eval.data_bus.unsubscribe(&key);
        preview_task.abort();
        let diag = plane.eval_diagnostics();
        println!("{}", serde_json::json!({"eval_completed_batches":diag.completed_batches, "injected_ingest_stalls":ingest_stalls, "injected_ingest_stall_ms":ingest_stall_ms}));
        assert!(ingest_stall_ms == 0 || ingest_stalls > 0, "摄入停顿场景必须实际注入至少一次");
        let buffer = plane.buffer_for("pt");
        let buffer = buffer.lock();
        println!("{}", serde_json::json!({"raw_capacity":buffer.max_points(), "raw_points":buffer.point_count(), "raw_overwritten":buffer.storage_overflow(), "buffer_estimated_bytes":buffer.estimated_bytes()}));
        println!("{}", serde_json::json!({"preview_batches":preview_batches.load(Ordering::Relaxed), "preview_lagged_batches":preview_lagged.load(Ordering::Relaxed), "data_bus":plane.eval.data_bus.health()}));
        println!("{}", serde_json::json!({"graph":if computed {"math4"} else {"raw4"}, "generator":generator, "elapsed_s":elapsed, "bytes":bytes, "frames":frames, "rx_MB_s":bytes as f64/elapsed/1e6, "lagged_messages":lagged, "eval_completed":diag.completed_frames, "eval_dropped":diag.dropped_frames, "max_queue_frames":max_queue_frames, "max_queue_estimated_bytes":max_queue_bytes, "queue_wait_max_ms":diag.queue_wait_max_ns as f64/1e6, "dispatch_wait_max_ms":diag.dispatch_wait_max_ns as f64/1e6, "eval_service_max_ms":diag.eval_service_max_ns as f64/1e6, "drain_ms":drain.elapsed().as_secs_f64()*1000.0, "ingest":summary(latencies)}));
        assert_eq!(bytes / 20, frames, "JustFloat 不得静默丢帧");
        (lagged, diag, bytes as f64/elapsed/1e6, preview_lagged.load(Ordering::Relaxed))
    });
    println!(
        "{}",
        serde_json::json!({"query_detail_and_overview":summary(queries.join().unwrap())})
    );
    if let Some(stalls) = stalls {
        stalls.thread().unpark();
        let count = stalls.join().unwrap();
        println!(
            "{}",
            serde_json::json!({"injected_graph_lock_stall_ms":stall_ms, "injected_stalls":count})
        );
        assert!(count > 0, "人为停顿场景必须实际注入至少一次");
    }
    assert_eq!(lagged, 0, "接收广播不得丢消息");
    assert_eq!(diag.dropped_frames, 0, "评估队列不得丢帧");
    assert_eq!(preview_lagged, 0, "样本订阅不得丢批");
    assert_eq!(
        plane.eval.data_bus.health().ingress_dropped,
        0,
        "DataBus 摄入不得丢批"
    );
    assert!(
        throughput > 10.0,
        "实收吞吐必须超过 10 MB/s，实际 {throughput}"
    );
}
