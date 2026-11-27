import { useCallback, useEffect, useId, useRef, useState } from 'react'
import { useThemeAndMotion } from './useThemeAndMotion'
import './mechanical.css'

type Props = {
  /** 当前激活的设置分组索引（切换分组时触发眨眼反馈） */
  activeIndex?: number
}

const EYE_CX = 32
const EYE_CY = 26
const MAX_OFFSET_X = 7
const MAX_OFFSET_Y = 3.5
const TRACK_RADIUS_PX = 260
const LERP_FACTOR = 0.14
const BLINK_MS = 180
const EYE_PATH = 'M 13 26 Q 32 6 51 26 Q 32 46 13 26 Z'

/**
 * 黑黄 HUD 主题「设置（SETTINGS / CONTROL CENTER）」页眉右侧极简几何鼠标追踪义眼：
 * - 纯 SVG（viewBox 64×52，渲染约 52×44px）+ requestAnimationFrame 阻尼视线追踪（lerp 0.14）；
 * - 严格仅在 `hud` 主题下渲染，完全不影响 `mono` 主题；
 * - 挂接「05 界面与动效」与 `@media (prefers-reduced-motion: reduce)`，减弱/关闭动效时不绑定监听并居中静止。
 */
export function MinimalCyberEye({ activeIndex = 0 }: Props) {
  const { theme, motion } = useThemeAndMotion()
  const rawId = useId()
  const clipId = `eye-aperture-${rawId.replace(/:/g, '')}`

  const svgRef = useRef<SVGSVGElement>(null)
  const irisRef = useRef<SVGGElement>(null)
  const blinkTimerRef = useRef<number | null>(null)
  const blinkRafRef = useRef<number | null>(null)
  const autoBlinkTimerRef = useRef<number | null>(null)
  const prevIndexRef = useRef(activeIndex)

  const [blinking, setBlinking] = useState(false)
  const [systemReduced, setSystemReduced] = useState(() =>
    typeof window !== 'undefined' && typeof window.matchMedia === 'function'
      ? window.matchMedia('(prefers-reduced-motion: reduce)').matches
      : false,
  )

  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return
    const media = window.matchMedia('(prefers-reduced-motion: reduce)')
    const sync = () => setSystemReduced(media.matches)
    sync()
    if (typeof media.addEventListener === 'function') {
      media.addEventListener('change', sync)
      return () => media.removeEventListener('change', sync)
    }
    return undefined
  }, [])

  const fullMotion = theme === 'hud' && motion === 'full' && !systemReduced

  const startBlinkCycle = useCallback(() => {
    setBlinking(true)
    blinkTimerRef.current = window.setTimeout(() => {
      setBlinking(false)
      blinkTimerRef.current = null
    }, BLINK_MS)
  }, [])

  const triggerBlink = useCallback(() => {
    if (!fullMotion) return
    if (blinkRafRef.current !== null) {
      window.cancelAnimationFrame(blinkRafRef.current)
      blinkRafRef.current = null
    }
    if (blinkTimerRef.current !== null) {
      window.clearTimeout(blinkTimerRef.current)
      blinkTimerRef.current = null
      setBlinking(false)
      blinkRafRef.current = window.requestAnimationFrame(() => {
        blinkRafRef.current = null
        startBlinkCycle()
      })
      return
    }
    startBlinkCycle()
  }, [fullMotion, startBlinkCycle])

  useEffect(() => {
    return () => {
      if (blinkTimerRef.current !== null) window.clearTimeout(blinkTimerRef.current)
      if (blinkRafRef.current !== null) window.cancelAnimationFrame(blinkRafRef.current)
      if (autoBlinkTimerRef.current !== null) window.clearTimeout(autoBlinkTimerRef.current)
    }
  }, [])

  // 切换左侧 CONTROL / 08 分组时轻快眨眼一次
  useEffect(() => {
    if (prevIndexRef.current !== activeIndex) {
      prevIndexRef.current = activeIndex
      triggerBlink()
    }
  }, [activeIndex, triggerBlink])

  // 每隔 6-10 秒随机自然眨眼
  useEffect(() => {
    if (!fullMotion) {
      setBlinking(false)
      return
    }
    let cancelled = false
    const scheduleNext = () => {
      const delay = 6000 + Math.random() * 4000
      autoBlinkTimerRef.current = window.setTimeout(() => {
        if (cancelled) return
        triggerBlink()
        scheduleNext()
      }, delay)
    }
    scheduleNext()
    return () => {
      cancelled = true
      if (autoBlinkTimerRef.current !== null) {
        window.clearTimeout(autoBlinkTimerRef.current)
        autoBlinkTimerRef.current = null
      }
    }
  }, [fullMotion, triggerBlink])

  // 全局鼠标阻尼视线追踪（仅在「动效完整」且未触发系统减弱动效时绑定）
  useEffect(() => {
    const iris = irisRef.current
    if (!fullMotion) {
      if (iris) iris.setAttribute('transform', 'translate(0, 0)')
      return
    }

    let targetX = 0
    let targetY = 0
    let currentX = 0
    let currentY = 0
    let rafId: number | null = null

    const step = () => {
      const nextX = currentX + (targetX - currentX) * LERP_FACTOR
      const nextY = currentY + (targetY - currentY) * LERP_FACTOR
      const settled =
        Math.abs(targetX - nextX) < 0.01 && Math.abs(targetY - nextY) < 0.01

      currentX = settled ? targetX : nextX
      currentY = settled ? targetY : nextY

      if (irisRef.current) {
        irisRef.current.setAttribute(
          'transform',
          `translate(${currentX.toFixed(2)}, ${currentY.toFixed(2)})`,
        )
      }

      if (!settled) {
        rafId = window.requestAnimationFrame(step)
      } else {
        rafId = null
      }
    }

    const ensureLoop = () => {
      if (rafId === null) {
        rafId = window.requestAnimationFrame(step)
      }
    }

    const onMouseMove = (event: MouseEvent) => {
      const rect = svgRef.current?.getBoundingClientRect()
      if (!rect || rect.width <= 0 || rect.height <= 0) return

      const centerX = rect.left + rect.width / 2
      const centerY = rect.top + rect.height / 2
      const vx = event.clientX - centerX
      const vy = event.clientY - centerY
      const dist = Math.hypot(vx, vy)

      if (dist < 0.001) {
        targetX = 0
        targetY = 0
      } else {
        const mag = Math.min(1, dist / TRACK_RADIUS_PX)
        targetX = (vx / dist) * MAX_OFFSET_X * mag
        targetY = (vy / dist) * MAX_OFFSET_Y * mag
      }
      ensureLoop()
    }

    const resetGaze = () => {
      targetX = 0
      targetY = 0
      ensureLoop()
    }

    const onMouseOut = (event: MouseEvent) => {
      if (!event.relatedTarget) resetGaze()
    }

    window.addEventListener('mousemove', onMouseMove, { passive: true })
    window.addEventListener('mouseout', onMouseOut, { passive: true })
    window.addEventListener('blur', resetGaze)

    return () => {
      window.removeEventListener('mousemove', onMouseMove)
      window.removeEventListener('mouseout', onMouseOut)
      window.removeEventListener('blur', resetGaze)
      if (rafId !== null) window.cancelAnimationFrame(rafId)
    }
  }, [fullMotion])

  if (theme !== 'hud') {
    return null
  }

  return (
    <button
      type="button"
      className="minimal-cyber-eye"
      aria-label="机械义眼状态模块，点击可触发眨眼"
      data-motion={motion}
      data-tracking={fullMotion ? 'true' : 'false'}
      data-blinking={blinking ? 'true' : 'false'}
      onClick={triggerBlink}
    >
      <svg
        ref={svgRef}
        className="minimal-cyber-eye-svg"
        viewBox="0 0 64 52"
        width="52"
        height="44"
        fill="none"
        aria-hidden="true"
      >
        <defs>
          <clipPath id={clipId}>
            <path d={EYE_PATH} />
          </clipPath>
        </defs>

        {/* 外框：深炭黑圆角底座 + 左右赛博黄微切角竖条 */}
        <rect
          x="8"
          y="4"
          width="48"
          height="44"
          rx="4"
          fill="#0E1015"
          stroke="rgba(255, 255, 255, 0.14)"
          strokeWidth="1"
        />
        <path d="M 6 5 H 7 V 47 H 6 L 4 45 V 7 Z" fill="#FCEE0A" />
        <path d="M 57 5 H 58 L 60 7 V 45 L 58 47 H 57 Z" fill="#FCEE0A" />

        {/* 上下三点装饰 (• • •) */}
        <g className="cyber-eye-dots" fill="#FCEE0A" opacity="0.85">
          <circle cx={EYE_CX - 6} cy="9.5" r="1.3" />
          <circle cx={EYE_CX} cy="9.5" r="1.3" />
          <circle cx={EYE_CX + 6} cy="9.5" r="1.3" />
          <circle cx={EYE_CX - 6} cy="42.5" r="1.3" />
          <circle cx={EYE_CX} cy="42.5" r="1.3" />
          <circle cx={EYE_CX + 6} cy="42.5" r="1.3" />
        </g>

        {/* 杏仁眼眶与内部可追踪眼球（眨眼时整体绕中轴压扁闭合） */}
        <g className="cyber-eye-blink-group">
          <g clipPath={`url(#${clipId})`}>
            <g ref={irisRef} className="cyber-eye-iris" transform="translate(0, 0)">
              <circle
                cx={EYE_CX}
                cy={EYE_CY}
                r="10.5"
                fill="none"
                stroke="#FCEE0A"
                strokeWidth="1.3"
              />
              <circle
                cx={EYE_CX}
                cy={EYE_CY}
                r="7.8"
                fill="none"
                stroke="#FCEE0A"
                strokeWidth="1.3"
              />
              <rect
                x={EYE_CX - 1.5}
                y={EYE_CY - 4.25}
                width="3"
                height="8.5"
                rx="0.8"
                fill="#FCEE0A"
              />
            </g>
          </g>

          <path
            className="cyber-eye-lid"
            d={EYE_PATH}
            fill="none"
            stroke="#FCEE0A"
            strokeWidth="1.6"
            strokeLinejoin="round"
          />
        </g>
      </svg>
    </button>
  )
}
