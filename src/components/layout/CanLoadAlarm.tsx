import { useEffect, useRef, useState } from 'react';
import { api } from '../../lib/tauri';
import { notify } from '../../lib/notifications';
import { t } from '../../i18n';
import { useAppStore } from '../../store/appStore';
import { useCanLoadAlarmStore } from '../../store/canLoadAlarmStore';

/// CAN 负载告警状态栏指示器
///
/// - 订阅 CAN 负载推送 (1000ms 间隔, 轻量)
/// - 当 load_ratio 超过阈值时: 显示红色脉冲点 + 当前负载率
/// - 触发桌面通知 (节流: 同阈值告警 5 秒内只通知一次)
/// - 阈值由 CanLoadView 工具栏通过 canLoadAlarmStore 控制
export function CanLoadAlarm() {
  const lang = useAppStore((s) => s.lang);
  const threshold = useCanLoadAlarmStore((s) => s.threshold);
  const enabled = useCanLoadAlarmStore((s) => s.enabled);
  const [loadRatio, setLoadRatio] = useState<number>(0);
  /// 上次告警时间戳 (用于节流)
  const lastAlarmRef = useRef<number>(0);

  useEffect(() => {
    if (!enabled) {
      setLoadRatio(0);
      return;
    }
    const sub = api.subscribeCanLoad(
      (snap) => {
        setLoadRatio(snap.load_ratio);
        // 超阈值时触发通知 (5 秒节流)
        if (snap.load_ratio >= threshold) {
          const now = Date.now();
          if (now - lastAlarmRef.current > 5000) {
            lastAlarmRef.current = now;
            notify.warn(
              t(lang, 'canLoadThresholdAlarm'),
              `${(snap.load_ratio * 100).toFixed(2)}% >= ${(threshold * 100).toFixed(0)}%`,
              { source: 'can-load-alarm' }
            );
          }
        }
      },
      { intervalMs: 1000 }
    );
    return () => sub.cancel();
  }, [enabled, threshold, lang]);

  if (!enabled || loadRatio < threshold) return null;

  return (
    <div
      className="flex items-center gap-1.5 px-1.5"
      title={`${t(lang, 'canLoadThresholdAlarm')}: ${(loadRatio * 100).toFixed(2)}% >= ${(threshold * 100).toFixed(0)}%`}
    >
      <span className="w-2 h-2 rounded-full bg-red animate-pulse inline-block" />
      <span className="text-red font-mono text-[10px]">
        {(loadRatio * 100).toFixed(1)}%
      </span>
    </div>
  );
}
