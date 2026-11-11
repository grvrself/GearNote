import { isTauri } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { HudButton } from './HudButton'
import './topbar.css'

export type BrowserChoice = 'Chrome' | 'Edge'

type Props = {
  sectionId: string
  browser: BrowserChoice
  onBrowserChange: (browser: BrowserChoice) => void
  extensionConnected: boolean
  onOpenExtensionSetup: () => void
}

function WindowGlyph({ kind }: { kind: 'minimize' | 'maximize' | 'close' }) {
  if (kind === 'minimize') return <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 8.5h10" /></svg>
  if (kind === 'maximize') return <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 3.5h9v9h-9z" /></svg>
  return <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 3.5l9 9m0-9l-9 9" /></svg>
}

export function Topbar({ sectionId, browser, onBrowserChange, extensionConnected, onOpenExtensionSetup }: Props) {
  const desktop = isTauri()
  const window = desktop ? getCurrentWindow() : null

  return <header className="hud-topbar">
    <div className="hud-title-strip" data-tauri-drag-region>
      <div className="hud-title-identity" data-tauri-drag-region>
        <span className="hud-title-serial" data-tauri-drag-region>KF-001 / {sectionId.toUpperCase()}</span>
        <span className="hud-title-name" data-tauri-drag-region>KNOWFLOW</span>
        <span className="hud-title-section" data-tauri-drag-region>知识工作台</span>
      </div>
      <div className="hud-title-drag-space" data-tauri-drag-region aria-hidden="true" />
      <div className="hud-window-controls" role="group" aria-label="窗口控制">
        <button type="button" aria-label="最小化" title="最小化" disabled={!desktop} onClick={() => { void window?.minimize() }}><WindowGlyph kind="minimize" /></button>
        <button type="button" aria-label="最大化或还原" title="最大化或还原" disabled={!desktop} onClick={() => { void window?.toggleMaximize() }}><WindowGlyph kind="maximize" /></button>
        <button className="hud-window-close" type="button" aria-label="关闭" title="关闭" disabled={!desktop} onClick={() => { void window?.close() }}><WindowGlyph kind="close" /></button>
      </div>
    </div>

    <div className="hud-command-strip">
      <label className="hud-browser-control"><span>浏览器</span><select value={browser} onChange={event => onBrowserChange(event.target.value as BrowserChoice)}><option value="Chrome">Chrome</option><option value="Edge">Edge</option></select></label>
      <div className="hud-extension-state" data-connected={extensionConnected} role="status"><span className="hud-extension-mark" aria-hidden="true" /><span>{extensionConnected ? '扩展已连接' : '扩展未连接'}</span></div>
      <div className="hud-capture-actions">
        <HudButton type="button" variant="primary" onClick={onOpenExtensionSetup}>↓ 网页捕获说明</HudButton>
        <HudButton type="button" onClick={onOpenExtensionSetup}>▣ 视频字幕说明</HudButton>
      </div>
    </div>
  </header>
}
