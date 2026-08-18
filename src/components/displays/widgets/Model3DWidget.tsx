import { useEffect, useMemo, useRef } from 'react';
import { Canvas } from '@react-three/fiber';
import { OrbitControls, Grid } from '@react-three/drei';
import * as THREE from 'three';
import { Settings2 } from 'lucide-react';
import type { WidgetConfig, Model3DMode } from '../../../types';
import { useAppStore } from '../../../store/appStore';
import { useGraphInputs } from '../../../lib/hooks/useGraphInput';
import { t } from '../../../i18n';
import { chipClass } from '../../ui/chip';

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
/// Line/geometry/material 只创建一次, 后续通过更新 position attribute 驱动;
/// 不能在 JSX 里 new THREE.Line() — R3F 的 <primitive> 未换 key 不会替换已挂载实例,
/// 会导致画面永远停留在首帧的空 geometry (轨迹"固定在原点"的 bug 根源)
function Trajectory({
  positions,
  color,
}: {
  positions: Float32Array;
  color: string;
}) {
  // 一次性创建底层对象 (frustumCulled=false: 拖尾增长期包围球滞后, 防止误剔除)
  const { line, geometry, material } = useMemo(() => {
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    const material = new THREE.LineBasicMaterial({ color });
    const line = new THREE.Line(geometry, material);
    line.frustumCulled = false;
    return { line, geometry, material };
    // 仅创建一次, 后续变化由下方 effect 原地更新
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 颜色变化 → 原地更新材质
  useEffect(() => {
    material.color.set(color);
  }, [material, color]);

  // 轨迹点变化 → 更新 position attribute (长度不变时复用缓冲, 避免每帧分配)
  useEffect(() => {
    const attr = geometry.attributes.position as THREE.BufferAttribute;
    if (attr.count === positions.length / 3) {
      attr.array.set(positions);
      attr.needsUpdate = true;
    } else {
      geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    }
  }, [geometry, positions]);

  // 卸载时释放 GPU 资源
  useEffect(
    () => () => {
      geometry.dispose();
      material.dispose();
    },
    [geometry, material]
  );

  // 端点小球跟随最新轨迹点
  const n = positions.length;
  const end: [number, number, number] =
    n >= 3 ? [positions[n - 3], positions[n - 2], positions[n - 1]] : [0, 0, 0];

  return (
    <>
      {/* 折线 */}
      <primitive object={line} />
      {/* 端点小球 */}
      <mesh position={end}>
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
    <div className="group widget-card-acrylic flex-1 min-w-0 min-h-0 flex relative overflow-hidden">
      {/* 主区: 3D Canvas 铺满 */}
      <div className="flex-1 min-w-0 min-h-0 bg-[#0a0a0a] relative">
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
        {/* 模式标签覆盖在左上角 */}
        <div className="absolute top-2 left-2 px-1.5 py-0.5 bg-accent/15 border border-accent/40 rounded-sm text-accent text-[10px] font-semibold uppercase tracking-[0.3px] pointer-events-none">
          {modeLabel}
        </div>
        {onEdit && (
          <button
            className="absolute top-2 right-2 opacity-0 transition-opacity duration-150 group-hover:opacity-100 w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary bg-bg-scrim"
            onClick={onEdit}
            title={t(lang, 'settings')}
          >
            <Settings2 size={11} />
          </button>
        )}
      </div>
      {/* 侧栏: 数值 + 设置 (固定宽, 纵向滚动, 直接展开) */}
      <div className="w-[240px] flex-shrink-0 border-l border-border bg-bg-sidebar overflow-y-auto flex flex-col gap-2 p-2.5">
        {/* xyz 实时数值 */}
        <div className="grid grid-cols-3 gap-1">
          {(['x', 'y', 'z'] as const).map((k, i) => (
            <div key={k} className="flex flex-col items-center bg-bg-input border border-border rounded-sm py-1">
              <span className="text-text-secondary text-[9px] font-semibold uppercase">{k}</span>
              <span className="text-text-bright text-[11px] font-mono">{[x, y, z][i].toFixed(3)}</span>
            </div>
          ))}
        </div>
        <div className="text-[10px] text-text-secondary uppercase tracking-wide font-semibold px-1 pt-1">{t(lang, 'model3dSettings')}</div>
        <div className="flex flex-col gap-1.5 px-1">
          <div className="grid grid-cols-[80px_1fr] gap-1.5 items-center">
            <label className="text-[10px] text-text-secondary">{t(lang, 'model3dMode')}</label>
            <div className="flex gap-0.5">
              {MODE_OPTIONS.map((opt) => (
                <button
                  key={opt.value}
                  className={`flex-1 ${chipClass(mode === opt.value)}`}
                  onClick={() => handleModeChange(opt.value)}
                >
                  {t(lang, opt.labelKey)}
                </button>
              ))}
            </div>
          </div>
          {mode === 'trajectory' && (
            <div className="grid grid-cols-[80px_1fr] gap-1.5 items-center">
              <label className="text-[10px] text-text-secondary">{t(lang, 'model3dTrailLength')}</label>
              <input
                type="number"
                value={trailLength}
                onChange={(e) => handleNumberChange('trailLength', e.target.value)}
                min={1}
                step={10}
                className="w-full px-1 py-0.5 bg-bg-input border border-border rounded-sm text-text-primary text-xs font-mono focus:outline-none focus:border-accent"
              />
            </div>
          )}
          <div className="grid grid-cols-[80px_1fr] gap-1.5 items-center">
            <label className="text-[10px] text-text-secondary">{t(lang, 'model3dColor')}</label>
            <input
              type="color"
              value={color}
              onChange={(e) => handleColorChange(e.target.value)}
              className="w-full h-[22px] p-0 bg-transparent border border-border rounded-sm cursor-pointer"
            />
          </div>
          <div className="grid grid-cols-[80px_1fr] gap-1.5 items-center">
            <label className="text-[10px] text-text-secondary">{t(lang, 'model3dAxisLength')}</label>
            <input
              type="number"
              value={axisLength}
              onChange={(e) => handleNumberChange('axisLength', e.target.value)}
              min={0.1}
              step={0.1}
              className="w-full px-1 py-0.5 bg-bg-input border border-border rounded-sm text-text-primary text-xs font-mono focus:outline-none focus:border-accent"
            />
          </div>
        </div>
      </div>
    </div>
  );
}
