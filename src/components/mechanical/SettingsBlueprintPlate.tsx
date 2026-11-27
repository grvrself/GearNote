import { useMemo, type CSSProperties } from 'react'
import './mechanical.css'

type Props = {
  /** 当前激活设置分组索引（0～7） */
  activeIndex: number
  /** 当前分组两位数序号（01～08） */
  sectionCode: string
}

const MATRIX_COLS = 10
const MATRIX_ROWS = 18
const CAD_CX = 60
const CAD_CY = 56

const CALLOUTS = [
  { code: 'AX-01', path: 'M44 40 L30 26 H8', x: 8, y: 22.5, anchor: 'start' as const },
  { code: 'R.24', path: 'M76 40 L90 26 H122', x: 122, y: 22.5, anchor: 'end' as const },
  { code: 'SYNC', path: 'M45 71 L31 85 H8', x: 8, y: 81.5, anchor: 'start' as const },
  { code: '0.02', path: 'M74 70 L88 84 H122', x: 122, y: 80.5, anchor: 'end' as const },
] as const

/**
 * HUD 主题设置页 `CONTROL / 08` 分组栏下方空白区的 CAD 精密机械铭牌：
 * 含左上序列号、8×8px 四角取景框、左侧 10×18 确定性像素点阵与微型遥测条、
 * 右侧 58mm 轴承剖面线稿与 45° 工程引线，并随左侧分组切换同步显示序号与 45° 内环转动。
 */
export function SettingsBlueprintPlate({ activeIndex, sectionCode }: Props) {
  const dots = useMemo(() => {
    const items: { key: string; x: number; y: number; opacity: number }[] = []
    for (let row = 0; row < MATRIX_ROWS; row++) {
      for (let col = 0; col < MATRIX_COLS; col++) {
        const hash = ((row * 37 + col * 19 + row * col * 7) % 23) / 100
        items.push({
          key: `${row}-${col}`,
          x: 1 + col * 4,
          y: 1 + row * 4,
          opacity: Number((0.06 + hash).toFixed(2)),
        })
      }
    }
    return items
  }, [])

  const safeIndex = Math.max(0, activeIndex)
  const cadAngle = safeIndex * 45

  return (
    <div className="settings-blueprint-plate" aria-hidden="true">
      <div className="blueprint-plate-head">
        <span className="blueprint-serial">GN.CTRL // {sectionCode}</span>
        <span className="blueprint-serial-sub">CAD-SPEC</span>
      </div>

      <div className="blueprint-chamber">
        <span className="blueprint-corner blueprint-corner--tl" />
        <span className="blueprint-corner blueprint-corner--tr" />
        <span className="blueprint-corner blueprint-corner--bl" />
        <span className="blueprint-corner blueprint-corner--br" />

        {/* 左侧列：微型遥测头 + 10×18 像素点阵数据矩阵 */}
        <div className="blueprint-telemetry-col">
          <div className="blueprint-telemetry-head">
            <span><i />LOCAL.CFG</span>
            <div className="blueprint-micro-bars">
              <span className="blueprint-micro-progress"><b style={{ width: `${38 + safeIndex * 8}%` }} /></span>
              <span className="blueprint-micro-dashes"><i /><i /></span>
            </div>
          </div>

          <svg className="blueprint-matrix-svg" viewBox="0 0 42 74" width="42" height="74" fill="none">
            {dots.map(dot => (
              <rect
                key={dot.key}
                className="blueprint-matrix-cell"
                x={dot.x}
                y={dot.y}
                width="1.5"
                height="1.5"
                fill="#FFFFFF"
                opacity={dot.opacity}
              />
            ))}
          </svg>
        </div>

        {/* 右侧列：CAD 机械轴承剖面示意图（viewBox 130×110） */}
        <div className="blueprint-cad-col">
          <svg className="blueprint-cad-svg" viewBox="0 0 130 110" fill="none">
            <g stroke="rgba(255, 255, 255, 0.34)" strokeWidth="0.85">
              {/* 外圈 270° 剖切弧（右上 12 点至 3 点留出 90° 剖切缺口露出内部结构，直径 58px -> r=29） */}
              <path d={`M ${CAD_CX + 29} ${CAD_CY} A 29 29 0 1 1 ${CAD_CX} ${CAD_CY - 29}`} />
              <line x1={CAD_CX} y1={CAD_CY - 29} x2={CAD_CX} y2={CAD_CY - 21} strokeDasharray="1.5 1.5" />
              <line x1={CAD_CX + 29} y1={CAD_CY} x2={CAD_CX + 21} y2={CAD_CY} strokeDasharray="1.5 1.5" />

              {/* 第二圈滚道环 r=21 */}
              <circle cx={CAD_CX} cy={CAD_CY} r="21" strokeDasharray="4 2.5" opacity="0.82" />

              {/* 十字定位轴线 */}
              <path
                d={`M${CAD_CX - 36} ${CAD_CY}h72M${CAD_CX} ${CAD_CY - 36}v72`}
                strokeDasharray="3 2 1 2"
                strokeWidth="0.65"
                opacity="0.52"
              />
            </g>

            {/* 中心双圈轴孔 + 随分组切换旋转的内环组件 */}
            <g
              className="blueprint-cad-inner"
              style={{ '--cad-angle': `${cadAngle}deg` } as CSSProperties}
              stroke="rgba(255, 255, 255, 0.36)"
              strokeWidth="0.85"
            >
              <circle cx={CAD_CX} cy={CAD_CY} r="15" stroke="rgba(255,255,255,0.01)" strokeWidth="0.1" />
              <circle cx={CAD_CX} cy={CAD_CY} r="14" strokeDasharray="6 3" />
              <circle cx={CAD_CX} cy={CAD_CY} r="8" />
              <circle cx={CAD_CX} cy={CAD_CY} r="4" opacity="0.72" />
              <line x1={CAD_CX - 14} y1={CAD_CY} x2={CAD_CX - 8} y2={CAD_CY} strokeWidth="0.9" />
              <line x1={CAD_CX + 8} y1={CAD_CY} x2={CAD_CX + 14} y2={CAD_CY} strokeWidth="0.9" />
            </g>

            {/* 4 条经典 45° 折线 CAD 标注引线与微缩代号 */}
            <g className="blueprint-cad-callouts">
              {CALLOUTS.map(item => (
                <g key={item.code}>
                  <path d={item.path} stroke="rgba(255, 255, 255, 0.24)" strokeWidth="0.65" fill="none" />
                  <text
                    x={item.x}
                    y={item.y}
                    textAnchor={item.anchor}
                    fontFamily="var(--font-mono), monospace"
                    fontSize="6.5"
                    fill="rgba(255, 255, 255, 0.38)"
                  >
                    {item.code}
                  </text>
                </g>
              ))}
            </g>
          </svg>
        </div>
      </div>
    </div>
  )
}
