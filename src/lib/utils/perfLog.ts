/// 调试用性能日志 — 按 key 聚合, 每秒最多输出一次, 避免高码率下刷屏
///
/// 用于排查大数据量下的卡顿/卡死: 统计各订阅通道的消息速率、字节速率、
/// 重组队列深度, 以及视图切换的耗时。默认开启; 置 false 可关闭:
/// `setPerfLogEnabled(false)` 或 localStorage 'vofa-perf-log' = '0'

interface Metric {
  count: number;
  bytes: number;
  maxExtra: number;
  lastFlush: number;
}

const metrics = new Map<string, Metric>();

let perfLogEnabled = (() => {
  try {
    return localStorage.getItem('vofa-perf-log') !== '0';
  } catch {
    return true;
  }
})();

export function setPerfLogEnabled(v: boolean): void {
  perfLogEnabled = v;
}

/// 计数一次消息; bytes 为消息载荷字节数, extra 为队列深度等辅助指标 (取峰值)
/// 每秒聚合输出: 消息速率 / 字节速率 / extra 峰值
export function tickMetric(key: string, bytes = 0, extra = 0): void {
  if (!perfLogEnabled) return;
  const now = performance.now();
  let m = metrics.get(key);
  if (!m) {
    m = { count: 0, bytes: 0, maxExtra: 0, lastFlush: now };
    metrics.set(key, m);
  }
  m.count++;
  m.bytes += bytes;
  if (extra > m.maxExtra) m.maxExtra = extra;
  if (now - m.lastFlush >= 1000) {
    console.info(`[perf ${key}] ${m.count}/s, ${m.bytes} B/s, max-extra=${m.maxExtra}`);
    m.count = 0;
    m.bytes = 0;
    m.maxExtra = 0;
    m.lastFlush = now;
  }
}

/// 输出一次性事件 (订阅创建/取消、视图切换等), 附时间戳
export function perfEvent(msg: string): void {
  if (!perfLogEnabled) return;
  console.info(`[perf ${performance.now().toFixed(1)}] ${msg}`);
}
