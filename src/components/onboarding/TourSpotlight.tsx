//! 聚光灯遮罩 — SVG mask 圆角镂空 + 高亮描边环
//!
//! - 全屏遮罩填 --color-bg-overlay, mask 内圆角矩形镂空目标区, 明暗主题自动跟随
//! - 几何量走 inline style 的 CSS 属性 (x/y/width/height), 步骤切换平滑滑动;
//!   不支持该特性的 WebView 下退化为瞬间跳位, 不影响正确性
//! - 整层 pointer-events: none — 高亮区完全可交互 (侧栏拖控件到画布建卡可行)

export interface TourRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

const CUT_RADIUS = 10;
const SLIDE_MS = 200;
const GEOMETRY_TRANSITION = `x ${SLIDE_MS}ms ease, y ${SLIDE_MS}ms ease, width ${SLIDE_MS}ms ease, height ${SLIDE_MS}ms ease`;
const BOX_TRANSITION = `left ${SLIDE_MS}ms ease, top ${SLIDE_MS}ms ease, width ${SLIDE_MS}ms ease, height ${SLIDE_MS}ms ease`;

export function TourSpotlight({ rect }: { rect: TourRect | null }) {
  return (
    <>
      <svg className="fixed inset-0 h-full w-full" style={{ pointerEvents: 'none' }} aria-hidden>
        <defs>
          <mask id="vofa-tour-spotlight" maskUnits="userSpaceOnUse">
            <rect x={0} y={0} width="100%" height="100%" fill="#fff" />
            {rect && (
              <rect
                fill="#000"
                rx={CUT_RADIUS}
                style={
                  {
                    x: rect.x,
                    y: rect.y,
                    width: rect.w,
                    height: rect.h,
                    transition: GEOMETRY_TRANSITION,
                  }
                }
              />
            )}
          </mask>
        </defs>
        <rect
          x={0}
          y={0}
          width="100%"
          height="100%"
          fill="var(--color-bg-overlay)"
          mask="url(#vofa-tour-spotlight)"
        />
      </svg>
      {rect && (
        <div
          className="fixed rounded-[10px] border-[1.5px] border-accent"
          style={{
            left: rect.x,
            top: rect.y,
            width: rect.w,
            height: rect.h,
            boxShadow: '0 0 0 4px color-mix(in srgb, var(--color-accent) 22%, transparent)',
            transition: BOX_TRANSITION,
            pointerEvents: 'none',
          }}
        />
      )}
    </>
  );
}
