/** 把框选时的相对秒范围固定到当时的后端绝对时间锚点。 */
export function absoluteTimeRangeUs(
  range: { startSec: number; endSec: number },
  latestTimestampUs: number,
): { startUs: number; endUs: number } {
  return {
    startUs: Math.round(latestTimestampUs + Math.min(range.startSec, range.endSec) * 1e6),
    endUs: Math.round(latestTimestampUs + Math.max(range.startSec, range.endSec) * 1e6),
  };
}
