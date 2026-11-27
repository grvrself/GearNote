import { useMemo } from 'react'
import {
  generateAngleLabels,
  generateGearPath,
  generateVernierSubTicks,
  meshedPartnerPhaseRad,
  meshingCenterDistance,
} from './dialGeometry'
import { useThemeAndMotion } from './useThemeAndMotion'
import './mechanical.css'

type Props = {
  /** Agent 是否已配置可用 */
  ready: boolean
  /** 当前是否处于思考/流式响应态 */
  active?: boolean
}

const DIAL_CX = 166
const DIAL_CY = 90
const MODULE = 3.2
const TEETH_A = 26
const TEETH_B = 16
const MESH_ANGLE_DEG = -40
const MESH_ANGLE_RAD = (MESH_ANGLE_DEG * Math.PI) / 180
const BASE_DURATION_SEC = 24

/**
 * Agent 对话页眉右上角精密状态区：
 * 1. 底层（z-index: 1）：320×150 纯 SVG 暗纹啮合齿轮蓝图（26 齿主轮 + 16 齿副轮），
 *    带 r=115 / r=92 游标刻度弧与 3°/15° 分度及左侧渐隐遮罩；
 * 2. 前景（z-index: 2）：6×6px 硬件 LED 状态灯 + 文案 + 108×12px 微型游标刻度尺与扫描指针。
 */
export function AgentHeaderStatus({ ready, active = false }: Props) {
  const { theme } = useThemeAndMotion()
  const blueprint = useMemo(() => {
    const centerDist = meshingCenterDistance(TEETH_A, TEETH_B, MODULE)
    const cxB = Number((DIAL_CX + Math.cos(MESH_ANGLE_RAD) * centerDist).toFixed(2))
    const cyB = Number((DIAL_CY + Math.sin(MESH_ANGLE_RAD) * centerDist).toFixed(2))
    const phaseBDeg = (meshedPartnerPhaseRad(MESH_ANGLE_RAD, TEETH_A, TEETH_B, 0) * 180) / Math.PI

    const pitchRadiusA = (MODULE * TEETH_A) / 2
    const pitchRadiusB = (MODULE * TEETH_B) / 2

    const gearA = {
      id: 'a' as const,
      cx: DIAL_CX,
      cy: DIAL_CY,
      pitchRadius: pitchRadiusA,
      guideRadius: pitchRadiusA + MODULE * 0.85,
      hubRingRadius: 13.4,
      durationSec: BASE_DURATION_SEC,
      path: generateGearPath({
        teeth: TEETH_A,
        module: MODULE,
        innerRadius: 11.5,
        spokeCount: 5,
        cx: DIAL_CX,
        cy: DIAL_CY,
      }),
    }

    const gearB = {
      id: 'b' as const,
      cx: cxB,
      cy: cyB,
      pitchRadius: pitchRadiusB,
      guideRadius: pitchRadiusB + MODULE * 0.85,
      hubRingRadius: 8.6,
      durationSec: Number((BASE_DURATION_SEC * (TEETH_B / TEETH_A)).toFixed(2)),
      path: generateGearPath({
        teeth: TEETH_B,
        module: MODULE,
        innerRadius: 7.2,
        spokeCount: 5,
        cx: cxB,
        cy: cyB,
        rotationDeg: phaseBDeg,
      }),
    }

    const arcTicks = generateVernierSubTicks(DIAL_CX, DIAL_CY, 115)
    const arcLabels = generateAngleLabels(DIAL_CX, DIAL_CY, 101, 30)
    const caliperTicks = Array.from({ length: 26 }, (_, idx) => ({
      x: 4 + idx * 4,
      major: idx % 5 === 0,
    }))

    return { gearA, gearB, arcTicks, arcLabels, caliperTicks }
  }, [])

  const statusText = active ? 'Agent 思考中...' : ready ? 'Agent 已就绪' : 'Agent 未配置'

  return (
    <>
      {theme !== 'mono' && (
        <div
          className="agent-header-blueprint"
          data-active={active}
          aria-hidden="true"
        >
        <svg viewBox="0 0 320 150" fill="none">
          {/* 外圈半露游标刻度弧（r=115 / r=92）与 3°/15° 分度 */}
          <g stroke="rgba(255, 255, 255, 0.20)" strokeWidth="0.75">
            <circle cx={DIAL_CX} cy={DIAL_CY} r="115" />
            <circle cx={DIAL_CX} cy={DIAL_CY} r="92" opacity="0.78" />
            <path d={`M51 ${DIAL_CY}h230M${DIAL_CX} -15v165`} strokeDasharray="2 4" opacity="0.42" />

            {blueprint.arcTicks.map((tick, index) => (
              <line
                key={index}
                x1={tick.x1}
                y1={tick.y1}
                x2={tick.x2}
                y2={tick.y2}
                strokeWidth={tick.level === 'minor' ? 0.65 : 0.85}
                opacity={tick.level === 'minor' ? 0.52 : 0.92}
              />
            ))}
          </g>

          <g
            fill="#FFFFFF"
            fontFamily="var(--font-mono), monospace"
            fontSize="7"
            opacity="0.25"
            textAnchor="middle"
            dominantBaseline="middle"
          >
            {blueprint.arcLabels.map(label => (
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

          {/* 两组严格按模数齿数比真实咬合的线框齿轮 */}
          {[blueprint.gearA, blueprint.gearB].map(gear => (
            <g
              key={gear.id}
              className={`agent-blueprint-gear agent-blueprint-gear--${gear.id}`}
              style={{ animationDuration: `${gear.durationSec}s` }}
            >
              {/* 对称透明导引圆确保 fill-box center 零抖动 */}
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
                stroke="rgba(255, 255, 255, 0.20)"
                strokeWidth="0.75"
                strokeLinejoin="round"
              />
              <circle
                cx={gear.cx}
                cy={gear.cy}
                r={gear.hubRingRadius}
                fill="none"
                stroke="rgba(255, 255, 255, 0.14)"
                strokeWidth="0.55"
              />
              <circle
                cx={gear.cx}
                cy={gear.cy}
                r={gear.pitchRadius}
                fill="none"
                stroke="rgba(255, 255, 255, 0.20)"
                strokeWidth="0.6"
                strokeDasharray="2 3"
                opacity="0.55"
              />
            </g>
          ))}
        </svg>
        </div>
      )}

      <div className="agent-header-telemetry" data-active={active}>
        <span className={`chat-agent-status${ready ? ' is-ready' : ''}`}>
          <i className={`gn-led ${ready ? 'gn-led--online' : 'gn-led--idle'}`} aria-hidden="true" />
          <span>{statusText}</span>
        </span>
        <div className="agent-vernier-ruler" aria-hidden="true">
          <svg viewBox="0 0 108 12" width="108" height="12" fill="none">
            <line x1="0" y1="10.5" x2="108" y2="10.5" stroke="rgba(255,255,255,0.16)" strokeWidth="1" />
            {blueprint.caliperTicks.map(tick => (
              <line
                key={tick.x}
                x1={tick.x}
                y1={tick.major ? 2.5 : 6.5}
                x2={tick.x}
                y2="10.5"
                stroke={tick.major ? 'rgba(255,255,255,0.32)' : 'rgba(255,255,255,0.15)'}
                strokeWidth="1"
              />
            ))}
          </svg>
          <span className="agent-vernier-pointer" />
        </div>
      </div>
    </>
  )
}
