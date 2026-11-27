import { useId } from 'react'
import './mechanical.css'

/**
 * HUD 主题「技能工作台」右上角高精密 PCB 主板走线暗纹（viewBox 680×140，纯 SVG）：
 * 全部走线严格遵守水平 / 垂直 / 45° 等距折线（|dx| === |dy|），含 4.5px 紧密平行总线束、
 * 对齐 BGA 过孔列、微型栅格排线、SMD 贴片焊盘阵列与微量暗金分支，不干扰任何前景操作或受下方卡片增减影响。
 */
export function CircuitBoardBackdrop() {
  const maskId = useId().replace(/:/g, '')

  return (
    <div className="circuit-board-backdrop" aria-hidden="true">
      <svg viewBox="0 0 680 140" preserveAspectRatio="xMidYMid slice" fill="none">
        <defs>
          <radialGradient id={`${maskId}-grad`} cx="56%" cy="50%" r="54%">
            <stop offset="0%" stopColor="#FFFFFF" stopOpacity="1" />
            <stop offset="56%" stopColor="#FFFFFF" stopOpacity="0.82" />
            <stop offset="100%" stopColor="#FFFFFF" stopOpacity="0" />
          </radialGradient>
          <mask id={maskId}>
            <rect x="0" y="0" width="680" height="140" fill={`url(#${maskId}-grad)`} />
          </mask>
        </defs>

        <g mask={`url(#${maskId})`}>
          {/* 1. 核心高密度平行总线束（4.5px 精密间距 + 严格 45° 同步折角） */}
          <g stroke="rgba(255, 255, 255, 0.16)" strokeWidth="0.8" strokeLinecap="round" strokeLinejoin="round">
            {/* 中央主干 4 线总线 -> 45° 向左下优雅扇出 */}
            <path d="M330 0 V68 L272 126 H120" />
            <path d="M334.5 0 V71 L276.5 129 H104" />
            <path d="M339 0 V74 L281 132 H92" />
            <path d="M343.5 0 V77 L285.5 135 H148" />

            {/* 左上 4 线总线 -> 45° 下折汇入垂直对齐过孔列 */}
            <path d="M130 16 H274 L298 40 H314" />
            <path d="M148 20.5 H270 L294.5 45 H314" />
            <path d="M166 25 H266 L291 50 H314" />
            <path d="M184 29.5 H262 L287.5 55 H314" />

            {/* 中左支线与终端过孔 */}
            <path d="M110 48 H212 L252 88 H278" />
            <path d="M126 70 L132 76 H248" />

            {/* 右上 4 线过孔列 -> 45° 上折平行向右延伸 */}
            <path d="M360 36 H376 L396 16 H610" />
            <path d="M360 40.5 H380 L400 20.5 H590" />
            <path d="M360 45 H384 L404 25 H566" />
            <path d="M360 49.5 H388 L408 29.5 H540" />

            {/* 右中 45° 上折支线与右下双线平行扇出 */}
            <path d="M390 66 L402 54 H582" />
            <path d="M396 84 H438 L468 114 H622" />
            <path d="M396 89 H434 L464 119 H596" />

            {/* 下缘垂直汇入细线 */}
            <path d="M372 140 V100 L358 86" />
            <path d="M376.5 140 V96 L362.5 82" />

            {/* 微缩虚线导轨 */}
            <line x1="418" y1="39" x2="482" y2="39" strokeDasharray="1.5 3.5" opacity="0.85" />
            <line x1="472" y1="102" x2="536" y2="102" strokeDasharray="1.5 3.5" opacity="0.72" />
          </g>

          {/* 2. 暗金色微光末端分支 */}
          <g stroke="rgba(252, 238, 10, 0.24)" strokeWidth="0.8" strokeLinecap="round" strokeLinejoin="round">
            <path d="M414 108 L428 94 H536 L552 110 H632" />
            <path d="M510 12 H554 L568 26 H626" />
          </g>

          {/* 3. 空心 BGA 过孔环（r=1.9, fill="#07080A"） */}
          <g fill="#07080A" stroke="rgba(255, 255, 255, 0.28)" strokeWidth="0.85">
            {[
              [314, 40],
              [314, 45],
              [314, 50],
              [314, 55],
              [278, 88],
              [292, 88],
              [360, 36],
              [360, 40.5],
              [360, 45],
              [360, 49.5],
              [390, 66],
              [396, 84],
              [396, 89],
              [358, 86],
              [362.5, 82],
            ].map(([cx, cy], idx) => (
              <circle key={`v-${idx}`} cx={cx} cy={cy} r="1.9" />
            ))}
          </g>

          {/* 实心终端圆点与暗金过孔环 */}
          <circle cx="248" cy="76" r="2.1" fill="rgba(255, 255, 255, 0.28)" />
          <circle cx="414" cy="108" r="1.9" fill="#07080A" stroke="rgba(252, 238, 10, 0.40)" strokeWidth="0.85" />
          <circle cx="632" cy="110" r="1.9" fill="#07080A" stroke="rgba(252, 238, 10, 0.40)" strokeWidth="0.85" />
          <circle cx="626" cy="26" r="1.8" fill="#07080A" stroke="rgba(252, 238, 10, 0.36)" strokeWidth="0.85" />

          {/* 4. 微型栅格排线与贴片焊盘阵列（2×3.5px） */}
          <g fill="rgba(255, 255, 255, 0.17)">
            {Array.from({ length: 9 }, (_, i) => (
              <rect key={`g-${i}`} x={164 + i * 4} y="67" width="1.5" height="5" />
            ))}
            {[422, 428].map(x => (
              <rect key={`s1-${x}`} x={x} y="44" width="2" height="3.5" />
            ))}
            {[492, 498, 504].map(x => (
              <rect key={`s2-${x}`} x={x} y="44" width="2" height="3.5" />
            ))}
            {[486, 492, 498, 504].map(x => (
              <rect key={`s3-${x}`} x={x} y="106" width="2" height="3.5" />
            ))}
          </g>

          {/* 5. 动效完整模式下的低频脉冲流 */}
          <path
            className="circuit-trace-pulse"
            d="M334.5 0 V71 L276.5 129 H104"
            stroke="rgba(252, 238, 10, 0.34)"
            strokeWidth="1.05"
            strokeDasharray="20 240"
            strokeLinecap="round"
          />
          <path
            className="circuit-trace-pulse circuit-trace-pulse--cyan"
            d="M414 108 L428 94 H536 L552 110 H632"
            stroke="rgba(0, 240, 255, 0.30)"
            strokeWidth="1"
            strokeDasharray="16 220"
            strokeLinecap="round"
          />
        </g>
      </svg>
    </div>
  )
}
