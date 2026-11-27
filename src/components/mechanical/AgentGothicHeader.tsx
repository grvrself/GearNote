import { useMemo } from 'react'
import agentGothicArt from '../../assets/agent-gothic-art.jpg'
import { useThemeAndMotion } from './useThemeAndMotion'
import './mechanical.css'

type Props = {
  /** Agent 是否处于思考或流式输出态 */
  active?: boolean
}

const CX = 130
const CY = 130

const round = (value: number) => Number(value.toFixed(2))

function polarPoint(angleDeg: number, radius: number) {
  const rad = ((angleDeg - 90) * Math.PI) / 180
  return {
    x: round(CX + Math.cos(rad) * radius),
    y: round(CY + Math.sin(rad) * radius),
  }
}

/**
 * 简约黑白（mono）主题「Agent 对话」页眉艺术横幅：
 * - 底层（z-index: 1）：纯 SVG 哥特尖拱曼陀罗光环（3 圈细圆环、12 组尖拱花窗弧线、24 条射线与星芒十字刻线），
 *   空闲态 60s 极缓旋转、思考/流式输出态提速至 18s，严格挂接三档动效开关；
 * - 上层（z-index: 2）：静止居中的版画骷髅主体，通过 screen 混合与紧凑椭圆径向遮罩淡出原图静态光环，
 *   让底层旋转的 SVG 光环无缝承接在骷髅周围；
 * - 严格仅在 `mono` 主题下渲染节点，不影响 `hud` 主题与正文阅读区。
 */
export function AgentGothicHeader({ active = false }: Props) {
  const { theme, motion } = useThemeAndMotion()

  const mandala = useMemo(() => {
    // 1. 12 组对称哥特式尖拱花窗弧线 + 内层小尖拱 + 交错棘尖
    const arches = Array.from({ length: 12 }, (_, index) => {
      const centerDeg = index * 30
      const span = 14
      const leftBase = polarPoint(centerDeg - span, 78)
      const leftCtrlA = polarPoint(centerDeg - 11.2, 95)
      const leftCtrlB = polarPoint(centerDeg - 3.8, 104)
      const apex = polarPoint(centerDeg, 111)
      const rightCtrlB = polarPoint(centerDeg + 3.8, 104)
      const rightCtrlA = polarPoint(centerDeg + 11.2, 95)
      const rightBase = polarPoint(centerDeg + span, 78)

      const innerL = polarPoint(centerDeg - 8.5, 78)
      const innerCtrlL = polarPoint(centerDeg - 5.2, 89)
      const innerApex = polarPoint(centerDeg, 95.5)
      const innerCtrlR = polarPoint(centerDeg + 5.2, 89)
      const innerR = polarPoint(centerDeg + 8.5, 78)

      // 相邻尖拱交界处的外伸棘状尖端（+15°）
      const cuspDeg = centerDeg + 15
      const cuspL = polarPoint(cuspDeg - 7.5, 96)
      const cuspCtrlL = polarPoint(cuspDeg - 3.2, 108)
      const cuspTip = polarPoint(cuspDeg, 118.5)
      const cuspCtrlR = polarPoint(cuspDeg + 3.2, 108)
      const cuspR = polarPoint(cuspDeg + 7.5, 96)

      return {
        index,
        outerPath: `M ${leftBase.x} ${leftBase.y} C ${leftCtrlA.x} ${leftCtrlA.y} ${leftCtrlB.x} ${leftCtrlB.y} ${apex.x} ${apex.y} C ${rightCtrlB.x} ${rightCtrlB.y} ${rightCtrlA.x} ${rightCtrlA.y} ${rightBase.x} ${rightBase.y}`,
        innerPath: `M ${innerL.x} ${innerL.y} Q ${innerCtrlL.x} ${innerCtrlL.y} ${innerApex.x} ${innerApex.y} Q ${innerCtrlR.x} ${innerCtrlR.y} ${innerR.x} ${innerR.y}`,
        cuspPath: `M ${cuspL.x} ${cuspL.y} Q ${cuspCtrlL.x} ${cuspCtrlL.y} ${cuspTip.x} ${cuspTip.y} Q ${cuspCtrlR.x} ${cuspCtrlR.y} ${cuspR.x} ${cuspR.y}`,
      }
    })

    // 2. 24 条放射状纤细尖刺射线，隔条点缀十字星芒刻线
    const rays = Array.from({ length: 24 }, (_, index) => {
      const angleDeg = index * 15
      const major = index % 2 === 0
      const inner = polarPoint(angleDeg, major ? 74 : 80)
      const outer = polarPoint(angleDeg, major ? 126 : 120)

      // 垂直于射线方向的单位法向量，用于绘制十字星芒小横刻线
      const rad = ((angleDeg - 90) * Math.PI) / 180
      const nx = -Math.sin(rad)
      const ny = Math.cos(rad)
      const starRadius = major ? 120 : 104
      const halfBar = major ? 2.8 : 2.1
      const starCenter = polarPoint(angleDeg, starRadius)
      const cross = {
        x1: round(starCenter.x - nx * halfBar),
        y1: round(starCenter.y - ny * halfBar),
        x2: round(starCenter.x + nx * halfBar),
        y2: round(starCenter.y + ny * halfBar),
      }
      // 奇数射线额外加一小段斜星芒，丰富铜版画质感
      const glintStart = polarPoint(angleDeg, starRadius - 2.4)
      const glintEnd = polarPoint(angleDeg, starRadius + 2.4)

      return {
        index,
        major,
        x1: inner.x,
        y1: inner.y,
        x2: outer.x,
        y2: outer.y,
        cross,
        glint: major ? null : { x1: glintStart.x, y1: glintStart.y, x2: glintEnd.x, y2: glintEnd.y },
      }
    })

    return { arches, rays }
  }, [])

  if (theme !== 'mono') {
    return null
  }

  return (
    <div
      className="agent-gothic-banner"
      data-active={active ? 'true' : 'false'}
      data-motion={motion}
      aria-hidden="true"
    >
      <div className="agent-gothic-halo">
        <svg
          className="agent-gothic-halo-svg"
          viewBox="0 0 260 260"
          fill="none"
          stroke="rgba(255, 255, 255, 0.28)"
          strokeWidth="0.85"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          {/* 3 圈同心细圆环 */}
          <g className="agent-gothic-rings">
            <circle cx={CX} cy={CY} r="112" />
            <circle cx={CX} cy={CY} r="96" />
            <circle cx={CX} cy={CY} r="78" />
            <circle cx={CX} cy={CY} r="84" strokeWidth="0.55" opacity="0.45" />
          </g>

          {/* 12 组哥特式尖拱花窗弧线 */}
          <g className="agent-gothic-arches">
            {mandala.arches.map(arch => (
              <g key={arch.index}>
                <path d={arch.outerPath} />
                <path d={arch.innerPath} strokeWidth="0.65" opacity="0.72" />
                <path d={arch.cuspPath} strokeWidth="0.7" opacity="0.82" />
              </g>
            ))}
          </g>

          {/* 24 条放射状尖刺射线与隔条十字星芒刻线 */}
          <g className="agent-gothic-rays">
            {mandala.rays.map(ray => (
              <g key={ray.index} opacity={ray.major ? 0.92 : 0.72}>
                <line
                  x1={ray.x1}
                  y1={ray.y1}
                  x2={ray.x2}
                  y2={ray.y2}
                  strokeWidth={ray.major ? 0.85 : 0.65}
                />
                <line
                  x1={ray.cross.x1}
                  y1={ray.cross.y1}
                  x2={ray.cross.x2}
                  y2={ray.cross.y2}
                  strokeWidth={ray.major ? 0.8 : 0.65}
                />
                {ray.glint && (
                  <line
                    x1={ray.glint.x1}
                    y1={ray.glint.y1}
                    x2={ray.glint.x2}
                    y2={ray.glint.y2}
                    strokeWidth="0.65"
                  />
                )}
              </g>
            ))}
          </g>
        </svg>
      </div>

      <img
        className="agent-gothic-skull"
        src={agentGothicArt}
        alt=""
        draggable={false}
      />
    </div>
  )
}
