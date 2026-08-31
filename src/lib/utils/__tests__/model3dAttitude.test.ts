import { describe, expect, it } from 'vitest';
import { normalizeModel3DConfig } from '../createWidget';
import {
  model3dAttitudePortIds,
  resolveModel3DRotation,
} from '../model3dAttitude';

describe('Model3D attitude input modes', () => {
  it('converts degree Euler inputs to radians', () => {
    expect(resolveModel3DRotation('degrees', { roll: 180, pitch: 90, yaw: -45 })).toEqual([
      Math.PI,
      Math.PI / 2,
      -Math.PI / 4,
    ]);
  });

  it('keeps radian Euler inputs unchanged', () => {
    expect(resolveModel3DRotation('radians', { roll: 1, pitch: 2, yaw: 3 })).toEqual([1, 2, 3]);
  });

  it('normalizes q0=w quaternion inputs and converts them to XYZ Euler angles', () => {
    const scaledComponent = Math.sqrt(0.5) * 4;
    const rotation = resolveModel3DRotation('quaternion', {
      q0: scaledComponent,
      q1: 0,
      q2: 0,
      q3: scaledComponent,
    });
    expect(rotation[0]).toBeCloseTo(0);
    expect(rotation[1]).toBeCloseTo(0);
    expect(rotation[2]).toBeCloseTo(Math.PI / 2);
  });

  it('treats an empty quaternion as the identity rotation', () => {
    expect(resolveModel3DRotation('quaternion', {})).toEqual([0, 0, 0]);
  });

  it('switches attitude ports and preserves legacy radians semantics', () => {
    expect(model3dAttitudePortIds('degrees')).toEqual(['roll', 'pitch', 'yaw']);
    expect(model3dAttitudePortIds('quaternion')).toEqual(['q0', 'q1', 'q2', 'q3']);
    expect(normalizeModel3DConfig({ id: 'legacy' }).attitudeInputMode).toBe('radians');
  });
});
