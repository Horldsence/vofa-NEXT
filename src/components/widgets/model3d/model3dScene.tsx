// ============ 3D 模型场景组件 ============
//
// Model3DWidget 的 <Canvas> 内渲染件: 拖尾轨迹 / 姿态立方体 / 自定义 GLB 模型
// (TEMP-DISABLED) 及其回退边界。从 Model3DWidget.tsx 拆出, 保持单文件 <500 行;
// 组件间的约定见各函数注释 (primitive 不可换 key、useGLTF hook 限制等)。

import { Component, useEffect, useMemo, useState, type ReactNode } from 'react';
import { useGLTF } from '@react-three/drei';
// [TEMP-DISABLED] 自定义模型导入已禁用 — 恢复时无需改动本文件
import { readFile } from '@tauri-apps/plugin-fs';
import * as THREE from 'three';
import type { Model3DSource } from '../../../types';


/// 拖尾轨迹 — 维护历史点队列, 用 Line + Points 渲染
///
/// Line/geometry/material 只创建一次, 后续通过更新 position attribute 驱动;
/// 不能在 JSX 里 new THREE.Line() — R3F 的 <primitive> 未换 key 不会替换已挂载实例,
/// 会导致画面永远停留在首帧的空 geometry (轨迹"固定在原点"的 bug 根源)
export function Trajectory({
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
export function AttitudeBox({
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
    (edgesLine.material).color.set(color);
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

/// 自定义 GLB/GLTF 模型 — fs 插件读字节 → Blob URL → useGLTF
///
/// 不能用 convertFileSrc (asset://): WKWebView 禁止 http://localhost 页面
/// fetch 自定义协议, 报 "Cross origin requests are only supported for HTTP"
///
/// - 必须位于 <Canvas> 子树内 (hook 限制)
/// - 路径变更时外层用 source.path 作 key 强制重 mount
/// - 读文件失败在渲染期抛出, 由 CustomModelBoundary 捕获并回退到内置立方体
/// - 仅保证 .glb (自包含二进制); .gltf 的外部 .bin/贴图无法相对 blob URL 解析
function CustomModel({
  path,
  rotation,
}: {
  path: string;
  rotation: [number, number, number];
}) {
  const [url, setUrl] = useState<string | null>(null);
  const [error, setError] = useState<unknown>(null);

  useEffect(() => {
    let objectUrl: string | null = null;
    let cancelled = false;
    readFile(path)
      .then((bytes) => {
        if (cancelled) return;
        objectUrl = URL.createObjectURL(
          new Blob([bytes], { type: 'model/gltf-binary' })
        );
        setUrl(objectUrl);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(err);
      });
    return () => {
      cancelled = true;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [path]);

  // 读文件失败 → 渲染期抛出才能被 ErrorBoundary 捕获
  if (error) {
    throw error instanceof Error
      ? error
      : new Error(typeof error === 'string' ? error : 'Failed to load model');
  }
  if (!url) return null;
  return <GltfScene url={url} rotation={rotation} />;
}

/// useGLTF 不能条件调用, 拆一层子组件保证 url 就绪后才进入 hook
function GltfScene({
  url,
  rotation,
}: {
  url: string;
  rotation: [number, number, number];
}) {
  const { scene } = useGLTF(url);
  return <primitive object={scene} rotation={rotation} />;
}

/// 自定义模型加载失败的回退边界 — 静默回退到内置立方体并上报错误
///
/// R3F hook 错误会冒泡到 React 渲染, 此处用 class 边界 try/catch 三件套捕获;
/// 与 drei Suspense 配合, 也覆盖文件不存在 / 格式错误等场景
class CustomModelBoundary extends Component<
  { children: ReactNode; fallback: ReactNode; onError?: (err: unknown) => void },
  { failed: boolean }
> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch(error: unknown) {
     
    console.warn('[Model3D] custom model load failed:', error);
    this.props.onError?.(error);
  }

  render() {
    if (this.state.failed) return this.props.fallback;
    return this.props.children;
  }
}

/// 实际渲染的模型 — builtin-cube 与 custom 分支的统一切换点
///
/// must be inside <Canvas>: RenderedModel consumes useGLTF (R3F hook)
///
/// [TEMP-DISABLED] 自定义模型导入功能 — 此组件暂时未使用, 恢复时取消注释
 
// @ts-expect-error TS6133: unused function — re-enable when custom model import is restored
function _RenderedModel({
  source,
  rotation,
  color,
  axisLength,
}: {
  source: Model3DSource;
  rotation: [number, number, number];
  color: string;
  axisLength: number;
}) {
  if (source.kind === 'builtin-cube') {
    return <AttitudeBox rotation={rotation} color={color} axisLength={axisLength} />;
  }
  // custom: 加载失败时回退到内置立方体, 不阻塞 widget 渲染
  return (
    <CustomModelBoundary
      fallback={
        <AttitudeBox rotation={rotation} color={color} axisLength={axisLength} />
      }
    >
      {/* key=path: 路径变化时强制重 mount, useGLTF 缓存才会重载 */}
      <CustomModel key={source.path} path={source.path} rotation={rotation} />
    </CustomModelBoundary>
  );
}

