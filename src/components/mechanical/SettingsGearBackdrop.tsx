import { useEffect, useMemo, useState, type CSSProperties } from 'react'
import { readMotion, type MotionPreference } from '../../theme'
import {
  generateAngleLabels,
  generateGearPath,
  meshedPartnerPhaseRad,
  meshingCenterDistance,
} from './dialGeometry'
import './mechanical.css'

type Props = {
  /** 当前激活的设置分组索引（01～08 对应 0～7） */
  activeIndex: number
}

const DIAL_CX = 215
const DIAL_CY = 145
const MODULE = 3.8
const TEETH_A = 28
const TEETH_B = 16
const TEETH_C = 20
const RATCHET_STEP_DEG = 30
const BASE_IDLE_DURATION_SEC = 180

const ANGLE_B_DEG = -124
const ANGLE_C_DEG = -12

const round = (value: number) => Number(value.toFixed(2))
const rad = (deg: number) => (deg * Math.PI) / 180

/**
 * 设置页跨层景深暗纹齿轮蓝图（380×340 SVG，z-index: 0）：
 * - 外圈以 (215, 145) 为圆心的 r=155 / r=138 / r=112 同心刻度环（每 2° 短刻度 / 15° 长刻度 + 等宽角度标注）；
 * - 三齿轮严格按 `module * (Za + Zb) / 2` 啮合（A=28t 6辐主轮、B=16t 左上从动轮、C=20t 右侧从动轮）；
 * - 随左侧 CONTROL/08 分组索引 `activeIndex` 做 650ms 阻尼棘轮步进，并实时响应「05 界面与动效」三档设置。
 */
export function SettingsGearBackdrop({ activeIndex }: Props) {
  const [motion, setMotion] = useState<MotionPreference>(readMotion)

  useEffect(() => {
    const syncMotion = () => setMotion(readMotion())
    const onMotion = (event: Event) => {
      const detail = (event as CustomEvent<MotionPreference>).detail
      setMotion(detail ?? readMotion())
    }
    window.addEventListener('knowflow:motion', onMotion)
    window.addEventListener('storage', syncMotion)
    return () => {
      window.removeEventListener('knowflow:motion', onMotion)
      window.removeEventListener('storage', syncMotion)
    }
  }, [])

  const blueprint = useMemo(() => {
    const outerTicks = Array.from({ length: 180 }, (_, idx) => {
      const deg = idx * 2
      const isMajor = deg % 15 === 0
      const rOut = 138
      const rIn = isMajor ? 128 : 134
      const angle = rad(deg - 90)
      return {
        x1: round(DIAL_CX + Math.cos(angle) * rIn),
        y1: round(DIAL_CY + Math.sin(angle) * rIn),
        x2: round(DIAL_CX + Math.cos(angle) * rOut),
        y2: round(DIAL_CY + Math.sin(angle) * rOut),
        major: isMajor,
      }
    })

    const outerLabels = generateAngleLabels(DIAL_CX, DIAL_CY, 147, 30)

    const pitchA = (MODULE * TEETH_A) / 2
    const pitchB = (MODULE * TEETH_B) / 2
    const pitchC = (MODULE * TEETH_C) / 2

    const distAB = meshingCenterDistance(TEETH_A, TEETH_B, MODULE)
    const angleABRad = rad(ANGLE_B_DEG)
    const cxB = round(DIAL_CX + Math.cos(angleABRad) * distAB)
    const cyB = round(DIAL_CY + Math.sin(angleABRad) * distAB)
    const phaseBDeg = round((meshedPartnerPhaseRad(angleABRad, TEETH_A, TEETH_B, 0) * 180) / Math.PI)

    const distAC = meshingCenterDistance(TEETH_A, TEETH_C, MODULE)
    const angleACRad = rad(ANGLE_C_DEG)
    const cxC = round(DIAL_CX + Math.cos(angleACRad) * distAC)
    const cyC = round(DIAL_CY + Math.sin(angleACRad) * distAC)
    const phaseCDeg = round((meshedPartnerPhaseRad(angleACRad, TEETH_A, TEETH_C, 0) * 180) / Math.PI)

    const gears = [
      {
        id: 'a' as const,
        cx: DIAL_CX,
        cy: DIAL_CY,
        pitchRadius: pitchA,
        guideRadius: round(pitchA + MODULE * 0.85),
        chamferRadius: 42.6,
        hubRingRadius: 17.8,
        direction: 'cw' as const,
        idleDurationSec: BASE_IDLE_DURATION_SEC,
        ratio: 1,
        path: generateGearPath({
          teeth: TEETH_A,
          module: MODULE,
          innerRadius: 15.5,
          spokeCount: 6,
          cx: DIAL_CX,
          cy: DIAL_CY,
        }),
      },
      {
        id: 'b' as const,
        cx: cxB,
        cy: cyB,
        pitchRadius: pitchB,
        guideRadius: round(pitchB + MODULE * 0.85),
        chamferRadius: 20.0,
        hubRingRadius: 11.2,
        direction: 'ccw' as const,
        idleDurationSec: round(BASE_IDLE_DURATION_SEC * (TEETH_B / TEETH_A)),
        ratio: -(TEETH_A / TEETH_B),
        path: generateGearPath({
          teeth: TEETH_B,
          module: MODULE,
          innerRadius: 9.5,
          spokeCount: 5,
          cx: cxB,
          cy: cyB,
          rotationDeg: phaseBDeg,
        }),
      },
      {
        id: 'c' as const,
        cx: cxC,
        cy: cyC,
        pitchRadius: pitchC,
        guideRadius: round(pitchC + MODULE * 0.85),
        chamferRadius: 27.4,
        hubRingRadius: 13.4,
        direction: 'ccw' as const,
        idleDurationSec: round(BASE_IDLE_DURATION_SEC * (TEETH_C / TEETH_A)),
        ratio: -(TEETH_A / TEETH_C),
        path: generateGearPath({
          teeth: TEETH_C,
          module: MODULE,
          innerRadius: 11.5,
          spokeCount: 5,
          cx: cxC,
          cy: cyC,
          rotationDeg: phaseCDeg,
        }),
      },
    ]

    return { outerTicks, outerLabels, gears }
  }, [])

  const safeIndex = Math.max(0, activeIndex)

  return (
    <div
      className="settings-gear-backdrop"
      data-motion={motion}
      data-section-index={safeIndex}
      aria-hidden="true"
    >
      <svg viewBox="0 0 380 340" fill="none">
        {/* 外圈精密表盘刻度环（r=155 / r=138 / r=112）与正交准星 */}
        <g stroke="rgba(255, 255, 255, 0.16)" strokeWidth="0.8">
          <circle cx={DIAL_CX} cy={DIAL_CY} r="155" />
          <circle cx={DIAL_CX} cy={DIAL_CY} r="138" opacity="0.85" />
          <circle cx={DIAL_CX} cy={DIAL_CY} r="112" opacity="0.62" />
          <path d={`M44 ${DIAL_CY}h328M${DIAL_CX} 0v318`} strokeDasharray="2 5" opacity="0.38" />

          {blueprint.outerTicks.map((tick, index) => (
            <line
              key={index}
              x1={tick.x1}
              y1={tick.y1}
              x2={tick.x2}
              y2={tick.y2}
              strokeWidth={tick.major ? 0.85 : 0.6}
              opacity={tick.major ? 0.88 : 0.48}
            />
          ))}
        </g>

        <g
          fill="#FFFFFF"
          fontFamily="var(--font-mono), monospace"
          fontSize="8"
          opacity="0.25"
          textAnchor="middle"
          dominantBaseline="middle"
        >
          {blueprint.outerLabels.map(label => (
            <text
              key={label.deg}
              x={label.x}
              y={label.y}
              transform={`rotate(${label.textRotation} ${label.x} ${label.y})`}
            >
              {label.deg}°
            </text>
          ))}
        </g>

        {/* 三齿轮机械棘轮步进 + 待机微转层 */}
        {blueprint.gears.map(gear => {
          const ratchetDeg = round(safeIndex * RATCHET_STEP_DEG * gear.ratio)
          return (
            <g
              key={gear.id}
              className={`settings-gear-ratchet settings-gear-ratchet--${gear.id}`}
              style={{ '--ratchet-angle': `${ratchetDeg}deg` } as CSSProperties}
            >
              <g
                className={`settings-gear-idle settings-gear-idle--${gear.id} settings-gear-idle--${gear.direction}`}
                style={{ animationDuration: `${gear.idleDurationSec}s` }}
              >
                {/* 对称透明导引圆确保 fill-box center 旋转与步进无任何偏心抖动 */}
                <circle
                  cx={gear.cx}
                  cy={gear.cy}
                  r={gear.guideRadius}
                  fill="none"
                  stroke="rgba(255,255,255,0.01)"
                  strokeWidth="0.1"
                />
                <path
                  d={gear.path}
                  fill="none"
                  stroke="rgba(255, 255, 255, 0.16)"
                  strokeWidth="0.8"
                  strokeLinejoin="round"
                />
                <circle
                  cx={gear.cx}
                  cy={gear.cy}
                  r={gear.chamferRadius}
                  fill="none"
                  stroke="rgba(255, 255, 255, 0.10)"
                  strokeWidth="0.55"
                />
                <circle
                  cx={gear.cx}
                  cy={gear.cy}
                  r={gear.hubRingRadius}
                  fill="none"
                  stroke="rgba(255, 255, 255, 0.12)"
                  strokeWidth="0.6"
                />
                <circle
                  cx={gear.cx}
                  cy={gear.cy}
                  r={gear.pitchRadius}
                  fill="none"
                  stroke="rgba(255, 255, 255, 0.16)"
                  strokeWidth="0.65"
                  strokeDasharray="2 3"
                  opacity="0.58"
                />
              </g>
            </g>
          )
        })}
      </svg>
    </div>
  )
}
