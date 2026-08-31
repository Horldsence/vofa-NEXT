import type { Model3DAttitudeInputMode } from '../../types';

export const MODEL3D_EULER_PORTS = ['roll', 'pitch', 'yaw'] as const;
export const MODEL3D_QUATERNION_PORTS = ['q0', 'q1', 'q2', 'q3'] as const;

export function model3dAttitudePortIds(mode: Model3DAttitudeInputMode): readonly string[] {
  return mode === 'quaternion' ? MODEL3D_QUATERNION_PORTS : MODEL3D_EULER_PORTS;
}

export type Model3DRotation = [number, number, number];

/// 将配置的姿态输入统一换算为 Three.js 默认 XYZ 欧拉角（弧度）。
/// 四元数端口约定 q0=w, q1=x, q2=y, q3=z；输入会先归一化。
export function resolveModel3DRotation(
  mode: Model3DAttitudeInputMode,
  values: Record<string, number>
): Model3DRotation {
  if (mode === 'degrees') {
    const scale = Math.PI / 180;
    return [
      (values.roll ?? 0) * scale,
      (values.pitch ?? 0) * scale,
      (values.yaw ?? 0) * scale,
    ];
  }
  if (mode === 'radians') {
    return [values.roll ?? 0, values.pitch ?? 0, values.yaw ?? 0];
  }

  let w = values.q0 ?? 0;
  let x = values.q1 ?? 0;
  let y = values.q2 ?? 0;
  let z = values.q3 ?? 0;
  const norm = Math.hypot(w, x, y, z);
  if (!Number.isFinite(norm) || norm === 0) return [0, 0, 0];
  w /= norm;
  x /= norm;
  y /= norm;
  z /= norm;

  // 与 Three.js Euler order='XYZ' 一致。
  const sinPitch = Math.max(-1, Math.min(1, 2 * (w * y + z * x)));
  return [
    Math.atan2(2 * (w * x - y * z), 1 - 2 * (x * x + y * y)),
    Math.asin(sinPitch),
    Math.atan2(2 * (w * z - x * y), 1 - 2 * (y * y + z * z)),
  ];
}
