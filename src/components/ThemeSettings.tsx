import { useState } from 'react'
import { applyTheme, readTheme, type Theme } from '../theme'
import './theme-settings.css'

export function ThemeSettings() {
  const [theme, setTheme] = useState<Theme>(readTheme)
  return <section className="theme-settings" aria-labelledby="theme-settings-title">
    <h2 id="theme-settings-title">界面风格</h2>
    <div role="group" aria-label="界面风格">
      <button type="button" aria-pressed={theme === 'hud'} onClick={() => { applyTheme('hud'); setTheme('hud') }}>工业 HUD</button>
      <button type="button" aria-pressed={theme === 'mono'} onClick={() => { applyTheme('mono'); setTheme('mono') }}>简约黑白</button>
    </div>
  </section>
}
