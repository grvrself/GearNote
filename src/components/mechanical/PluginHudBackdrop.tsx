import { useMemo } from 'react'
import { generateGearPath, meshedPartnerPhaseRad, meshingCenterDistance } from './dialGeometry'
import './mechanical.css'

type Props = {
  /** 是否处于活动或扫描工作态 */
  active?: boolean
}

const DIAL_CX = 212
const DIAL_CY = 58
const MODULE = 3.2
const TEETH_A = 26
const TEETH_B = 18
const TEETH_C = 14
const BASE_DURATION_SEC = 48

const round = (value: number) => Number(value.toFixed(2))
const rad = (deg: number) => (deg * Math.PI) / 180

/**
 * HUD 主题「连接与插件」页眉右上角金色暗纹啮合齿轮蓝图（360×118 SVG）：
 * 严格限定 `body[data-theme='hud']` 可见，黑白 `mono` 主题下自动隐藏。
 */
export function PluginHudBackdrop({ active = false }: Props) {
  const gears = useMemo(() => {
    const pitchA = (MODULE * TEETH_A) / 2
    const pitchB = (MODULE * TEETH_B) / 2
    const pitchC = (MODULE * TEETH_C) / 2

    const distAB = meshingCenterDistance(TEETH_A, TEETH_B, MODULE)
    const angleAB = rad(-156)
    const cxB = round(DIAL_CX + Math.cos(angleAB) * distAB)
    const cyB = round(DIAL_CY + Math.sin(angleAB) * distAB)
    const phaseB = round((meshedPartnerPhaseRad(angleAB, TEETH_A, TEETH_B, 0) * 180) / Math.PI)

    const distAC = meshingCenterDistance(TEETH_A, TEETH_C, MODULE)
    const angleAC = rad(18)
    const cxC = round(DIAL_CX + Math.cos(angleAC) * distAC)
    const cyC = round(DIAL_CY + Math.sin(angleAC) * distAC)
    const phaseC = round((meshedPartnerPhaseRad(angleAC, TEETH_A, TEETH_C, 0) * 180) / Math.PI)

    return [
      {
        id: 'a' as const,
        cx: DIAL_CX,
        cy: DIAL_CY,
        pitchRadius: pitchA,
        guideRadius: round(pitchA + MODULE * 0.85),
        hubRingRadius: 15.2,
        durationSec: BASE_DURATION_SEC,
        direction: 'cw' as const,
        path: generateGearPath({
          teeth: TEETH_A,
          module: MODULE,
          innerRadius: 13.5,
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
        hubRingRadius: 10.6,
        durationSec: round(BASE_DURATION_SEC * (TEETH_B / TEETH_A)),
        direction: 'ccw' as const,
        path: generateGearPath({
          teeth: TEETH_B,
          module: MODULE,
          innerRadius: 9.2,
          spokeCount: 5,
          cx: cxB,
          cy: cyB,
          rotationDeg: phaseB,
        }),
      },
      {
        id: 'c' as const,
        cx: cxC,
        cy: cyC,
        pitchRadius: pitchC,
        guideRadius: round(pitchC + MODULE * 0.85),
        hubRingRadius: 8.6,
        durationSec: round(BASE_DURATION_SEC * (TEETH_C / TEETH_A)),
        direction: 'ccw' as const,
        path: generateGearPath({
          teeth: TEETH_C,
          module: MODULE,
          innerRadius: 7.4,
          spokeCount: 5,
          cx: cxC,
          cy: cyC,
          rotationDeg: phaseC,
        }),
      },
    ]
  }, [])

  return (
    <div className="plugin-hud-backdrop" data-active={active} aria-hidden="true">
      <svg viewBox="0 0 360 118" fill="none">
        <g stroke="rgba(252, 238, 10, 0.22)" strokeWidth="0.75">
          <circle cx={DIAL_CX} cy={DIAL_CY} r="94" opacity="0.5" />
          <circle cx={DIAL_CX} cy={DIAL_CY} r="76" strokeDasharray="2 4" opacity="0.55" />
          <path d={`M64 ${DIAL_CY}h266M${DIAL_CX} -12v136`} strokeDasharray="2 5" opacity="0.35" />
        </g>

        {gears.map(gear => (
          <g
            key={gear.id}
            className={`plugin-hud-gear plugin-hud-gear--${gear.direction}`}
            style={{ animationDuration: `${gear.durationSec}s` }}
          >
            <circle
              cx={gear.cx}
              cy={gear.cy}
              r={gear.guideRadius}
              fill="none"
              stroke="rgba(252, 238, 10, 0.01)"
              strokeWidth="0.1"
            />
            <path
              d={gear.path}
              fill="none"
              stroke="rgba(252, 238, 10, 0.22)"
              strokeWidth="0.85"
              strokeLinejoin="round"
            />
            <circle
              cx={gear.cx}
              cy={gear.cy}
              r={gear.hubRingRadius}
              fill="none"
              stroke="rgba(252, 238, 10, 0.16)"
              strokeWidth="0.6"
            />
            <circle
              cx={gear.cx}
              cy={gear.cy}
              r={gear.pitchRadius}
              fill="none"
              stroke="rgba(252, 238, 10, 0.22)"
              strokeWidth="0.6"
              strokeDasharray="2 3"
              opacity="0.58"
            />
          </g>
        ))}
      </svg>
    </div>
  )
}
