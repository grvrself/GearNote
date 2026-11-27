import { useEffect, useMemo, useState } from 'react'
import { readMotion, readTheme, type MotionPreference, type Theme } from '../../theme'
import './mechanical.css'

type Props = {
  /** 浏览器扩展桥接是否处于已连接状态 */
  connected?: boolean
}

const DIAL_CX = 130
const DIAL_CY = 110
const SUB_CX = 94
const SUB_CY = 128
const SUB_R = 21

const ROMAN_NUMERALS = ['XII', 'I', 'II', 'III', 'IV', 'V', 'VI', 'VII', 'VIII', 'IX', 'X', 'XI'] as const

const round = (value: number) => Number(value.toFixed(2))

function clockPoint(cx: number, cy: number, angleDeg: number, radius: number) {
  const rad = ((angleDeg - 90) * Math.PI) / 180
  return {
    x: round(cx + Math.cos(rad) * radius),
    y: round(cy + Math.sin(rad) * radius),
  }
}

/**
 * 简约黑白（mono）主题「连接与插件」页眉复古精密怀表循环动画组件：
 * - 纯程序化铜版画细线 SVG（viewBox 340×220，表盘圆心 (130, 110)），3 点钟方向水平伸出洋葱头表冠与提环；
 * - 严格仅在 `mono` 主题下渲染节点，绝不影响黑黄（hud）主题与任何插件逻辑；
 * - 仅当「浏览器桥接已连接」且「05 界面与动效」为完整动效时走动，否则静驻在优雅的 10 点 10 分构图角。
 */
export function PocketWatchBackdrop({ connected = false }: Props) {
  const [theme, setTheme] = useState<Theme>(readTheme)
  const [motion, setMotion] = useState<MotionPreference>(readMotion)

  useEffect(() => {
    const syncTheme = () => setTheme(readTheme())
    const onTheme = (event: Event) => {
      const detail = (event as CustomEvent<Theme>).detail
      setTheme(detail ?? readTheme())
    }
    const syncMotion = () => setMotion(readMotion())
    const onMotion = (event: Event) => {
      const detail = (event as CustomEvent<MotionPreference>).detail
      setMotion(detail ?? readMotion())
    }

    window.addEventListener('knowflow:theme', onTheme)
    window.addEventListener('knowflow:motion', onMotion)
    const onStorage = () => {
      syncTheme()
      syncMotion()
    }
    window.addEventListener('storage', onStorage)
    return () => {
      window.removeEventListener('knowflow:theme', onTheme)
      window.removeEventListener('knowflow:motion', onMotion)
      window.removeEventListener('storage', onStorage)
    }
  }, [])

  const dial = useMemo(() => {
    // 1. 钱币纹表壳边缘（r=91 ~ r=96，避开 3 点钟表颈连接区）
    const coinTicks = Array.from({ length: 180 }, (_, index) => {
      const angleDeg = index * 2
      if (angleDeg >= 84 && angleDeg <= 96) return null
      const inner = clockPoint(DIAL_CX, DIAL_CY, angleDeg, 91.3)
      const outer = clockPoint(DIAL_CX, DIAL_CY, angleDeg, 95.7)
      return { angleDeg, x1: inner.x, y1: inner.y, x2: outer.x, y2: outer.y }
    }).filter((item): item is NonNullable<typeof item> => item !== null)

    // 2. 铁轨式 60 格分钟刻度（r=76 ~ r=81，每 5 格加粗加长）
    const minuteTicks = Array.from({ length: 60 }, (_, index) => {
      const angleDeg = index * 6
      const major = index % 5 === 0
      const inner = clockPoint(DIAL_CX, DIAL_CY, angleDeg, major ? 74.8 : 76.2)
      const outer = clockPoint(DIAL_CX, DIAL_CY, angleDeg, major ? 81.4 : 80.8)
      return { index, major, x1: inner.x, y1: inner.y, x2: outer.x, y2: outer.y }
    })

    // 3. r=65 罗马数字整点时标
    const numerals = ROMAN_NUMERALS.map((label, index) => {
      const pos = clockPoint(DIAL_CX, DIAL_CY, index * 30, 65)
      return { label, x: pos.x, y: pos.y }
    })

    // 4. 左下偏心小秒盘 12 格微刻度
    const subTicks = Array.from({ length: 12 }, (_, index) => {
      const angleDeg = index * 30
      const major = index % 3 === 0
      const inner = clockPoint(SUB_CX, SUB_CY, angleDeg, major ? 14.2 : 15.8)
      const outer = clockPoint(SUB_CX, SUB_CY, angleDeg, 18.8)
      return { index, major, x1: inner.x, y1: inner.y, x2: outer.x, y2: outer.y }
    })

    return { coinTicks, minuteTicks, numerals, subTicks }
  }, [])

  if (theme !== 'mono') {
    return null
  }

  return (
    <div
      className="pocket-watch-backdrop"
      data-connected={connected ? 'true' : 'false'}
      data-motion={motion}
      aria-hidden="true"
    >
      <svg viewBox="0 0 340 220" fill="none" strokeLinecap="round" strokeLinejoin="round">
        {/* 1. 复古表壳三层同心细线 + 钱币纹表壳边缘 */}
        <g className="pocket-watch-case" stroke="rgba(255, 255, 255, 0.24)" strokeWidth="0.85">
          <circle cx={DIAL_CX} cy={DIAL_CY} r="96" />
          <circle cx={DIAL_CX} cy={DIAL_CY} r="91" />
          <circle cx={DIAL_CX} cy={DIAL_CY} r="84" />
          <circle cx={DIAL_CX} cy={DIAL_CY} r="88" stroke="rgba(255, 255, 255, 0.08)" strokeWidth="0.5" />
          <g stroke="rgba(255, 255, 255, 0.20)" strokeWidth="0.6">
            {dial.coinTicks.map(tick => (
              <line key={tick.angleDeg} x1={tick.x1} y1={tick.y1} x2={tick.x2} y2={tick.y2} />
            ))}
          </g>
        </g>

        {/* 1b. 3 点钟位置 (226, 110) 水平表颈、滚花洋葱头表冠与椭圆提环 */}
        <g className="pocket-watch-crown" stroke="rgba(255, 255, 255, 0.28)" strokeWidth="0.85">
          {/* 表颈底座与阶梯套管 */}
          <path d="M225.4 101.6 H230.2 V118.4 H225.4" />
          <path d="M230.2 103.8 H240.4 V116.2 H230.2" />
          <line x1="227.8" y1="101.8" x2="227.8" y2="118.2" stroke="rgba(255, 255, 255, 0.16)" strokeWidth="0.6" />
          <line x1="233.6" y1="104" x2="233.6" y2="116" stroke="rgba(255, 255, 255, 0.16)" strokeWidth="0.6" />
          <line x1="237.4" y1="104" x2="237.4" y2="116" stroke="rgba(255, 255, 255, 0.16)" strokeWidth="0.6" />
          <line x1="230.8" y1="106.8" x2="239.8" y2="106.8" stroke="rgba(255, 255, 255, 0.14)" strokeWidth="0.5" />
          <line x1="230.8" y1="113.2" x2="239.8" y2="113.2" stroke="rgba(255, 255, 255, 0.14)" strokeWidth="0.5" />
          {/* 提环耳轴销 */}
          <circle cx="235.4" cy="102.6" r="1.4" strokeWidth="0.65" />
          <circle cx="235.4" cy="117.4" r="1.4" strokeWidth="0.65" />

          {/* 复古洋葱头表冠外轮廓与顶部微钮 */}
          <path d="M240.4 103.8 C243.4 96.2 251.4 94.4 257.6 98.2 C261.2 100.4 263.2 105.2 263.2 110 C263.2 114.8 261.2 119.6 257.6 121.8 C251.4 125.6 243.4 123.8 240.4 116.2" />
          <path d="M263.2 107.6 C264.8 107.6 265.6 108.6 265.6 110 C265.6 111.4 264.8 112.4 263.2 112.4" strokeWidth="0.7" />
          {/* 表冠竖向滚花细线 */}
          <g stroke="rgba(255, 255, 255, 0.22)" strokeWidth="0.6">
            <path d="M243.4 99.6 Q244.8 110 243.4 120.4" />
            <path d="M246.4 97.4 Q248 110 246.4 122.6" />
            <path d="M249.6 96.4 Q251.4 110 249.6 123.6" />
            <path d="M252.8 96.7 Q254.6 110 252.8 123.3" />
            <path d="M255.8 97.8 Q257.6 110 255.8 122.2" />
            <path d="M258.8 99.8 Q260.4 110 258.8 120.2" />
            <path d="M261.2 103.4 Q262.2 110 261.2 116.6" />
          </g>

          {/* 套在表冠外侧的椭圆怀表提环（双轮廓细金属丝） */}
          <path d="M234.4 101.6 C242.5 85.8 264.5 81 280.5 86 C292.2 89.8 296.4 98.8 296.4 110 C296.4 121.2 292.2 130.2 280.5 134 C264.5 139 242.5 134.2 234.4 118.4" />
          <path d="M237 103.6 C244.2 90 263.4 85.8 277.8 90 C287.6 92.9 291.4 100.4 291.4 110 C291.4 119.6 287.6 127.1 277.8 130 C263.4 134.2 244.2 130 237 116.4" stroke="rgba(255, 255, 255, 0.22)" strokeWidth="0.75" />
          <path d="M244 92.8 C256 85.6 271.5 85.5 282.8 90.2" stroke="rgba(255, 255, 255, 0.14)" strokeWidth="0.5" />
        </g>

        {/* 2. 铁轨式分钟刻度圈（r=76 ~ r=81） */}
        <g className="pocket-watch-railroad">
          <circle cx={DIAL_CX} cy={DIAL_CY} r="81" stroke="rgba(255, 255, 255, 0.26)" strokeWidth="0.75" />
          <circle cx={DIAL_CX} cy={DIAL_CY} r="76" stroke="rgba(255, 255, 255, 0.22)" strokeWidth="0.7" />
          {dial.minuteTicks.map(tick => (
            <line
              key={tick.index}
              x1={tick.x1}
              y1={tick.y1}
              x2={tick.x2}
              y2={tick.y2}
              stroke={tick.major ? 'rgba(255, 255, 255, 0.44)' : 'rgba(255, 255, 255, 0.24)'}
              strokeWidth={tick.major ? 1.15 : 0.65}
            />
          ))}
        </g>

        {/* 2b. r=65 衬线罗马数字时标 */}
        <g
          className="pocket-watch-numerals"
          fill="rgba(255, 255, 255, 0.34)"
          stroke="none"
          fontFamily='Georgia, "Times New Roman", serif'
          fontSize="11"
          textAnchor="middle"
          dominantBaseline="central"
        >
          {dial.numerals.map(item => (
            <text key={item.label} x={item.x} y={item.y}>
              {item.label}
            </text>
          ))}
        </g>

        {/* 3. 内圈细纹、十字定位细线与内嵌小秒盘 (94, 128) */}
        <g className="pocket-watch-inner">
          <circle cx={DIAL_CX} cy={DIAL_CY} r="53" stroke="rgba(255, 255, 255, 0.14)" strokeWidth="0.7" />
          <path
            d={`M${DIAL_CX - 53} ${DIAL_CY}H${DIAL_CX + 53}M${DIAL_CX} ${DIAL_CY - 53}V${DIAL_CY + 53}`}
            stroke="rgba(255, 255, 255, 0.08)"
            strokeWidth="0.55"
            strokeDasharray="2 3"
          />

          <g className="pocket-watch-subdial">
            <circle cx={SUB_CX} cy={SUB_CY} r={SUB_R} stroke="rgba(255, 255, 255, 0.24)" strokeWidth="0.75" />
            <circle cx={SUB_CX} cy={SUB_CY} r="18.6" stroke="rgba(255, 255, 255, 0.13)" strokeWidth="0.55" />
            {dial.subTicks.map(tick => (
              <line
                key={tick.index}
                x1={tick.x1}
                y1={tick.y1}
                x2={tick.x2}
                y2={tick.y2}
                stroke={tick.major ? 'rgba(255, 255, 255, 0.36)' : 'rgba(255, 255, 255, 0.20)'}
                strokeWidth={tick.major ? 0.85 : 0.55}
              />
            ))}
            <g transform={`rotate(35 ${SUB_CX} ${SUB_CY})`}>
              <g className="pocket-watch-hand pocket-watch-hand--sub" stroke="rgba(255, 255, 255, 0.58)" strokeWidth="0.75">
                <line x1={SUB_CX} y1={SUB_CY + 4} x2={SUB_CX} y2={SUB_CY - 14.5} />
              </g>
            </g>
            <circle cx={SUB_CX} cy={SUB_CY} r="1.7" stroke="rgba(255, 255, 255, 0.45)" strokeWidth="0.7" />
          </g>
        </g>

        {/* 4. 宝玑风格三指针（静态默认停在优雅的 10 点 10 分夹角，联动动画绕表盘圆心旋转） */}
        <g className="pocket-watch-hands">
          {/* 时针（长约 42px，经典 10 点 10 分位置 -55°） */}
          <g transform={`rotate(-55 ${DIAL_CX} ${DIAL_CY})`}>
            <g className="pocket-watch-hand pocket-watch-hand--hour" stroke="rgba(255, 255, 255, 0.48)" strokeWidth="1.2">
              <path d={`M${DIAL_CX - 1.3} ${DIAL_CY + 7} L${DIAL_CX - 1.6} ${DIAL_CY} L${DIAL_CX - 0.9} ${DIAL_CY - 28.5}`} />
              <path d={`M${DIAL_CX + 1.3} ${DIAL_CY + 7} L${DIAL_CX + 1.6} ${DIAL_CY} L${DIAL_CX + 0.9} ${DIAL_CY - 28.5}`} />
              <line x1={DIAL_CX - 1.3} y1={DIAL_CY + 7} x2={DIAL_CX + 1.3} y2={DIAL_CY + 7} />
              <circle cx={DIAL_CX} cy={DIAL_CY - 32.3} r="3.8" />
              <line x1={DIAL_CX} y1={DIAL_CY - 36.1} x2={DIAL_CX} y2={DIAL_CY - 42} />
            </g>
          </g>

          {/* 分针（长约 62px，10 分位置 60°） */}
          <g transform={`rotate(60 ${DIAL_CX} ${DIAL_CY})`}>
            <g className="pocket-watch-hand pocket-watch-hand--minute" stroke="rgba(255, 255, 255, 0.55)" strokeWidth="1.05">
              <path d={`M${DIAL_CX - 1.1} ${DIAL_CY + 9} L${DIAL_CX - 1.35} ${DIAL_CY} L${DIAL_CX - 0.75} ${DIAL_CY - 48}`} />
              <path d={`M${DIAL_CX + 1.1} ${DIAL_CY + 9} L${DIAL_CX + 1.35} ${DIAL_CY} L${DIAL_CX + 0.75} ${DIAL_CY - 48}`} />
              <line x1={DIAL_CX - 1.1} y1={DIAL_CY + 9} x2={DIAL_CX + 1.1} y2={DIAL_CY + 9} />
              <circle cx={DIAL_CX} cy={DIAL_CY - 51.4} r="3.4" />
              <line x1={DIAL_CX} y1={DIAL_CY - 54.8} x2={DIAL_CX} y2={DIAL_CY - 62} />
            </g>
          </g>

          {/* 秒针（长约 72px，尾部带平衡配重小圆环） */}
          <g transform={`rotate(150 ${DIAL_CX} ${DIAL_CY})`}>
            <g className="pocket-watch-hand pocket-watch-hand--second" stroke="rgba(255, 255, 255, 0.75)" strokeWidth="0.8">
              <line x1={DIAL_CX} y1={DIAL_CY + 9.4} x2={DIAL_CX} y2={DIAL_CY - 72} />
              <circle cx={DIAL_CX} cy={DIAL_CY + 12} r="2.6" />
              <line x1={DIAL_CX} y1={DIAL_CY + 14.6} x2={DIAL_CX} y2={DIAL_CY + 17.5} />
            </g>
          </g>

          {/* 圆心 r=3.5 双圈中心轴盖 */}
          <circle cx={DIAL_CX} cy={DIAL_CY} r="3.5" stroke="rgba(255, 255, 255, 0.62)" strokeWidth="0.9" />
          <circle cx={DIAL_CX} cy={DIAL_CY} r="1.6" stroke="rgba(255, 255, 255, 0.82)" strokeWidth="0.8" />
        </g>
      </svg>
    </div>
  )
}
