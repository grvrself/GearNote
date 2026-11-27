import { useId, useMemo } from 'react'
import { generateGearGeometry } from './dialGeometry'
import './mechanical.css'

const CX = 110
const CY = 110
const TEETH = 16
const MODULE = 9.0
const SPOKE_COUNT = 5
const EXTRUDE_DX = -5
const EXTRUDE_DY = 4

const round = (v: number) => Number(v.toFixed(2))

/**
 * 伪 3D 机械浮雕齿轮（viewBox 220×220，纯 SVG + CSS）：
 * 1. 背面挤出层（错位 translate(-5, 4)）；
 * 2. 64 条齿顶/齿根挤出连接棱线（Extrusion Side Edges）；
 * 3. 正面定向受光渐变轮廓 + 错位阶梯内腔凹槽 + 双边线工业轮辐 + 轴孔倒角高光弧；
 * 4. 外层 CSS 3D 透视倾角 perspective(480px) rotateY(-18deg) rotateX(10deg) 与径向消隐。
 */
export function ExtrudedGear3D() {
  const scopeId = useId().replace(/:/g, '')
  const strokeGradId = `${scopeId}-e3d-stroke`
  const fillGradId = `${scopeId}-e3d-fill`

  const geometry = useMemo(() => {
    const { outerPath, outerPoints } = generateGearGeometry({
      teeth: TEETH,
      module: MODULE,
      innerRadius: 22,
      spokeCount: SPOKE_COUNT,
      cx: CX,
      cy: CY,
      rotationDeg: -8,
    })

    // 5 根带双边线厚度的工业轮辐（从 r=23 延伸到 r=61）
    const spokes = Array.from({ length: SPOKE_COUNT }, (_, idx) => {
      const angle = (idx / SPOKE_COUNT) * Math.PI * 2 - 0.14
      const cos = Math.cos(angle)
      const sin = Math.sin(angle)
      const nx = -sin
      const ny = cos
      const halfWidth = 4.2
      const rIn = 23
      const rOut = 61
      const edgeA = `M${round(CX + cos * rIn + nx * halfWidth)} ${round(CY + sin * rIn + ny * halfWidth)} L${round(CX + cos * rOut + nx * halfWidth)} ${round(CY + sin * rOut + ny * halfWidth)}`
      const edgeB = `M${round(CX + cos * rIn - nx * halfWidth)} ${round(CY + sin * rIn - ny * halfWidth)} L${round(CX + cos * rOut - nx * halfWidth)} ${round(CY + sin * rOut - ny * halfWidth)}`
      const sideEdge = `M${round(CX + cos * rIn + nx * halfWidth)} ${round(CY + sin * rIn + ny * halfWidth)} L${round(CX + cos * rIn + nx * halfWidth - 2)} ${round(CY + sin * rIn + ny * halfWidth + 1.5)}`
      return { edgeA, edgeB, sideEdge }
    })

    return { outerPath, outerPoints, spokes }
  }, [])

  return (
    <div className="extruded-gear-3d" aria-hidden="true">
      <div className="extruded-gear-3d-tilt">
        <svg viewBox="0 0 220 220" fill="none">
          <defs>
            <linearGradient id={strokeGradId} x1="88%" y1="8%" x2="12%" y2="92%">
              <stop offset="0%" stopColor="rgba(252, 238, 10, 0.42)" />
              <stop offset="52%" stopColor="rgba(252, 238, 10, 0.24)" />
              <stop offset="100%" stopColor="rgba(252, 238, 10, 0.12)" />
            </linearGradient>
            <linearGradient id={fillGradId} x1="85%" y1="10%" x2="15%" y2="90%">
              <stop offset="0%" stopColor="rgba(252, 238, 10, 0.06)" />
              <stop offset="55%" stopColor="rgba(252, 238, 10, 0.02)" />
              <stop offset="100%" stopColor="rgba(10, 12, 16, 0.35)" />
            </linearGradient>
          </defs>

          <g className="extruded-gear-3d-rotator">
            {/* 对称隐形导引圆固定 fill-box center */}
            <circle cx={CX} cy={CY} r="86" stroke="rgba(252, 238, 10, 0.01)" strokeWidth="0.1" />

            {/* 层 1：背面挤出层 Back Face */}
            <g transform={`translate(${EXTRUDE_DX}, ${EXTRUDE_DY})`}>
              <path
                d={geometry.outerPath}
                fill="rgba(252, 238, 10, 0.04)"
                stroke="rgba(252, 238, 10, 0.14)"
                strokeWidth="0.9"
                strokeLinejoin="round"
              />
              <circle cx={CX} cy={CY} r="62" stroke="rgba(252, 238, 10, 0.10)" strokeWidth="0.7" />
              <circle cx={CX} cy={CY} r="34" stroke="rgba(252, 238, 10, 0.10)" strokeWidth="0.7" />
            </g>

            {/* 层 2：齿顶与齿根挤出连接棱线（立体厚度感核心） */}
            <g className="extruded-gear-edges" stroke="rgba(252, 238, 10, 0.16)" strokeWidth="0.75">
              {geometry.outerPoints.map((pt, idx) => (
                <line
                  key={idx}
                  className="extruded-gear-edge"
                  x1={pt.x}
                  y1={pt.y}
                  x2={round(pt.x + EXTRUDE_DX)}
                  y2={round(pt.y + EXTRUDE_DY)}
                />
              ))}
            </g>

            {/* 层 3：正面主体 + 阶梯凹槽内环 + 双边线工业轮辐 + 轴孔反光 */}
            <path
              d={geometry.outerPath}
              fill={`url(#${fillGradId})`}
              stroke={`url(#${strokeGradId})`}
              strokeWidth="1.15"
              strokeLinejoin="round"
            />

            {/* 阶梯凹槽内环 r=62, 48, 34, 22，其中 r=62 / r=34 叠加错位内壁台阶 (-2, 1.5) */}
            <g transform="translate(-2, 1.5)" stroke="rgba(252, 238, 10, 0.14)" strokeWidth="0.75">
              <circle cx={CX} cy={CY} r="62" />
              <circle cx={CX} cy={CY} r="34" />
            </g>

            <circle cx={CX} cy={CY} r="62" stroke="rgba(252, 238, 10, 0.28)" strokeWidth="0.9" />
            <circle cx={CX} cy={CY} r="48" stroke="rgba(252, 238, 10, 0.18)" strokeWidth="0.75" strokeDasharray="3 3" />
            <circle cx={CX} cy={CY} r="34" stroke="rgba(252, 238, 10, 0.30)" strokeWidth="0.85" />
            <circle cx={CX} cy={CY} r="22" stroke="rgba(252, 238, 10, 0.26)" strokeWidth="0.95" />
            <circle cx={CX} cy={CY} r="11" stroke="rgba(252, 238, 10, 0.20)" strokeWidth="0.8" />

            {/* 十字细刻线 */}
            <path
              d={`M${CX} 40v140M40 ${CY}h140`}
              stroke="rgba(252, 238, 10, 0.14)"
              strokeWidth="0.6"
              strokeDasharray="2 4"
            />

            {/* 5 根双边线厚度轮辐 */}
            <g stroke="rgba(252, 238, 10, 0.26)" strokeWidth="0.85">
              {geometry.spokes.map((spoke, idx) => (
                <g key={idx}>
                  <path d={spoke.edgeA} />
                  <path d={spoke.edgeB} />
                  <path d={spoke.sideEdge} stroke="rgba(252, 238, 10, 0.18)" strokeWidth="0.65" />
                </g>
              ))}
            </g>

            {/* 中心轴孔右上受光倒角亮弧 */}
            <path
              d={`M${CX} ${CY - 22} A22 22 0 0 1 ${CX + 22} ${CY}`}
              stroke="rgba(252, 238, 10, 0.48)"
              strokeWidth="1.45"
              strokeLinecap="round"
            />
          </g>
        </svg>
      </div>
    </div>
  )
}
