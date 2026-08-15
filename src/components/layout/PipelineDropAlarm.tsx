//! 管道数据丢弃告警状态栏指示器
//!
//! - 订阅 stats.rxDroppedWindow (transport:rx 100ms 窗口内 broadcast Lagged 丢弃数)
//! - > 0 时: 红色脉冲点 + 本窗口丢弃数, 点击打开丢弃说明弹层
//! - 0 -> >0 沿触发警告通知 (30s 节流, source='pipeline-drop' 同源自折叠)

import { useEffect, useRef, useState } from 'react';
import { notify } from '../../lib/tauri/notifications';
import { t } from '../../i18n';
import { useAppStore } from '../../store/appStore';
import { DroppedInfoPopover } from '../common/DroppedInfoPopover';

/// 告警通知节流间隔 (ms)
const ALARM_THROTTLE_MS = 30_000;

export function PipelineDropAlarm() {
  const lang = useAppStore((s) => s.lang);
  const rxDroppedWindow = useAppStore((s) => s.stats.rxDroppedWindow);
  const [infoOpen, setInfoOpen] = useState(false);
  /// 上次告警时间戳 (用于节流)
  const lastAlarmRef = useRef<number>(0);
  /// 上一窗口值 — 用于 0 -> >0 沿检测
  const prevRef = useRef<number>(0);

  useEffect(() => {
    if (rxDroppedWindow > 0 && prevRef.current === 0) {
      const now = Date.now();
      if (now - lastAlarmRef.current > ALARM_THROTTLE_MS) {
        lastAlarmRef.current = now;
        notify.warn(
          t(lang, 'notifDataDroppedTitle'),
          t(lang, 'notifDataDroppedBody').replace('{{count}}', String(rxDroppedWindow)),
          { source: 'pipeline-drop' }
        );
      }
    }
    prevRef.current = rxDroppedWindow;
  }, [rxDroppedWindow, lang]);

  if (rxDroppedWindow <= 0) return null;

  return (
    <>
      <div
        className="flex items-center gap-1.5 px-1.5 cursor-pointer hover:bg-bg-hover rounded"
        title={t(lang, 'statusDropped')}
        onClick={() => setInfoOpen(true)}
      >
        <span className="w-2 h-2 rounded-full bg-red animate-pulse inline-block" />
        <span className="text-red font-mono text-[10px]">
          {rxDroppedWindow}
        </span>
      </div>
      <DroppedInfoPopover
        open={infoOpen}
        onClose={() => setInfoOpen(false)}
        variant="pipeline"
      />
    </>
  );
}
