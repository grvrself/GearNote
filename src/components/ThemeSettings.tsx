import { useEffect, useState } from 'react'
import { applyMotion, applyTheme, readMotion, readTheme, type MotionPreference, type Theme } from '../theme'
import './theme-settings.css'

const motionOptions: { id: MotionPreference; label: string; detail: string }[] = [
  { id: 'full', label: '完整', detail: '齿轮组联动、反向旋转与扫描线。' },
  { id: 'reduced', label: '减弱', detail: '只保留慢速品牌齿轮与工作状态动画。' },
  { id: 'off', label: '关闭', detail: '所有装饰与状态动画保持静止。' },
]

export function ThemeSettings() {
  const [theme, setTheme] = useState<Theme>(readTheme)
  const [motion, setMotion] = useState<MotionPreference>(readMotion)

  useEffect(() => {
    const onTheme = (event: Event) => setTheme((event as CustomEvent<Theme>).detail)
    const onMotion = (event: Event) => setMotion((event as CustomEvent<MotionPreference>).detail)
    window.addEventListener('knowflow:theme', onTheme)
    window.addEventListener('knowflow:motion', onMotion)
    return () => {
      window.removeEventListener('knowflow:theme', onTheme)
      window.removeEventListener('knowflow:motion', onMotion)
    }
  }, [])

  const chooseMotion = (next: MotionPreference) => {
    applyMotion(next)
    setMotion(next)
  }

  return <section className="theme-settings" aria-labelledby="theme-settings-title">
    <div className="theme-settings-heading"><span>APPEARANCE / 02</span><h2 id="theme-settings-title">界面风格</h2><p>两套风格共用相同的资料、会话与功能，只改变视觉表达。</p></div>
    <div className="theme-choices" role="group" aria-label="界面风格">
      <button className="theme-choice" data-theme="hud" type="button" aria-pressed={theme === 'hud'} onClick={() => { applyTheme('hud'); setTheme('hud') }}>
        <span className="theme-choice-index">01 / MECHANICAL</span><strong>机械传动</strong><span>齿轮联动 · 金属切角 · 能量黄</span><small>原工业 HUD 的升级版，突出工作台的机械感。</small>
      </button>
      <button className="theme-choice" data-theme="mono" type="button" aria-pressed={theme === 'mono'} onClick={() => { applyTheme('mono'); setTheme('mono') }}>
        <span className="theme-choice-index">02 / MONOCHROME</span><strong>简约黑白</strong><span>灰阶层级 · 克制留白 · 少装饰</span><small>适合长时间阅读、整理和专注提问。</small>
      </button>
    </div>
    <div className="motion-settings">
      <div className="motion-settings-head"><strong>齿轮动效</strong><span>独立于界面风格，并遵循系统的减少动画设置</span></div>
      <div className="motion-options" role="group" aria-label="齿轮动效">
        {motionOptions.map(option => <button key={option.id} type="button" aria-pressed={motion === option.id} onClick={() => chooseMotion(option.id)}>{option.label}</button>)}
      </div>
      <p className="motion-description" aria-live="polite">{motionOptions.find(option => option.id === motion)?.detail}</p>
    </div>
  </section>
}
