import { useId, useMemo } from 'react'
import {
  generateGearPath,
  generateOuterTicks,
  generateVernierSubTicks,
  meshedPartnerPhaseRad,
  meshingCenterDistance,
} from './dialGeometry'
import './mechanical.css'

type Props = {
  active?: boolean
  className?: string
}

const rad = (deg: number) => (deg * Math.PI) / 180

/**
 * 页眉工程级制表机芯切片（viewBox 640×160）：
 * 以共轴中心轮 (395, 92) 统领外圈 240 分度与内圈 120 游标密刻度，
 * 搭配 m=2.4 超细微齿 40/54/32/28 齿 4 轮精密啮合、0.55px 工程线宽与多层倒角环，
 * 呈现在上下边界裁切出血、左右柔和融入黑底的高精密蓝图效果。
 */
export function GearWatermark({ active = false, className = '' }: Props) {
  const rawId = useId()
  const scopeId = rawId.replace(/:/g, '')
  const haloGradId = `${scopeId}-wm-halo`
  const strokeGradId = `${scopeId}-wm-stroke`
  const fillGradId = `${scopeId}-wm-fill`

  const { outerTicks, innerTicks, gears } = useMemo(() => {
    const dialCx = 395
    const dialCy = 92
    const module = 2.4

    const teethCenter = 40
    const teethTopLeft = 54
    const teethBottomLeft = 32
    const teethRight = 28

    const placeMeshed = (angleDeg: number, partnerTeeth: number, innerRadius: number, spokeCount: number, id: string, baseDuration: number) => {
      const aRad = rad(angleDeg)
      const dist = meshingCenterDistance(teethCenter, partnerTeeth, module)
      const cx = Number((dialCx + Math.cos(aRad) * dist).toFixed(2))
      const cy = Number((dialCy + Math.sin(aRad) * dist).toFixed(2))
      const phase = (meshedPartnerPhaseRad(aRad, teethCenter, partnerTeeth, 0) * 180) / Math.PI
      const pitchRadius = (module * partnerTeeth) / 2
      const rootRadius = pitchRadius - module * 1.05
      const rimInner = Math.max(innerRadius + 2.5, rootRadius - Math.max(4.4, module * 1.15))
      return {
        id,
        cx,
        cy,
        guideRadius: pitchRadius + module * 0.85,
        pitchRadius,
        chamferRadius: Number((rimInner - 2.2).toFixed(2)),
        hubRingRadius: Number((innerRadius * 1.16).toFixed(2)),
        duration: Number((baseDuration * (partnerTeeth / teethCenter)).toFixed(2)),
        path: generateGearPath({ teeth: partnerTeeth, module, innerRadius, spokeCount, cx, cy, rotationDeg: phase }),
      }
    }

    const centerPitch = (module * teethCenter) / 2
    const centerRoot = centerPitch - module * 1.05
    const centerRimInner = Math.max(18.5, centerRoot - Math.max(4.4, module * 1.15))
    const centerGear = {
      id: 'center',
      cx: dialCx,
      cy: dialCy,
      guideRadius: centerPitch + module * 0.85,
      pitchRadius: centerPitch,
      chamferRadius: Number((centerRimInner - 2.2).toFixed(2)),
      hubRingRadius: 18.5,
      duration: 60,
      path: generateGearPath({ teeth: teethCenter, module, innerRadius: 16, spokeCount: 6, cx: dialCx, cy: dialCy }),
    }

    return {
      outerTicks: generateOuterTicks(dialCx, dialCy, 224, 1.5),
      innerTicks: generateVernierSubTicks(dialCx, dialCy, 148),
      gears: [
        centerGear,
        placeMeshed(-142, teethTopLeft, 22, 6, 'top-left', 60),
        placeMeshed(148, teethBottomLeft, 13, 5, 'bottom-left', 60),
        placeMeshed(-12, teethRight, 11, 5, 'right', 60),
      ],
    }
  }, [])

  return (
    <div className={`gn-watermark ${className}`.trim()} data-active={active} aria-hidden="true">
      <svg viewBox="0 0 640 160" preserveAspectRatio="xMidYMid slice" fill="none">
        <defs>
          <radialGradient id={haloGradId} cx="62%" cy="52%" r="42%">
            <stop offset="0%" stopColor="rgba(255,255,255,0.10)" />
            <stop offset="52%" stopColor="rgba(255,255,255,0.035)" />
            <stop offset="100%" stopColor="rgba(255,255,255,0)" />
          </radialGradient>
          <linearGradient id={strokeGradId} x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stopColor="rgba(255,255,255,0.52)" />
            <stop offset="52%" stopColor="rgba(255,255,255,0.28)" />
            <stop offset="100%" stopColor="rgba(255,255,255,0.14)" />
          </linearGradient>
          <linearGradient id={fillGradId} x1="0" y1="0" x2="0" y2="160" gradientUnits="userSpaceOnUse">
            <stop offset="0%" stopColor="rgba(255,255,255,0.05)" />
            <stop offset="100%" stopColor="rgba(255,255,255,0.015)" />
          </linearGradient>
        </defs>

        {/* 共轴区域柔和背光 */}
        <circle cx="395" cy="92" r="206" fill={`url(#${haloGradId})`} />

        {/* 巨幅多轨同心游标环 + 240 外刻度 + 120 内刻度 */}
        <g stroke="currentColor" strokeWidth="0.55">
          <circle cx="395" cy="92" r="236" opacity="0.22" />
          <circle cx="395" cy="92" r="224" stroke={`url(#${strokeGradId})`} strokeWidth="0.8" />
          <circle cx="395" cy="92" r="210" opacity="0.34" />
          <circle cx="395" cy="92" r="192" opacity="0.22" strokeDasharray="1.5 5" />
          <circle cx="395" cy="92" r="170" opacity="0.32" />
          <circle cx="395" cy="92" r="148" stroke={`url(#${strokeGradId})`} strokeWidth="0.7" />
          <circle cx="395" cy="92" r="134" opacity="0.24" />
          <circle cx="395" cy="92" r="106" opacity="0.20" strokeDasharray="2 4" />

          {outerTicks.map((tick, index) => (
            <line
              key={`wo-${index}`}
              x1={tick.x1}
              y1={tick.y1}
              x2={tick.x2}
              y2={tick.y2}
              opacity={tick.level === 'major' ? 0.72 : tick.level === 'medium' ? 0.45 : 0.22}
              strokeWidth={tick.level === 'major' ? 0.75 : 0.5}
            />
          ))}
          {innerTicks.map((tick, index) => (
            <line
              key={`wi-${index}`}
              x1={tick.x1}
              y1={tick.y1}
              x2={tick.x2}
              y2={tick.y2}
              opacity={tick.level === 'major' ? 0.54 : tick.level === 'medium' ? 0.32 : 0.16}
              strokeWidth={tick.level === 'major' ? 0.65 : 0.45}
            />
          ))}
        </g>

        {/* 共轴正交工程准星、靶心同心圆与高光星点 */}
        <g stroke="rgba(255,255,255,0.28)" strokeWidth="0.6">
          <path d="M96 92h512M395 -40v264" strokeDasharray="3 4" />
          <circle cx="395" cy="92" r="24" opacity="0.45" />
          <circle cx="395" cy="92" r="14" strokeDasharray="2 2" opacity="0.62" />
          <circle cx="395" cy="92" r="2" fill="rgba(255,255,255,0.85)" stroke="none" />
          <circle cx="218" cy="92" r="6" opacity="0.52" />
          <circle cx="218" cy="92" r="1.4" fill="rgba(255,255,255,0.75)" stroke="none" />
        </g>

        {/* 4 枚超细微齿精密镂空齿轮（共轴中心轮 + 三向啮合轮系） */}
        {gears.map(gear => (
          <g
            key={gear.id}
            className={`gn-watermark-gear gn-watermark-gear--${gear.id}`}
            style={{ animationDuration: `${gear.duration}s` }}
          >
            <circle cx={gear.cx} cy={gear.cy} r={gear.guideRadius} stroke="rgba(255,255,255,0.01)" strokeWidth="0.1" />
            <path
              d={gear.path}
              fill={`url(#${fillGradId})`}
              fillRule="evenodd"
              stroke={`url(#${strokeGradId})`}
              strokeWidth="0.6"
              strokeLinejoin="round"
            />
            <circle cx={gear.cx} cy={gear.cy} r={gear.chamferRadius} stroke="rgba(255,255,255,0.24)" strokeWidth="0.45" />
            <circle cx={gear.cx} cy={gear.cy} r={gear.hubRingRadius} stroke="rgba(255,255,255,0.20)" strokeWidth="0.45" strokeDasharray="1.5 2.5" />
            <circle cx={gear.cx} cy={gear.cy} r={gear.pitchRadius} stroke="rgba(255,255,255,0.48)" strokeWidth="0.5" strokeDasharray="2 3" opacity="0.32" />
          </g>
        ))}
      </svg>
    </div>
  )
}
