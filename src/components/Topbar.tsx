import { useEffect, useState } from 'react'
import { isTauri } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { applyMotion, readMotion, type MotionPreference } from '../theme'
import { HudButton } from './HudButton'
import { GearIcon } from './GearIcon'
import { MechanicalGear } from './MechanicalGear'
import './topbar.css'

export type BrowserChoice = 'Chrome' | 'Edge'

type Props = {
  sectionId: string
  browser: BrowserChoice
  onBrowserChange: (browser: BrowserChoice) => void
  extensionConnected: boolean
  onOpenExtensionSetup: () => void
  onOpenUrlImport?: () => void
}

function WindowGlyph({ kind }: { kind: 'minimize' | 'maximize' | 'close' }) {
  if (kind === 'minimize') return <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 8.5h10" /></svg>
  if (kind === 'maximize') return <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 3.5h9v9h-9z" /></svg>
  return <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 3.5l9 9m0-9l-9 9" /></svg>
}

export function Topbar({ sectionId, browser, onBrowserChange, extensionConnected, onOpenExtensionSetup, onOpenUrlImport }: Props) {
  const desktop = isTauri()
  const window = desktop ? getCurrentWindow() : null
  const [motion, setMotion] = useState<MotionPreference>(readMotion)

  useEffect(() => {
    const onMotion = (event: Event) => setMotion((event as CustomEvent<MotionPreference>).detail)
    globalThis.addEventListener('knowflow:motion', onMotion)
    return () => globalThis.removeEventListener('knowflow:motion', onMotion)
  }, [])

  const cycleMotion = () => {
    const next = motion === 'full' ? 'reduced' : motion === 'reduced' ? 'off' : 'full'
    applyMotion(next)
    setMotion(next)
  }

  return <header className="hud-topbar" data-tauri-drag-region>
    <div className="hud-title-identity" data-tauri-drag-region>
      <span className="mono-brand-gear" data-tauri-drag-region><GearIcon kind="brand" size={20} /></span>
      <span className="hud-title-name" data-tauri-drag-region>GEARNOTE</span>
      <span className="hud-title-serial" data-tauri-drag-region>GN-001 / {sectionId.toUpperCase()}</span>
      <span className="hud-title-section" data-tauri-drag-region>知识工作台</span>
    </div>
    <div className="hud-title-drag-space" data-tauri-drag-region aria-hidden="true" />
    <div className="hud-bridge-pill" data-connected={extensionConnected}>
      <span className={`hud-extension-mark gn-led ${extensionConnected ? 'gn-led--online' : 'gn-led--idle'}`} aria-hidden="true" />
      <label className="hud-browser-control"><select aria-label="浏览器" value={browser} onChange={event => onBrowserChange(event.target.value as BrowserChoice)}><option value="Chrome">Chrome</option><option value="Edge">Edge</option></select></label>
      <span className="hud-extension-state" data-connected={extensionConnected} role="status">{extensionConnected ? '扩展已连接' : '扩展未连接'}</span>
    </div>
    <div className="hud-title-drag-space" data-tauri-drag-region aria-hidden="true">
      <div className="mech-command-gears" aria-hidden="true"><MechanicalGear teeth={12} speed="12s" /><i /><MechanicalGear teeth={16} speed="12s" reverse /><i /><MechanicalGear teeth={12} speed="12s" /></div>
    </div>
    <div className="hud-capture-actions">
      {onOpenUrlImport && <HudButton className="hud-url-import-entry" type="button" onClick={onOpenUrlImport}>＋ 导入链接</HudButton>}
      <HudButton type="button" onClick={onOpenExtensionSetup}>▣ 字幕说明</HudButton>
      <HudButton type="button" onClick={onOpenExtensionSetup}>↓ 网页捕获</HudButton>
      <button className="mech-motion-toggle" type="button" data-motion={motion} aria-label={`齿轮动效：${motion === 'full' ? '完整' : motion === 'reduced' ? '减弱' : '关闭'}，点击切换`} title="点击切换完整、减弱和关闭" onClick={cycleMotion}><i />MOTION / {motion === 'full' ? 'FULL' : motion === 'reduced' ? 'LOW' : 'OFF'}</button>
    </div>
    <div className="hud-window-controls" role="group" aria-label="窗口控制">
      <button type="button" aria-label="最小化" title="最小化" disabled={!desktop} onClick={() => { void window?.minimize() }}><WindowGlyph kind="minimize" /></button>
      <button type="button" aria-label="最大化或还原" title="最大化或还原" disabled={!desktop} onClick={() => { void window?.toggleMaximize() }}><WindowGlyph kind="maximize" /></button>
      <button className="hud-window-close" type="button" aria-label="关闭" title="关闭" disabled={!desktop} onClick={() => { void window?.close() }}><WindowGlyph kind="close" /></button>
    </div>
  </header>
}
