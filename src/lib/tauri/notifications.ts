/// 全局通知管理器 — VSCode 风格 Toast + Tauri 系统通知
///
/// 设计:
/// - 应用内 Toast 始终显示 (所有 I/O 错误)
/// - 系统通知 (Tauri OS 通知) 仅在关键事件由后端 notify.rs 触发, 前端不再重复发送
/// - 错误持久保留直到用户关闭; warn/info 8 秒后自动消失
/// - 最多显示 5 条, 超出折叠为 "还有 N 条"
/// - 同一 source 的连续错误折叠为一条 (累计计数)

export type Severity = 'error' | 'warn' | 'info';

export interface NotificationAction {
  label: string;
  run: () => void;
}

export interface AppNotification {
  id: string;
  severity: Severity;
  title: string;
  message: string;
  actions: NotificationAction[];
  /** 来源标识, 用于同源折叠 (如 'connect', 'send') */
  source?: string;
  timestamp: number;
  /** 同源错误累计次数 (折叠时 >1) */
  count: number;
}

type Listener = (list: AppNotification[]) => void;

const AUTO_DISMISS_MS = 8000;

let nextId = 1;

class NotificationManager {
  private list: AppNotification[] = [];
  private listeners = new Set<Listener>();
  private timers = new Map<string, ReturnType<typeof setTimeout>>();

  /** 推送错误通知 (持久, 不自动消失) */
  error(
    title: string,
    message: string,
    opts?: { actions?: NotificationAction[]; source?: string }
  ): string {
    return this.push('error', title, message, opts);
  }

  /** 推送警告通知 (8 秒后自动消失) */
  warn(
    title: string,
    message: string,
    opts?: { actions?: NotificationAction[]; source?: string }
  ): string {
    return this.push('warn', title, message, opts);
  }

  /** 推送信息通知 (8 秒后自动消失) */
  info(
    title: string,
    message: string,
    opts?: { actions?: NotificationAction[]; source?: string }
  ): string {
    return this.push('info', title, message, opts);
  }

  /** 关闭指定通知 */
  dismiss(id: string): void {
    const timer = this.timers.get(id);
    if (timer) {
      clearTimeout(timer);
      this.timers.delete(id);
    }
    this.list = this.list.filter((n) => n.id !== id);
    this.emit();
  }

  /** 关闭全部 */
  dismissAll(): void {
    this.timers.forEach((t) => clearTimeout(t));
    this.timers.clear();
    this.list = [];
    this.emit();
  }

  /** 订阅通知列表变更 */
  subscribe(fn: Listener): () => void {
    this.listeners.add(fn);
    return () => {
      this.listeners.delete(fn);
    };
  }

  getList(): AppNotification[] {
    return this.list;
  }

  private push(
    severity: Severity,
    title: string,
    message: string,
    opts?: { actions?: NotificationAction[]; source?: string }
  ): string {
    const source = opts?.source;

    // 同源折叠: 错误与警告可叠加计数 (info 不折叠, 避免漏看)
    if (source && severity !== 'info') {
      const existing = this.list.find(
        (n) => n.source === source && n.severity === severity
      );
      if (existing) {
        existing.count += 1;
        existing.message = message;
        existing.timestamp = Date.now();
        // 重置自动消失定时器 (warn)
        this.scheduleAutoDismiss(existing.id, severity);
        this.emit();
        return existing.id;
      }
    }

    const id = `n${nextId++}`;
    const notif: AppNotification = {
      id,
      severity,
      title,
      message,
      actions: opts?.actions ?? [],
      source,
      timestamp: Date.now(),
      count: 1,
    };
    // 新通知插入到顶部
    this.list = [notif, ...this.list];
    this.scheduleAutoDismiss(id, severity);
    this.emit();
    return id;
  }

  private scheduleAutoDismiss(id: string, severity: Severity): void {
    // 错误持久, 不自动消失
    if (severity === 'error') return;
    // 清除已有定时器
    const existing = this.timers.get(id);
    if (existing) clearTimeout(existing);
    const timer = setTimeout(() => {
      this.dismiss(id);
    }, AUTO_DISMISS_MS);
    this.timers.set(id, timer);
  }

  private emit(): void {
    for (const fn of this.listeners) {
      fn(this.list);
    }
  }
}

export const notify = new NotificationManager();

/** 将 Error 对象格式化为简洁消息 */
export function formatError(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === 'string') return e;
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}
