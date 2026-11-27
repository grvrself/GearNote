import { useId, useMemo } from 'react'
import { CrosshairFrame } from './CrosshairFrame'
import {
  computeMeshedGearLayout,
  generateAmbientDust,
  generateAngleLabels,
  generateGearPath,
  generateOuterTicks,
  generateVernierSubTicks,
} from './dialGeometry'
import './mechanical.css'

type Props = {
  /** 是否处于生成工作态 */
  active?: boolean
  /** 悬停/聚焦“生成高级版 →”等主触发按钮时，提升表镜顶部高光 */
  highlighted?: boolean
  className?: string
}

/**
 * 精密机械表盘舞台（5 层程序化 SVG + CSS）：
 * 1. 点阵底纹、光学微粒与四角准星；
 * 2. 640×640 外圈多轨道同心游标刻度环；
 * 3. 内圈 r=182 蓝宝石表镜立体光晕、斜向玻璃反光与顶部打光；
 * 4. 上半区 r=180 裁切内真实齿数比啮合的三齿轮 3D 金属组；
 * 5. 从 48% 高度平滑渐隐的下半区文字可读性遮罩。
 */
export function HeroDialStage({ active = false, highlighted = false, className = '' }: Props) {
  const rawId = useId()
  const scopeId = rawId.replace(/:/g, '')
  const bezelGradId = `${scopeId}-bezel-rim`
  const crystalRimGradId = `${scopeId}-crystal-rim`
  const crystalOuterHaloId = `${scopeId}-crystal-halo`
  const crystalSpotlightId = `${scopeId}-crystal-spot`
  const crystalSideGlareId = `${scopeId}-crystal-side`
  const streakGradId = `${scopeId}-streak-grad`
  const gearFillGradId = `${scopeId}-gear-fill`
  const gearStrokeGradId = `${scopeId}-gear-stroke`
  const lowerFadeGradId = `${scopeId}-lower-fade-grad`
  const lowerFadeMaskId = `${scopeId}-lower-fade-mask`
  const crystalClipId = `${scopeId}-crystal-clip`
  const crystalGlowFilterId = `${scopeId}-crystal-glow`

  const geometry = useMemo(() => {
    const outerTicks = generateOuterTicks(320, 320, 294)
    const innerTicks = generateVernierSubTicks(320, 320, 212)
    const outerLabels = generateAngleLabels(320, 320, 266, 15)
    const innerLabels = generateAngleLabels(320, 320, 224, 30)
    const dust = generateAmbientDust(320, 320, 48)
    const gears = computeMeshedGearLayout().map(gear => {
      const boltRadius = (gear.hubRadius + gear.innerRadius * 0.34) * 0.56
      const bolts = Array.from({ length: gear.spokeCount }, (_, idx) => {
        const angle = ((idx + 0.5) / gear.spokeCount) * Math.PI * 2 + (gear.rotationDeg * Math.PI) / 180
        return {
          x: Number((gear.cx + Math.cos(angle) * boltRadius).toFixed(2)),
          y: Number((gear.cy + Math.sin(angle) * boltRadius).toFixed(2)),
        }
      })
      return {
        ...gear,
        path: generateGearPath(gear),
        pitchGuideRadius: gear.pitchRadius + gear.module * 0.85,
        bolts,
      }
    })
    return { outerTicks, innerTicks, outerLabels, innerLabels, dust, gears }
  }, [])

  return (
    <div
      className={`hero-dial-stage gn-dot-grid ${className}`.trim()}
      data-active={active}
      data-highlighted={highlighted}
      aria-hidden="true"
    >
      <div className="hero-dial-crosshairs">
        <CrosshairFrame className="hero-dial-crosshair-frame" />
      </div>

      <div className="hero-dial-svg-wrap">
        <svg className="hero-dial-svg" viewBox="0 0 640 640" fill="none">
          <defs>
            <filter id={crystalGlowFilterId} x="-25%" y="-25%" width="150%" height="150%">
              <feGaussianBlur stdDeviation="5.5" result="blur" />
              <feMerge>
                <feMergeNode in="blur" />
                <feMergeNode in="SourceGraphic" />
              </feMerge>
            </filter>

            <linearGradient id={bezelGradId} x1="0%" y1="0%" x2="0%" y2="100%">
              <stop offset="0%" stopColor="rgba(255,255,255,0.48)" />
              <stop offset="48%" stopColor="rgba(255,255,255,0.20)" />
              <stop offset="100%" stopColor="rgba(255,255,255,0.08)" />
            </linearGradient>

            <radialGradient id={crystalOuterHaloId} cx="50%" cy="32%" r="60%">
              <stop offset="52%" stopColor="rgba(255,255,255,0.12)" />
              <stop offset="78%" stopColor="rgba(255,255,255,0.04)" />
              <stop offset="100%" stopColor="rgba(255,255,255,0)" />
            </radialGradient>

            <linearGradient id={crystalRimGradId} x1="0%" y1="0%" x2="0%" y2="100%">
              <stop offset="0%" stopColor="rgba(255,255,255,0.88)" />
              <stop offset="32%" stopColor="rgba(255,255,255,0.46)" />
              <stop offset="68%" stopColor="rgba(255,255,255,0.12)" />
              <stop offset="100%" stopColor="rgba(255,255,255,0.04)" />
            </linearGradient>

            <radialGradient id={crystalSpotlightId} cx="46%" cy="18%" r="68%" fx="46%" fy="14%">
              <stop offset="0%" stopColor="rgba(255,255,255,0.22)" />
              <stop offset="42%" stopColor="rgba(255,255,255,0.09)" />
              <stop offset="76%" stopColor="rgba(255,255,255,0.025)" />
              <stop offset="100%" stopColor="rgba(255,255,255,0)" />
            </radialGradient>

            <linearGradient id={crystalSideGlareId} x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="rgba(255,255,255,0.24)" />
              <stop offset="45%" stopColor="rgba(255,255,255,0.06)" />
              <stop offset="100%" stopColor="rgba(255,255,255,0)" />
            </linearGradient>

            <linearGradient id={streakGradId} x1="0%" y1="0%" x2="100%" y2="38%">
              <stop offset="0%" stopColor="rgba(255,255,255,0)" />
              <stop offset="45%" stopColor="rgba(255,255,255,0.30)" />
              <stop offset="72%" stopColor="rgba(255,255,255,0.62)" />
              <stop offset="100%" stopColor="rgba(255,255,255,0)" />
            </linearGradient>

            <linearGradient id={gearFillGradId} x1="0" y1="140" x2="0" y2="380" gradientUnits="userSpaceOnUse">
              <stop offset="0%" stopColor="rgba(255,255,255,0.085)" />
              <stop offset="55%" stopColor="rgba(255,255,255,0.032)" />
              <stop offset="100%" stopColor="rgba(255,255,255,0.008)" />
            </linearGradient>

            <linearGradient id={gearStrokeGradId} x1="0" y1="140" x2="0" y2="380" gradientUnits="userSpaceOnUse">
              <stop offset="0%" stopColor="rgba(255,255,255,0.72)" />
              <stop offset="55%" stopColor="rgba(255,255,255,0.34)" />
              <stop offset="100%" stopColor="rgba(255,255,255,0.10)" />
            </linearGradient>

            {/* 从 48% 高度柔和淡出，彻底隔离齿轮线条干扰居中的大字标题 */}
            <linearGradient id={lowerFadeGradId} x1="0" y1="0" x2="0" y2="640" gradientUnits="userSpaceOnUse">
              <stop offset="0%" stopColor="#FFFFFF" stopOpacity="1" />
              <stop offset="47%" stopColor="#FFFFFF" stopOpacity="0.98" />
              <stop offset="56%" stopColor="#FFFFFF" stopOpacity="0.24" />
              <stop offset="65%" stopColor="#FFFFFF" stopOpacity="0.04" />
              <stop offset="100%" stopColor="#FFFFFF" stopOpacity="0" />
            </linearGradient>

            <mask id={lowerFadeMaskId} maskUnits="userSpaceOnUse" x="0" y="0" width="640" height="640">
              <rect x="0" y="0" width="640" height="640" fill={`url(#${lowerFadeGradId})`} />
            </mask>

            <clipPath id={crystalClipId}>
              <circle cx="320" cy="320" r="180" />
            </clipPath>
          </defs>

          {/* 层 1b：表盘周缘光学微粒 */}
          <g className="hero-dial-dust">
            {geometry.dust.map((particle, idx) => (
              <circle
                key={idx}
                cx={particle.x}
                cy={particle.y}
                r={particle.r}
                fill="#FFFFFF"
                opacity={particle.opacity}
              />
            ))}
          </g>

          {/* 层 2：多轨道外圈同心游标刻度环 */}
          <g className="hero-dial-bezel-group">
            <g className="hero-dial-bezel-outer">
              <circle cx="320" cy="320" r="306" stroke="rgba(255,255,255,0.07)" strokeWidth="0.6" />
              <circle cx="320" cy="320" r="294" stroke={`url(#${bezelGradId})`} strokeWidth="0.95" />
              <circle cx="320" cy="320" r="279" stroke="rgba(255,255,255,0.10)" strokeWidth="0.6" strokeDasharray="1.5 6" />
              <circle cx="320" cy="320" r="252" stroke="rgba(255,255,255,0.18)" strokeWidth="0.75" />
              <circle cx="320" cy="320" r="236" stroke="rgba(255,255,255,0.11)" strokeWidth="0.65" />
              <circle cx="320" cy="320" r="212" stroke="rgba(255,255,255,0.20)" strokeWidth="0.8" />
              <circle cx="320" cy="320" r="198" stroke="rgba(255,255,255,0.12)" strokeWidth="0.65" />

              {/* 分度弧段装饰 */}
              <path
                d="M196 146 A215 215 0 0 1 444 146"
                stroke="rgba(255,255,255,0.26)"
                strokeWidth="1.1"
                strokeDasharray="18 6 4 6"
              />
              <path
                d="M110 264 A220 220 0 0 1 180 156"
                stroke="rgba(255,255,255,0.22)"
                strokeWidth="0.9"
              />

              {geometry.outerTicks.map((tick, index) => (
                <line
                  key={`o-${index}`}
                  x1={tick.x1}
                  y1={tick.y1}
                  x2={tick.x2}
                  y2={tick.y2}
                  stroke={
                    tick.level === 'major'
                      ? 'rgba(255,255,255,0.50)'
                      : tick.level === 'medium'
                        ? 'rgba(255,255,255,0.28)'
                        : 'rgba(255,255,255,0.15)'
                  }
                  strokeWidth={tick.level === 'major' ? 0.95 : 0.65}
                />
              ))}

              {geometry.innerTicks.map((tick, index) => (
                <line
                  key={`i-${index}`}
                  x1={tick.x1}
                  y1={tick.y1}
                  x2={tick.x2}
                  y2={tick.y2}
                  stroke={
                    tick.level === 'major'
                      ? 'rgba(255,255,255,0.34)'
                      : tick.level === 'medium'
                        ? 'rgba(255,255,255,0.20)'
                        : 'rgba(255,255,255,0.10)'
                  }
                  strokeWidth={tick.level === 'major' ? 0.8 : 0.55}
                />
              ))}
            </g>

            {/* 正交与 45° 对角刻度辅助线 */}
            <path
              d="M320 12v116M320 512v116M12 320h116M512 320h116"
              stroke="rgba(255,255,255,0.14)"
              strokeWidth="0.7"
              strokeDasharray="2 5"
            />
            <path
              d="M112 112l72 72M528 112l-72 72M112 528l72-72M528 528l-72-72"
              stroke="rgba(255,255,255,0.08)"
              strokeWidth="0.6"
              strokeDasharray="1.5 6"
            />

            <g className="hero-dial-labels" fill="rgba(255,255,255,0.40)" fontSize="9" textAnchor="middle" dominantBaseline="middle">
              {geometry.outerLabels.map(label => (
                <text
                  key={`ol-${label.deg}`}
                  x={label.x}
                  y={label.y}
                  transform={`rotate(${label.textRotation} ${label.x} ${label.y})`}
                >
                  {label.deg}
                </text>
              ))}
            </g>
            <g className="hero-dial-labels" fill="rgba(255,255,255,0.28)" fontSize="8" textAnchor="middle" dominantBaseline="middle">
              {geometry.innerLabels.map(label => (
                <text
                  key={`il-${label.deg}`}
                  x={label.x}
                  y={label.y}
                  transform={`rotate(${label.textRotation} ${label.x} ${label.y})`}
                >
                  {label.deg}
                </text>
              ))}
            </g>
          </g>

          {/* 表镜外部柔和发光光晕 */}
          <circle cx="320" cy="320" r="206" fill={`url(#${crystalOuterHaloId})`} />

          {/* 层 3 & 4 & 5：蓝宝石表镜腔体、上半区 3D 金属啮合齿轮组与下半区 48%–65% 可读性遮罩 */}
          <g mask={`url(#${lowerFadeMaskId})`}>
            {/* 表镜内部背光与左侧弧形反射区 */}
            <circle cx="320" cy="320" r="181" fill={`url(#${crystalSpotlightId})`} />
            <ellipse
              cx="236"
              cy="252"
              rx="84"
              ry="116"
              transform="rotate(-24 236 252)"
              fill={`url(#${crystalSideGlareId})`}
              opacity="0.55"
            />

            {/* 上半区 r=180 裁切内 3 枚金属镂空齿轮 */}
            <g clipPath={`url(#${crystalClipId})`}>
              {geometry.gears.map(gear => (
                <g
                  key={gear.id}
                  className={`hero-dial-gear hero-dial-gear--${gear.id}`}
                  style={{ animationDuration: `${gear.durationSeconds}s` }}
                >
                  {/* 对称导引圆锁定 fill-box center */}
                  <circle
                    cx={gear.cx}
                    cy={gear.cy}
                    r={gear.pitchGuideRadius}
                    fill="none"
                    stroke="rgba(255,255,255,0.01)"
                    strokeWidth="0.1"
                  />
                  {/* 带立体倒角金属质感与透视扇窗的齿轮主体 */}
                  <path
                    d={gear.path}
                    fill={`url(#${gearFillGradId})`}
                    fillRule="evenodd"
                    stroke={`url(#${gearStrokeGradId})`}
                    strokeWidth="0.95"
                    strokeLinejoin="round"
                  />
                  {/* 内轮缘装饰细圈 */}
                  <circle
                    cx={gear.cx}
                    cy={gear.cy}
                    r={gear.rimInnerRadius - 1.4}
                    fill="none"
                    stroke="rgba(255,255,255,0.22)"
                    strokeWidth="0.5"
                  />
                  {/* 分度虚线圆 */}
                  <circle
                    cx={gear.cx}
                    cy={gear.cy}
                    r={gear.pitchRadius}
                    fill="none"
                    stroke="rgba(255,255,255,0.85)"
                    strokeWidth="0.7"
                    strokeDasharray="2 3"
                    opacity="0.34"
                  />
                  {/* 轴套螺钉细节 */}
                  {gear.bolts.map((bolt, idx) => (
                    <circle
                      key={idx}
                      cx={bolt.x}
                      cy={bolt.y}
                      r="1.35"
                      fill="rgba(255,255,255,0.16)"
                      stroke="rgba(255,255,255,0.48)"
                      strokeWidth="0.55"
                    />
                  ))}
                </g>
              ))}

              {/* 蓝宝石斜向光学高光光束 */}
              <path
                d="M236 164 L478 272"
                stroke={`url(#${streakGradId})`}
                strokeWidth="2.4"
                strokeLinecap="round"
                filter={`url(#${crystalGlowFilterId})`}
                opacity="0.78"
              />
              <circle cx="428" cy="249" r="3" fill="rgba(255,255,255,0.55)" filter={`url(#${crystalGlowFilterId})`} />
            </g>

            {/* 蓝宝石表镜主高光圆弧与双圈金属包边 */}
            <circle cx="320" cy="320" r="185" stroke="rgba(255,255,255,0.14)" strokeWidth="0.65" />
            <circle cx="320" cy="320" r="182" stroke={`url(#${crystalRimGradId})`} strokeWidth="1.35" filter={`url(#${crystalGlowFilterId})`} />
            <path
              className="hero-dial-crystal-sheen"
              d="M174 214 A182 182 0 0 1 466 214"
              stroke="rgba(255,255,255,0.72)"
              strokeWidth="1.8"
              strokeLinecap="round"
              filter={`url(#${crystalGlowFilterId})`}
            />
          </g>

          {/* 极弱底层闭合外框，保证表镜完整几何轮廓 */}
          <circle cx="320" cy="320" r="182" stroke="rgba(255,255,255,0.08)" strokeWidth="0.75" />
        </svg>
      </div>
    </div>
  )
}
