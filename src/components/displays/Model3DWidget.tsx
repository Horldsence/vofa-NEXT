import { useState, useEffect, useMemo, useRef } from 'react';
import { Canvas } from '@react-three/fiber';
import { OrbitControls, Grid } from '@react-three/drei';
import * as THREE from 'three';
import { Settings2, ChevronDown, ChevronUp } from 'lucide-react';
import type { WidgetConfig, Model3DMode } from '../../types';
import { useAppStore } from '../../store/appStore';
import { useGraphInputs } from '../../lib/useGraphInput';
import { t } from '../../i18n';

interface Model3DWidgetProps {
  widget: Extract<WidgetConfig, { kind: 'Model3D' }>;
  onRemove: () => void;
  onEdit?: () => void;
}

const MODE_OPTIONS: { value: Model3DMode; labelKey: string }[] = [
  { value: 'trajectory', labelKey: 'model3dTrajectory' },
  { value: 'attitude', labelKey: 'model3dAttitude' },
];

/// 拖尾轨迹 — 维护历史点队列, 用 Line + Points 渲染
///
/// 使用 useEffect 累积点而非直接渲染 [x,y,z], 这样 Three.js 只更新 geometry attribute,
/// 不会因 React 重建对象导致 GC 压力
function Trajectory({
  positions,
  color,
}: {
  positions: Float32Array;
  color: string;
}) {
  const lineRef = useRef<THREE.Line>(null);

  // 创建一份可复用的 geometry, 每次 positions 变化时更新 attribute
  const geometry = useMemo(() => {
    const geo = new THREE.BufferGeometry();
    const attr = new THREE.BufferAttribute(positions, 3);
    geo.setAttribute('position', attr);
    return geo;
  }, [positions]);

  useEffect(() => {
    geometry.attributes.position.needsUpdate = true;
  }, [geometry, positions]);

  return (
    <>
      {/* 折线 */}
      <primitive
        object={
          new THREE.Line(
            geometry,
            new THREE.LineBasicMaterial({ color, linewidth: 2 })
          )
        }
        ref={lineRef}
      />
      {/* 端点小球 */}
      <mesh>
        <sphereGeometry args={[0.05, 12, 12]} />
        <meshStandardMaterial color={color} emissive={color} emissiveIntensity={0.5} />
      </mesh>
    </>
  );
}

/// 姿态立方体 — xyz 作为欧拉角 (roll/pitch/yaw, 弧度)
function AttitudeBox({
  rotation,
  color,
  axisLength,
}: {
  rotation: [number, number, number];
  color: string;
  axisLength: number;
}) {
  // 预创建 lineSegments 对象 (EdgesGeometry 类型与 R3F 期望的 BufferGeometry 不兼容)
  const edgesLine = useMemo(() => {
    const geo = new THREE.EdgesGeometry(new THREE.BoxGeometry(1, 1, 1));
    return new THREE.LineSegments(
      geo,
      new THREE.LineBasicMaterial({ color })
    );
  }, [color]);
  // 颜色变化时更新材质
  useEffect(() => {
    (edgesLine.material as THREE.LineBasicMaterial).color.set(color);
  }, [edgesLine, color]);

  return (
    <group rotation={rotation}>
      {/* 半透明立方体 */}
      <mesh>
        <boxGeometry args={[1, 1, 1]} />
        <meshStandardMaterial color={color} transparent opacity={0.35} />
      </mesh>
      {/* 边框线 (通过 primitive 避免类型冲突) */}
      <primitive object={edgesLine} />
      {/* 跟随旋转的坐标轴 */}
      <axesHelper args={[axisLength]} />
    </group>
  );
}

/// 3D 模型控件 — Three.js 双模式渲染
///
/// 数据流 (前端纯渲染, 后端仅作为 Sink 透传输入):
///   1. 后端 CompiledGraph 把 Model3D 节点当作 Sink (不在 eval_order)
///   2. 前端 useGraphInputs 读取 x/y/z 三通道值 (60 FPS)
///   3. trajectory 模式: 累积历史点队列 → 渲染拖尾
///   4. attitude 模式: xyz 作为欧拉角 → 渲染旋转立方体
///
/// 输入端口: x / y / z (缺失补 0)
export function Model3DWidget({ widget, onEdit }: Model3DWidgetProps) {
  const { id, mode, trailLength, color, axisLength } = widget.params;
  const updateWidget = useAppStore((s) => s.updateWidget);
  const lang = useAppStore((s) => s.lang);
  const [showSettings, setShowSettings] = useState(false);

  // 读取 x/y/z 三通道 (缺失补 0)
  const inputs = useGraphInputs(id, ['x', 'y', 'z'], 0);
  const x = inputs.x ?? 0;
  const y = inputs.y ?? 0;
  const z = inputs.z ?? 0;

  // 维护拖尾点队列 (Float32Array, 直接喂给 BufferGeometry)
  const pointsRef = useRef<number[]>([]);
  useEffect(() => {
    if (mode !== 'trajectory') return;
    pointsRef.current.push(x, y, z);
    const maxLen = trailLength * 3;
    if (pointsRef.current.length > maxLen) {
      pointsRef.current = pointsRef.current.slice(-maxLen);
    }
  }, [x, y, z, mode, trailLength]);

  // 切换模式或 trailLength 改变时清空拖尾
  useEffect(() => {
    pointsRef.current = [];
  }, [mode, trailLength]);

  // 当前拖尾数据 → Float32Array (避免长度抖动时残留旧数据)
  const positions = useMemo(() => {
    const arr = new Float32Array(pointsRef.current.length);
    for (let i = 0; i < pointsRef.current.length; i++) arr[i] = pointsRef.current[i];
    return arr;
    // 依赖 [x, y, z, mode, trailLength] 触发重建
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [x, y, z, mode, trailLength]);

  const handleModeChange = (newMode: Model3DMode) => {
    updateWidget(id, {
      kind: 'Model3D',
      params: { ...widget.params, mode: newMode },
    });
  };

  const handleNumberChange = (field: 'trailLength' | 'axisLength', value: string) => {
    const num = parseFloat(value);
    if (!Number.isFinite(num) || num <= 0) return;
    updateWidget(id, {
      kind: 'Model3D',
      params: { ...widget.params, [field]: num },
    });
  };

  const handleColorChange = (value: string) => {
    updateWidget(id, {
      kind: 'Model3D',
      params: { ...widget.params, color: value },
    });
  };

  const modeLabel = t(lang, MODE_OPTIONS.find((o) => o.value === mode)?.labelKey ?? 'model3dTrajectory');

  return (
    <div className="widget-card model3d-widget">
      {onEdit && (
        <button
          className="btn-icon widget-edit"
          onClick={onEdit}
          title={t(lang, 'settings')}
          style={{ right: 24 }}
        >
          <Settings2 size={11} />
        </button>
      )}
      <div className="model3d-widget-mode-badge">{modeLabel}</div>
      <div className="model3d-widget-body">
        <div className="model3d-widget-canvas">
          <Canvas
            camera={{ position: [3, 3, 3], fov: 50 }}
            gl={{ antialias: true }}
            dpr={[1, 2]}
          >
            <ambientLight intensity={0.5} />
            <directionalLight position={[5, 5, 5]} intensity={0.8} />
            <Grid
              infiniteGrid
              cellSize={0.5}
              sectionSize={1}
              cellColor="#3c3c3c"
              sectionColor="#555555"
              fadeDistance={20}
            />
            <axesHelper args={[axisLength]} />
            {mode === 'trajectory' ? (
              <Trajectory positions={positions} color={color} />
            ) : (
              <AttitudeBox
                rotation={[x, y, z]}
                color={color}
                axisLength={axisLength}
              />
            )}
            <OrbitControls makeDefault />
          </Canvas>
        </div>
        <div className="model3d-widget-input-row">
          <span className="model3d-widget-input-label">x</span>
          <span className="model3d-widget-input-value">{x.toFixed(3)}</span>
          <span className="model3d-widget-input-label">y</span>
          <span className="model3d-widget-input-value">{y.toFixed(3)}</span>
          <span className="model3d-widget-input-label">z</span>
          <span className="model3d-widget-input-value">{z.toFixed(3)}</span>
        </div>
        <button
          className="model3d-widget-toggle"
          onClick={() => setShowSettings((v) => !v)}
          title={t(lang, 'settings')}
        >
          {showSettings ? <ChevronUp size={10} /> : <ChevronDown size={10} />}
          <span>{t(lang, 'model3dSettings')}</span>
        </button>
        {showSettings && (
          <div className="model3d-widget-settings">
            <div className="model3d-widget-setting-row">
              <label>{t(lang, 'model3dMode')}</label>
              <div className="model3d-widget-btn-group">
                {MODE_OPTIONS.map((opt) => (
                  <button
                    key={opt.value}
                    className={`model3d-widget-btn ${mode === opt.value ? 'active' : ''}`}
                    onClick={() => handleModeChange(opt.value)}
                  >
                    {t(lang, opt.labelKey)}
                  </button>
                ))}
              </div>
            </div>
            {mode === 'trajectory' && (
              <div className="model3d-widget-setting-row">
                <label>{t(lang, 'model3dTrailLength')}</label>
                <input
                  type="number"
                  value={trailLength}
                  onChange={(e) => handleNumberChange('trailLength', e.target.value)}
                  min={1}
                  step={10}
                />
              </div>
            )}
            <div className="model3d-widget-setting-row">
              <label>{t(lang, 'model3dColor')}</label>
              <input
                type="color"
                value={color}
                onChange={(e) => handleColorChange(e.target.value)}
              />
            </div>
            <div className="model3d-widget-setting-row">
              <label>{t(lang, 'model3dAxisLength')}</label>
              <input
                type="number"
                value={axisLength}
                onChange={(e) => handleNumberChange('axisLength', e.target.value)}
                min={0.1}
                step={0.1}
              />
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
