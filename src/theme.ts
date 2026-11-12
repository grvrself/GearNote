import { isTauri } from '@tauri-apps/api/core'
import { emit, listen } from '@tauri-apps/api/event'

export type Theme = 'hud' | 'mono'

const KEY = 'knowflow-theme'

export function readTheme(): Theme {
  try { return localStorage.getItem(KEY) === 'mono' ? 'mono' : 'hud' }
  catch { return 'hud' }
}

export function applyTheme(theme: Theme) {
  document.body.dataset.theme = theme
  try { localStorage.setItem(KEY, theme) } catch { /* Keep the theme for this window. */ }
  window.dispatchEvent(new CustomEvent<Theme>('knowflow:theme', { detail: theme }))
  if (isTauri()) void emit('knowflow:theme-changed', theme)
}

export function initializeTheme() {
  document.body.dataset.theme = readTheme()
  window.addEventListener('storage', event => {
    if (event.key === KEY) document.body.dataset.theme = readTheme()
  })
  if (isTauri()) void listen<Theme>('knowflow:theme-changed', event => {
    if (event.payload === 'hud' || event.payload === 'mono') document.body.dataset.theme = event.payload
  })
}
