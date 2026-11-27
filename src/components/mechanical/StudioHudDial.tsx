import { useMemo } from 'react'
import './mechanical.css'

type Props = {
  /** 是否处于高级版生成中 */
  active?: boolean
  /** 悬停/聚焦“生成高级版 →”或生成中时触发扳动卡榫微转 */
  highlighted?: boolean
}

const OUTER_CALLOUTS = [
  {
    id: 'ax',
    text: 'AX.00 // TYPESET',
    x: 92,
    y: 122,
    anchor: 'start' as const,
    leader: 'M92 126 H168 L184 142',
  },
  {
    id: 'sync',
    text: 'SYNC: READY',
    activeText: 'SYNC: ACTIVE',
    x: 696,
    y: 114,
    anchor: 'end' as const,
    leader: 'M696 118 H620 L604 134',
  },
  {
    id: 'scale',
    text: 'SCALE 1:1',
    x: 688,
    y: 410,
    anchor: 'end' as const,
    leader: 'M688 402 H632',
  },
] as const

/** 生成简洁锐利的 10 齿实心哑光黄齿轮 + 中心镂空圆孔路径（evenodd） */
function buildSolidCogPath(): string {
  const teeth = 10
  const cx = 18
  const cy = 18
  const tipR = 16.5
  const rootR = 12.2
  const boreR = 5.4
  const tau = (Math.PI * 2) / teeth
  const round = (v: number) => Number(v.toFixed(2))
  const cmds: string[] = []

  for (let i = 0; i < teeth; i++) {
    const a = -Math.PI / 2 + i * tau
    const profile: readonly [number, number][] = [
      [a - tau * 0.34, rootR],
      [a - tau * 0.16, tipR],
      [a + tau * 0.16, tipR],
      [a + tau * 0.34, rootR],
    ]
    for (const [ang, r] of profile) {
      cmds.push(`${cmds.length === 0 ? 'M' : 'L'}${round(cx + Math.cos(ang) * r)} ${round(cy + Math.sin(ang) * r)}`)
    }
  }
  cmds.push('Z')
  cmds.push(`M${cx + boreR} ${cy} A${boreR} ${boreR} 0 1 0 ${cx - boreR} ${cy} A${boreR} ${boreR} 0 1 0 ${cx + boreR} ${cy} Z`)
  return cmds.join(' ')
}

/**
 * HUD 主题方案 B 画布骨架（StudioHudDial）：
 * - 四角 16px × 16px 极简细灰白 L 形角标（距边缘 18px）；
 * - 画布外围低存在感等宽工程微字（AX.00 // TYPESET、SYNC: READY、SCALE 1:1）。
 */
export function StudioHudDial({ active = false, highlighted = false }: Props) {
  return (
    <div
      className="studio-hud-stage"
      data-active={active}
      data-highlighted={highlighted}
      aria-hidden="true"
    >
      <div className="studio-hud-frame">
        <span className="studio-hud-corner studio-hud-corner--tl" />
        <span className="studio-hud-corner studio-hud-corner--tr" />
        <span className="studio-hud-corner studio-hud-corner--bl" />
        <span className="studio-hud-corner studio-hud-corner--br" />
      </div>

      <svg className="studio-hud-callout-svg" viewBox="0 0 780 520" fill="none">
        {OUTER_CALLOUTS.map(item => (
          <g key={item.id}>
            <path d={item.leader} stroke="rgba(255, 255, 255, 0.12)" strokeWidth="0.7" />
            <text
              x={item.x}
              y={item.y}
              textAnchor={item.anchor}
              fontFamily="var(--font-mono), monospace"
              fontSize="9"
              fill="rgba(255, 255, 255, 0.22)"
            >
              {'activeText' in item && active ? item.activeText : item.text}
            </text>
          </g>
        ))}
      </svg>
    </div>
  )
}

/**
 * HUD 主题方案 B 核心视觉：位于居中主标题上方约 28px 处的「水平精密游标刻度轨 + 单体哑光黄齿轮」。
 */
export function StudioHudVernierRail({ highlighted = false }: { highlighted?: boolean }) {
  const { ticks, cogPath } = useMemo(() => {
    const items: { x: number; level: 'minor' | 'medium' | 'major'; label?: string }[] = []
    for (let i = 0; i <= 100; i++) {
      const x = 30 + i * 6
      const isMajor = i % 10 === 0
      const isMedium = !isMajor && i % 5 === 0
      const level = isMajor ? 'major' : isMedium ? 'medium' : 'minor'
      // 跳过正中心 x=330 (i===50)，保留给 48px 圆形齿轮遮罩区
      const label = isMajor && i !== 50 ? String((i / 10) * 2) : undefined
      items.push({ x, level, label })
    }
    return { ticks: items, cogPath: buildSolidCogPath() }
  }, [])

  return (
    <div className="studio-hud-vernier-rail" data-highlighted={highlighted} aria-hidden="true">
      <div className="studio-hud-vernier-track-wrap">
        <svg className="studio-hud-vernier-svg" viewBox="0 0 660 52" preserveAspectRatio="xMidYMid meet" fill="none">
          <line x1="0" y1="26" x2="660" y2="26" stroke="rgba(252, 238, 10, 0.24)" strokeWidth="1" />
          {ticks.map(tick => (
            <g key={tick.x}>
              <line
                x1={tick.x}
                y1={tick.level === 'major' ? 15 : tick.level === 'medium' ? 19 : 22}
                x2={tick.x}
                y2={tick.level === 'major' ? 27 : 26}
                stroke={
                  tick.level === 'major'
                    ? 'rgba(252, 238, 10, 0.45)'
                    : tick.level === 'medium'
                      ? 'rgba(252, 238, 10, 0.30)'
                      : 'rgba(252, 238, 10, 0.18)'
                }
                strokeWidth="1"
              />
              {tick.label && (
                <text
                  x={tick.x}
                  y="11"
                  textAnchor="middle"
                  fontFamily="var(--font-mono), monospace"
                  fontSize="8"
                  fill="rgba(252, 238, 10, 0.32)"
                >
                  {tick.label}
                </text>
              )}
            </g>
          ))}
          {/* 48px #07080A 圆形遮罩断开齿轮两侧刻度 */}
          <circle cx="330" cy="26" r="24" fill="#07080A" />
        </svg>
      </div>

      {/* 居中单体实心哑光黄齿轮（34×34px）+ 上下 14px 垂直细轴线 */}
      <div className="studio-hud-cog-center">
        <span className="studio-hud-cog-axis studio-hud-cog-axis--top" />
        <svg className="studio-hud-vernier-cog" viewBox="0 0 36 36" width="34" height="34" fill="none">
          <path d={cogPath} fill="#FCEE0A" fillRule="evenodd" />
        </svg>
        <span className="studio-hud-cog-axis studio-hud-cog-axis--bottom" />
      </div>
    </div>
  )
}
