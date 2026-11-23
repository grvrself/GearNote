import { isTauri } from '@tauri-apps/api/core'
import { emit, listen } from '@tauri-apps/api/event'

export type Theme = 'hud' | 'mono'
export type MotionPreference = 'full' | 'reduced' | 'off'

const THEME_KEY = 'knowflow-theme'
const MOTION_KEY = 'gearnote-motion'

function systemMotion(): MotionPreference {
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'off' : 'full'
}

function storedMotion(): MotionPreference | null {
  try {
    const value = localStorage.getItem(MOTION_KEY)
    return value === 'full' || value === 'reduced' || value === 'off' ? value : null
  } catch { return null }
}

export function readTheme(): Theme {
  try { return localStorage.getItem(THEME_KEY) === 'hud' ? 'hud' : 'mono' }
  catch { return 'mono' }
}

export function readMotion(): MotionPreference {
  return storedMotion() ?? systemMotion()
}

export function applyTheme(theme: Theme) {
  document.body.dataset.theme = theme
  try { localStorage.setItem(THEME_KEY, theme) } catch { /* Keep the theme for this window. */ }
  window.dispatchEvent(new CustomEvent<Theme>('knowflow:theme', { detail: theme }))
  if (isTauri()) void emit('knowflow:theme-changed', theme)
}

export function applyMotion(motion: MotionPreference) {
  document.body.dataset.motion = motion
  try { localStorage.setItem(MOTION_KEY, motion) } catch { /* Keep the preference for this window. */ }
  window.dispatchEvent(new CustomEvent<MotionPreference>('knowflow:motion', { detail: motion }))
  if (isTauri()) void emit('gearnote:motion-changed', motion)
}

export function initializeTheme() {
  document.body.dataset.theme = readTheme()
  window.addEventListener('storage', event => {
    if (event.key === THEME_KEY) document.body.dataset.theme = readTheme()
  })
  if (isTauri()) void listen<Theme>('knowflow:theme-changed', event => {
    if (event.payload === 'hud' || event.payload === 'mono') document.body.dataset.theme = event.payload
  })
}

export function initializeMotion() {
  document.body.dataset.motion = readMotion()
  window.addEventListener('storage', event => {
    if (event.key === MOTION_KEY) document.body.dataset.motion = readMotion()
  })
  const media = window.matchMedia('(prefers-reduced-motion: reduce)')
  media.addEventListener('change', () => {
    if (storedMotion() === null) document.body.dataset.motion = systemMotion()
  })
  if (isTauri()) void listen<MotionPreference>('gearnote:motion-changed', event => {
    if (event.payload === 'full' || event.payload === 'reduced' || event.payload === 'off') {
      document.body.dataset.motion = event.payload
    }
  })
}
