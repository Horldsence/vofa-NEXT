import type { ChannelMeasurementPayload, ScopeMeasurements } from '../../types';

/**
 * 把后端单通道测量载荷换算为耦合展示语义 — 纯算术换算, 无数据遍历
 * (统计已在后端权威缓冲上完成, 前端只做展示语义修正):
 * - DC: 直通 (vrms_ac 后端附送备用)
 * - AC: 去直流展示 — vavg=0, vmax/vmin 平移 −vavg, RMS 用 vrms_ac; 周期/占空比不变
 * - GND: 接地基准 — 全 0 / null
 */
export function toDisplayMeasurements(
  ch: ChannelMeasurementPayload,
  coupling: 'DC' | 'AC' | 'GND',
): ScopeMeasurements {
  if (coupling === 'GND') {
    return {
      vpp: 0,
      vmin: 0,
      vmax: 0,
      vavg: 0,
      vrms: 0,
      vrms_ac: 0,
      duty: null,
      freq: null,
      period: null,
    };
  }
  if (coupling === 'AC') {
    return {
      vpp: ch.vpp,
      vmin: ch.vmin - ch.vavg,
      vmax: ch.vmax - ch.vavg,
      vavg: 0,
      vrms: ch.vrms_ac,
      vrms_ac: ch.vrms_ac,
      duty: ch.duty,
      freq: ch.freq,
      period: ch.period,
    };
  }
  return {
    vpp: ch.vpp,
    vmin: ch.vmin,
    vmax: ch.vmax,
    vavg: ch.vavg,
    vrms: ch.vrms,
    vrms_ac: ch.vrms_ac,
    duty: ch.duty,
    freq: ch.freq,
    period: ch.period,
  };
}
