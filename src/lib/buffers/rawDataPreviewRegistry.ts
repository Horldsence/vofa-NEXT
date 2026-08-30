import { RawDataBuffer } from './dataBuffer';

interface Stats {
  usage: number;
  length: number;
  capacity: number;
}

const DEFAULT_CAPACITY = 1_048_576;
let configuredCapacity = DEFAULT_CAPACITY;
const tracked = new Map<RawDataBuffer, () => void>();
const listeners = new Set<(usage: number, length: number, capacity: number) => void>();

function snapshot(): Stats {
  let length = 0;
  let capacity = 0;
  for (const buffer of tracked.keys()) {
    length += buffer.storedBytes;
    capacity += buffer.capacityBytes;
  }
  if (capacity === 0) capacity = configuredCapacity;
  return { usage: length / Math.max(1, capacity), length, capacity };
}

function notify() {
  const stats = snapshot();
  for (const listener of listeners) {
    listener(stats.usage, stats.length, stats.capacity);
  }
}

export function createRawDataPreviewBuffer(): RawDataBuffer {
  return new RawDataBuffer(configuredCapacity);
}

export function trackRawDataPreviewBuffer(buffer: RawDataBuffer): () => void {
  const existing = tracked.get(buffer);
  if (existing) return () => {};
  const unsubscribe = buffer.subscribeStats(() => notify());
  tracked.set(buffer, unsubscribe);
  notify();
  return () => {
    const current = tracked.get(buffer);
    if (!current) return;
    current();
    tracked.delete(buffer);
    notify();
  };
}

export function subscribeRawDataPreviewStats(
  listener: (usage: number, length: number, capacity: number) => void,
): () => void {
  listeners.add(listener);
  const stats = snapshot();
  listener(stats.usage, stats.length, stats.capacity);
  return () => listeners.delete(listener);
}

export function setRawDataPreviewCapacity(capacity: number) {
  configuredCapacity = Math.max(1, capacity);
  for (const buffer of tracked.keys()) buffer.setCapacity(configuredCapacity);
  notify();
}

export function clearAllRawDataPreviewBuffers() {
  for (const buffer of tracked.keys()) buffer.clear();
}
