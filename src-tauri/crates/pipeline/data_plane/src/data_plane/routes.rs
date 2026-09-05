//! DataPlaneState 拓扑与生命周期 — 协议状态同步 / 去重组重建 / 读任务挂载

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use kind::NodeKind;
use parking_lot::Mutex;
use schema_types::ProtocolConfig;
use tauri::AppHandle;

use super::read_task;
use super::{
    DataPlaneState, ProtocolNodeState, RouteGroups, BUFFER_WINDOW_TARGET_SECONDS,
    DEFAULT_BUFFER_CHANNELS, MAX_RAW_POINTS,
};

impl DataPlaneState {
    /// 同步 protocol_states 与全局节点表中的 Protocol 节点 (图重编译后调用):
    /// 新增/配置变更 → 重建引擎; 节点删除 → 移除状态并清理 source_frames/source_texts 对应项
    pub fn sync_protocol_states(&self) {
        let nodes = self.global_nodes.lock();
        let mut states = self.protocol_states.lock();
        // 移除已不存在的 Protocol 节点
        states.retain(|id, _| {
            matches!(
                nodes.get(id).map(|n| &n.kind),
                Some(NodeKind::Protocol { .. })
            )
        });
        // 新增 / 配置变更重建
        let mut rebuilt: Vec<(String, ProtocolConfig)> = Vec::new();
        for n in nodes.values() {
            if let NodeKind::Protocol {
                config,
                convert_to,
                schema,
            } = &n.kind
            {
                match states.get(&n.id) {
                    Some(st) => {
                        let mut st = st.lock();
                        if !st.matches(config, convert_to.as_ref(), schema.as_ref()) {
                            *st = ProtocolNodeState::new(
                                config,
                                convert_to.as_ref(),
                                schema.as_ref(),
                            );
                            rebuilt.push((n.id.clone(), config.clone()));
                        }
                    }
                    None => {
                        states.insert(
                            n.id.clone(),
                            Arc::new(Mutex::new(ProtocolNodeState::new(
                                config,
                                convert_to.as_ref(),
                                schema.as_ref(),
                            ))),
                        );
                        rebuilt.push((n.id.clone(), config.clone()));
                    }
                }
            }
        }
        drop(states);
        drop(nodes);
        // 引擎 (重) 建后对齐该源 buffer 通道数: 手动 = 配置值;
        // 自动 = 检测值随引擎重置失效, 回默认通道数待重新检测 (set_channels 会清空已有数据)
        for (id, cfg) in rebuilt {
            let effective = cfg.manual_channels().unwrap_or(DEFAULT_BUFFER_CHANNELS);
            self.buffer_for(&id).lock().set_channels(effective);
        }
        // source_frames / source_texts / 评估队列清理由 protocol_states 存活集决定
        let live: Vec<String> = self.protocol_states.lock().keys().cloned().collect();
        self.source_frames
            .lock()
            .retain(|id, _| live.iter().any(|k| k == id));
        self.source_texts
            .lock()
            .retain(|id, _| live.iter().any(|k| k == id));
        self.frame_queues
            .lock()
            .retain(|id, _| live.iter().any(|k| k == id));
        // 路由去重组与缓冲别名 (不变量 4): 同 (字节源, 协议配置等价) 只解析一次
        self.rebuild_route_groups();
    }

    /// 依据 BytePlan + 协议节点配置等价性重建去重组与缓冲别名 (冷路径,
    /// 图重编译后调用)。等价 key = (config, convert_to, schema) 的 serde 值。
    fn rebuild_route_groups(&self) {
        let consumers: Vec<(String, Vec<String>)> = {
            let plan = self.byte_plan.lock();
            plan.consumers
                .iter()
                .map(|(source, routes)| {
                    (
                        source.clone(),
                        routes.iter().map(|r| r.target.clone()).collect(),
                    )
                })
                .collect()
        };
        let nodes = self.global_nodes.lock();
        let states = self.protocol_states.lock();
        let mut groups: RouteGroups = HashMap::new();
        let mut aliases: HashMap<String, String> = HashMap::new();
        for (source, targets) in consumers {
            let mut protos: Vec<String> = targets
                .into_iter()
                .filter(|t| {
                    matches!(
                        nodes.get(t).map(|n| &n.kind),
                        Some(NodeKind::Protocol { .. })
                    )
                })
                .collect();
            protos.sort();
            let mut local: Vec<(String, String, Vec<String>)> = Vec::new();
            for target in protos {
                let key = states.get(&target).and_then(|st| {
                    let s = st.lock();
                    serde_json::to_string(&(&s.config, &s.convert_config, &s.schema)).ok()
                });
                let Some(key) = key else { continue };
                match local.iter_mut().find(|(k, ..)| *k == key) {
                    Some((_, _, members)) => members.push(target.clone()),
                    None => local.push((key, target.clone(), vec![target.clone()])),
                }
            }
            for (_, repr, members) in &local {
                for member in members {
                    aliases.insert(member.clone(), repr.clone());
                }
            }
            groups.insert(source, local);
        }
        *self.route_groups.lock() = groups;
        *self.buffer_aliases.lock() = aliases;
    }

    /// 挂载 Transport 节点读任务 (open 成功后调用; 同 id 重复调用先 detach)
    pub async fn attach(&self, app: AppHandle, node_id: &str) {
        self.ensure_eval_worker();
        self.detach(node_id);
        let rx = self.transport.lock().await.subscribe(node_id);
        let Some(rx) = rx else {
            log::warn!("读任务挂载失败: 传输节点未打开: {node_id}");
            return;
        };
        // 确保按源 raw 收集器存在 (rx 方向)
        self.raw_collector_for(node_id);
        let plane = self.clone();
        let id = node_id.to_string();
        let handle = tokio::spawn(read_task::read_task(app, plane, id.clone(), rx));
        self.read_tasks.lock().insert(id, handle);
    }

    /// 卸载 Transport 节点读任务 (close 时调用)
    pub fn detach(&self, node_id: &str) {
        let handle = self.read_tasks.lock().remove(node_id);
        if let Some(h) = handle {
            h.abort();
        }
    }

    /// 在主动中止读任务前同步发布下游断开状态；abort 不会执行 read_task 的退出清理。
    pub fn mark_source_disconnected(&self, node_id: &str) {
        read_task::mark_downstream_disconnected(self, node_id);
    }

    /// 把 TestData 广播层的丢消息换算为所有下游协议源的逻辑采样缺口。
    /// 丢包发生在解析前，若不推进时钟，后续真实样本会被压到前一段末尾。
    pub(super) fn note_test_data_lagged(
        &self,
        transport_id: &str,
        lost_messages: u64,
        sample_rate: f32,
    ) {
        let lost_frames = lost_messages
            .saturating_mul(transport_core::test_data::samples_per_message(sample_rate));
        if lost_frames == 0 {
            return;
        }
        let targets = {
            let plan = self.byte_plan.lock();
            let nodes = self.global_nodes.lock();
            let mut pending = VecDeque::from([transport_id.to_string()]);
            let mut visited = std::collections::HashSet::new();
            let mut targets = std::collections::HashSet::new();
            while let Some(source) = pending.pop_front() {
                if !visited.insert(source.clone()) {
                    continue;
                }
                for route in plan.routes_for(&source) {
                    if matches!(
                        nodes.get(&route.target).map(|node| &node.kind),
                        Some(NodeKind::Protocol { .. })
                    ) {
                        targets.insert(route.target.clone());
                    }
                    pending.push_back(route.target.clone());
                }
            }
            targets
        };
        let states = self.protocol_states.lock();
        for target in targets {
            if let Some(state) = states.get(&target) {
                state
                    .lock()
                    .note_exact_frame_gap(lost_frames, f64::from(sample_rate));
            }
        }
    }

    /// 容量自洽 (不变量 2): 按来源名义帧率整备缓冲容量
    ///
    /// L0 目标容量 = 帧率 × 目标窗口秒数, 受内存预算半额折算的点数封顶
    /// (另一半留给派生层/金字塔/停止快照)。±5% 内的速率波动不重复整备。
    /// 超出封顶的窗口由金字塔层提供包络 (示波器语义)。
    pub(crate) fn tune_buffer_capacity(&self, source_id: &str, frames_per_sec: f64) {
        if !frames_per_sec.is_finite() || frames_per_sec <= 0.0 {
            return;
        }
        {
            let tuned = self.tuned_rate.lock();
            if tuned
                .get(source_id)
                .is_some_and(|r| (r - frames_per_sec).abs() / *r < 0.05)
            {
                return;
            }
        }
        let budget_mb =
            f64::from(u32::try_from(self.pipeline_config.read().memory_budget_mb).unwrap_or(256));
        let channels = f64::from(
            u32::try_from(self.buffer_for(source_id).lock().channel_count()).unwrap_or(4),
        )
        .max(1.0);
        // 每样本 ≈ 8B 时间戳 + 4B×通道; 半预算给原始层
        let bytes_per_point = 4.0f64.mul_add(channels, 8.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let cap_points = (budget_mb * 0.5 * 1_048_576.0 / bytes_per_point) as usize;
        let cap = cap_points.min(MAX_RAW_POINTS);
        let buffer = self.buffer_for(source_id);
        let mut b = buffer.lock();
        if b.ensure_capacity_for_rate(frames_per_sec, BUFFER_WINDOW_TARGET_SECONDS, cap) {
            log::info!(
                "波形缓冲容量整备: 源 {source_id} 帧率 {frames_per_sec:.0}/s → {} 点 \
                 (目标窗口 {BUFFER_WINDOW_TARGET_SECONDS}s, 封顶 {cap})",
                b.max_points()
            );
        }
        self.tuned_rate
            .lock()
            .insert(source_id.to_string(), frames_per_sec);
    }
}
