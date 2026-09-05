import { useEffect, useMemo, useRef } from 'react';
import { Canvas } from '@react-three/fiber';
import { Grid, OrbitControls } from '@react-three/drei';
// [TEMP-DISABLED] 自定义模型导入已禁用, open 暂时注释 — 恢复时取消下方注释
// import { open } from '@tauri-apps/plugin-dialog';
import type {
  Model3DAttitudeInputMode,
  Model3DMode,
  Model3DSource,
  WidgetConfig,
} from '../../../types';
import { useAppStore } from '../../../store/appStore';
import { useNumericInputs } from '../../../lib/hooks/useNumericPort';
import {
  model3dAttitudePortIds,
  resolveModel3DRotation,
} from '../../../lib/utils/model3dAttitude';
import { t } from '../../../i18n';
import { chipClass } from '../../ui/chip';
import { AttitudeBox, Trajectory } from './model3dScene';

interface Model3DWidgetProps {
  widget: Extract<WidgetConfig, { kind: 'Model3D' }>;
}

/// 模式由两个独立 toggle 组合而成:
///   仅 trajectory          -> mode = 'trajectory'
///   仅 attitude            -> mode = 'attitude'
///   trajectory + attitude  -> mode = 'trajectory-attitude'
/// 不允许两个 toggle 同时关闭 — 取消最后一个时该 toggle 保持开启
const MODE_TOGGLES: { key: 'trajectory' | 'attitude'; labelKey: string }[] = [
  { key: 'trajectory', labelKey: 'model3dTrajectory' },
  { key: 'attitude', labelKey: 'model3dAttitude' },
];
/// 3D 模型控件 — Three.js 三模式渲染
///
/// 数据流 (前端纯渲染, 后端仅作为 Sink 透传输入):
///   1. 后端 CompiledGraph 把 Model3D 节点当作 Sink (不在 eval_order)
///   2. 前端 useGraphInputs 读取位置与当前姿态格式对应的通道 (缺失补 0)
///   3. trajectory 模式:          xyz 累积历史点 → 渲染拖尾
///   4. attitude 模式:            角度/弧度/四元数统一换算为欧拉角 → 旋转模型 (原点)
///   5. trajectory-attitude 模式: 同时渲染拖尾 + 跟随 xyz/姿态的模型
///
/// 输入端口: x / y / z + roll/pitch/yaw 或 q0/q1/q2/q3 (缺失补 0)
export function Model3DWidget({ widget }: Model3DWidgetProps) {
  const {
    id,
    mode,
    trailLength,
    color,
    axisLength,
    modelSource,
    attitudeInputMode = 'radians',
  } = widget.params;
  // [TEMP-DISABLED] modelSource 解构保留 (用于将来恢复), 显式 void 抑制 noUnusedLocals
  void modelSource;
  const updateWidget = useAppStore((s) => s.updateWidget);
  const lang = useAppStore((s) => s.lang);

  const attitudePorts = model3dAttitudePortIds(attitudeInputMode);
  const inputStates = useNumericInputs(id, ['x', 'y', 'z', ...attitudePorts]);
  const inputs = Object.fromEntries(
    Object.entries(inputStates).map(([port, state]) => [port, state.latest?.value ?? 0]),
  );
  const x = inputs.x ?? 0;
  const y = inputs.y ?? 0;
  const z = inputs.z ?? 0;
  const rotation = resolveModel3DRotation(attitudeInputMode, inputs);

  // [TEMP-DISABLED] 自定义 3D 模型导入功能暂时关闭 — 始终渲染内置立方体
  //   恢复方式 (按编号顺序):
  //     1. 顶部 imports 解除 open / readFile 注释
  //     2. 把 effectiveSource 改回基于 modelSource 的解析:
  //          loadError && modelSource.kind === 'custom' ? { kind: 'builtin-cube' } : modelSource
  //     3. 解除三个 handler (handlePickModel / handleResetModel / handleLoadError) 的注释
  //     4. 解除侧栏 model UI 段的 JSX 注释
  //     5. 解除 Canvas 内 CustomModelBoundary / RenderedModel 块, 改回
  //          <CustomModelBoundary onError={handleLoadError} fallback={...}><RenderedModel ... /></CustomModelBoundary>
  //     6. 解除 loadError banner 的 JSX 注释
  //   保留物 (无须改动):
  //     - CustomModel / GltfScene / CustomModelBoundary / RenderedModel 组件定义
  //     - modelSource 配置字段 + normalizeModel3DConfig 归一化
  //     - fs:allow-read-file capability + model3dModel* i18n 文案
  // [TEMP-DISABLED] 恢复时删除本行 + 改回: loadError && modelSource.kind === 'custom' ? ...
  // @ts-expect-error — effectiveSource temporarily forced to builtin-cube; remove this line when re-enabling
  const _effectiveSource: Model3DSource = { kind: 'builtin-cube' };

  // 维护拖尾点队列 (Float32Array, 直接喂给 BufferGeometry) — 仅 trajectory / trajectory-attitude 模式累积
  const pointsRef = useRef<number[]>([]);
  useEffect(() => {
    if (mode !== 'trajectory' && mode !== 'trajectory-attitude') return;
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

  /// 解析 toggle 状态 → 实际 mode
  /// - 两个都关: 返回 null (不更新, 保持当前 mode)
  /// - 两个都开: 'trajectory-attitude'
  /// - 仅其中一个开: 单模式
  const resolveMode = (trajectoryOn: boolean, attitudeOn: boolean): Model3DMode | null => {
    if (trajectoryOn && attitudeOn) return 'trajectory-attitude';
    if (trajectoryOn) return 'trajectory';
    if (attitudeOn) return 'attitude';
    return null;
  };

  const isTrajectory = mode === 'trajectory' || mode === 'trajectory-attitude';
  const isAttitude = mode === 'attitude' || mode === 'trajectory-attitude';

  const handleToggleMode = (key: 'trajectory' | 'attitude') => {
    const newTraj = key === 'trajectory' ? !isTrajectory : isTrajectory;
    const newAtt = key === 'attitude' ? !isAttitude : isAttitude;
    const nextMode = resolveMode(newTraj, newAtt);
    // 两个都关是禁用状态 — 静默忽略, 保留当前 mode
    if (nextMode === null) return;
    if (nextMode === mode) return;
    updateWidget(id, {
      kind: 'Model3D',
      params: { ...widget.params, mode: nextMode },
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

  const handleAttitudeInputModeChange = (value: string) => {
    if (value !== 'degrees' && value !== 'radians' && value !== 'quaternion') return;
    updateWidget(id, {
      kind: 'Model3D',
      params: { ...widget.params, attitudeInputMode: value },
    });
  };

  // [TEMP-DISABLED] 以下三个 handler 暂时禁用, 恢复自定义模型导入时取消注释
  //
  //   // 通过 Tauri 原生对话框选择 .glb / .gltf 文件 — 路径持久化到 widget 配置
  //   const handlePickModel = async () => {
  //     try {
  //       const selected = await open({
  //         multiple: false,
  //         directory: false,
  //         filters: [{ name: '3D Model', extensions: ['glb', 'gltf'] }],
  //       });
  //       if (typeof selected !== 'string') return;
  //       const name = selected.split(/[/\\]/).pop() ?? 'model.glb';
  //       setLoadError(false); // 切换路径时清掉旧错误
  //       updateWidget(id, {
  //         kind: 'Model3D',
  //         params: {
  //           ...widget.params,
  //           modelSource: { kind: 'custom', path: selected, name },
  //         },
  //       });
  //     } catch (err) {
  //       // eslint-disable-next-line no-console
  //       console.warn('[Model3D] open dialog failed:', err);
  //     }
  //   };
  //
  //   // 清除自定义模型回到内置立方体
  //   const handleResetModel = () => {
  //     setLoadError(false);
  //     updateWidget(id, {
  //       kind: 'Model3D',
  //       params: { ...widget.params, modelSource: { kind: 'builtin-cube' } },
  //     });
  //   };
  //
  //   // 加载失败回调: 切换 effectiveSource 回 builtin-cube
  //   const handleLoadError = () => {
  //     setLoadError(true);
  //   };

  const modeLabel =
    mode === 'trajectory-attitude'
      ? t(lang, 'model3dTrajectoryAttitude')
      : mode === 'attitude'
        ? t(lang, 'model3dAttitude')
        : t(lang, 'model3dTrajectory');
  // [TEMP-DISABLED] 自定义模型导入已禁用 — modelLabel 不再需要, 恢复时取消下方 UI 注释
  //   const modelLabel =
  //     effectiveSource.kind === 'custom' ? effectiveSource.name : t(lang, 'model3dModelBuiltin');

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

          {/* trajectory / trajectory-attitude 模式: 渲染拖尾轨迹 */}
          {(mode === 'trajectory' || mode === 'trajectory-attitude') && (
            <Trajectory positions={positions} color={color} />
          )}

          {/* attitude / trajectory-attitude 模式: 渲染姿态模型 */}
          {(mode === 'attitude' || mode === 'trajectory-attitude') && (
            <group
              position={
                mode === 'trajectory-attitude'
                  ? [x, y, z]
                  : [0, 0, 0]
              }
              rotation={rotation}
            >
              {/* [TEMP-DISABLED] 自定义模型导入已禁用 — 直接渲染内置立方体, 恢复时改回:
                  <CustomModelBoundary onError={handleLoadError} fallback={<AttitudeBox .../>}>
                    <RenderedModel source={effectiveSource} ... />
                  </CustomModelBoundary> */}
              <AttitudeBox
                rotation={[0, 0, 0]}
                color={color}
                axisLength={axisLength}
              />
            </group>
          )}

          <OrbitControls makeDefault />
        </Canvas>
        {/* 模式标签覆盖在左上角 */}
        <div className="absolute top-2 left-2 px-1.5 py-0.5 bg-accent/15 border border-accent/40 rounded-sm text-accent text-[10px] font-semibold uppercase tracking-[0.3px] pointer-events-none">
          {modeLabel}
        </div>
        {/* [TEMP-DISABLED] 自定义模型导入已禁用, 加载失败 banner 暂时隐藏 — 恢复时取消下方注释
        {loadError && (
          <div className="absolute bottom-2 left-2 right-2 px-2 py-1 bg-red-900/60 border border-red-500/50 rounded-sm text-red-200 text-[10px] font-medium pointer-events-none">
            {t(lang, 'model3dModelLoadError')}
          </div>
        )} */}
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
        <div
          className={`grid ${attitudeInputMode === 'quaternion' ? 'grid-cols-4' : 'grid-cols-3'} gap-1`}
        >
          {attitudePorts.map((port) => (
            <div key={port} className="flex flex-col items-center bg-bg-input border border-border rounded-sm py-1">
              <span className="text-text-secondary text-[9px] font-semibold uppercase">{port}</span>
              <span className="text-text-bright text-[11px] font-mono">
                {(inputs[port] ?? 0).toFixed(attitudeInputMode === 'degrees' ? 3 : 4)}
              </span>
            </div>
          ))}
        </div>
        <div className="text-[10px] text-text-secondary uppercase tracking-wide font-semibold px-1 pt-1">{t(lang, 'model3dSettings')}</div>
        <div className="flex flex-col gap-1.5 px-1">
          <div className="grid grid-cols-[80px_1fr] gap-1.5 items-center">
            <label className="text-[10px] text-text-secondary">{t(lang, 'model3dMode')}</label>
            <div className="flex gap-0.5">
              {MODE_TOGGLES.map((opt) => {
                const active = opt.key === 'trajectory' ? isTrajectory : isAttitude;
                return (
                  <button
                    key={opt.key}
                    className={`flex-1 ${chipClass(active)}`}
                    aria-pressed={active}
                    onClick={() => handleToggleMode(opt.key)}
                  >
                    {t(lang, opt.labelKey)}
                  </button>
                );
              })}
            </div>
          </div>
          {(mode === 'attitude' || mode === 'trajectory-attitude') && (
            <div className="grid grid-cols-[80px_1fr] gap-1.5 items-center">
              <label className="text-[10px] text-text-secondary">{t(lang, 'model3dAttitudeInput')}</label>
              <select
                value={attitudeInputMode}
                onChange={(e) => handleAttitudeInputModeChange(e.target.value)}
                className="w-full px-1 py-0.5 bg-bg-input border border-border rounded-sm text-text-primary text-xs focus:outline-none focus:border-accent"
              >
                {([
                  ['degrees', 'model3dAttitudeDegrees'],
                  ['radians', 'model3dAttitudeRadians'],
                  ['quaternion', 'model3dAttitudeQuaternion'],
                ] as const satisfies readonly [Model3DAttitudeInputMode, string][]).map(([value, labelKey]) => (
                  <option key={value} value={value}>
                    {t(lang, labelKey)}
                  </option>
                ))}
              </select>
            </div>
          )}
          {(mode === 'trajectory' || mode === 'trajectory-attitude') && (
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
          {/* [TEMP-DISABLED] 自定义模型导入 UI 段暂时隐藏 — 恢复时取消下方注释
          <div className="grid grid-cols-[80px_1fr] gap-1.5 items-center">
            <label className="text-[10px] text-text-secondary">{t(lang, 'model3dModel')}</label>
            <div className="flex gap-1 items-center">
              <span
                className="flex-1 truncate px-1 py-0.5 bg-bg-input border border-border rounded-sm text-text-primary text-xs"
                title={modelLabel}
              >
                {modelLabel}
              </span>
              {effectiveSource.kind === 'custom' && (
                <button
                  onClick={handleResetModel}
                  title={t(lang, 'model3dResetModel')}
                  className="px-1.5 py-0.5 bg-bg-input border border-border rounded-sm text-text-secondary hover:text-text-primary hover:bg-bg-hover text-xs"
                >
                  ↺
                </button>
              )}
            </div>
            <button
              onClick={handlePickModel}
              className="col-span-2 mt-1 px-1.5 py-1 bg-bg-input border border-border rounded-sm text-text-primary hover:bg-bg-hover text-xs"
            >
              {t(lang, 'model3dPickModel')}
            </button>
          </div>
          */}
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
