import { useEffect, useState } from 'react'
import { isTauri } from '@tauri-apps/api/core'
import type { AgentStatus } from '../agent'
import type { BridgeStatus } from '../records'
import type { SettingsSection } from './Sidebar'
import { AgentSetup } from './AgentSetup'
import { PersonaSetup } from './PersonaSetup'
import { ThemeSettings } from './ThemeSettings'
import { HudButton } from './HudButton'
import { MechanicalGear } from './MechanicalGear'
import { MinimalCyberEye, RevolverCylinderWidget, SettingsBlueprintPlate, SettingsGearBackdrop, SettingsOverviewTelemetry, HudCornerBrackets, ThornRoseBackdrop, VernierGauge } from './mechanical'
import { readMotion, readTheme } from '../theme'
import { getToolchainStatus, getWorkspaceInfo, openWorkspacePath, type ToolStatus, type WorkspaceInfo } from '../extensions'
import { formatToolVersion } from '../utils/format'
import packageInfo from '../../package.json'
import './extension-workspace.css'

type Props = {
  section: SettingsSection
  onSectionChange: (section: SettingsSection) => void
  agentStatus: AgentStatus | null
  onAgentStatusChange: (status: AgentStatus) => void
  bridgeStatus: BridgeStatus | null
}

const sections: { id: SettingsSection; label: string; index: string }[] = [
  { id: 'overview', label: '概览', index: '01' },
  { id: 'agent', label: 'Agent 服务', index: '02' },
  { id: 'reading', label: '阅读与检索', index: '03' },
  { id: 'persona', label: '人格与表达', index: '04' },
  { id: 'appearance', label: '界面与动效', index: '05' },
  { id: 'data', label: '数据与存储', index: '06' },
  { id: 'shortcuts', label: '快捷键', index: '07' },
  { id: 'about', label: '关于与诊断', index: '08' },
]

export function SettingsPanel({ section, onSectionChange, agentStatus, onAgentStatusChange, bridgeStatus }: Props) {
  const desktop = isTauri()
  const [tools, setTools] = useState<ToolStatus[]>([])
  const [toolsLoading, setToolsLoading] = useState(desktop)
  const [workspace, setWorkspace] = useState<WorkspaceInfo | null>(null)
  const [error, setError] = useState('')
  const currentIndex = Math.max(0, sections.findIndex(item => item.id === section))
  const current = sections[currentIndex] ?? sections[0]

  useEffect(() => {
    if (!desktop) return
    let cancelled = false
    // 诊断只在打开设置时执行一次，并延迟到首屏绘制之后启动：
    // 避免工具进程（尤其 yt-dlp）的启动 CPU 冲击与首屏渲染争用造成卡顿。
    // 手动刷新走"关于与诊断"里的"重新检测"按钮。
    const schedule = (callback: () => void) => {
      if (typeof window.requestIdleCallback === 'function') window.requestIdleCallback(callback, { timeout: 1500 })
      else window.setTimeout(callback, 120)
    }
    schedule(() => {
      if (cancelled) return
      void getToolchainStatus()
        .then(result => { if (!cancelled) { setTools(result); setToolsLoading(false) } })
        .catch(cause => { if (!cancelled) { setError(String(cause)); setToolsLoading(false) } })
      void getWorkspaceInfo().then(result => { if (!cancelled) setWorkspace(result) }).catch(cause => { if (!cancelled) setError(String(cause)) })
    })
    return () => { cancelled = true }
  }, [desktop])

  const openPath = async (kind: 'data' | 'cache' | 'exports') => {
    try { await openWorkspacePath(kind) } catch (cause) { setError(String(cause)) }
  }
  const readyCount = tools.filter(tool => tool.found).length
  const theme = readTheme()
  const [motion, setMotion] = useState(readMotion)
  useEffect(() => {
    const syncMotion = () => setMotion(readMotion())
    window.addEventListener('knowflow:motion', syncMotion)
    return () => window.removeEventListener('knowflow:motion', syncMotion)
  }, [])
  const motionLabel = motion === 'full' ? '完整' : motion === 'reduced' ? '减弱' : '关闭'

  return <section className="extension-page settings-page" aria-labelledby="settings-title">
    <SettingsGearBackdrop activeIndex={currentIndex} />
    <ThornRoseBackdrop />
    <header className="extension-page-heading"><div><span>SETTINGS / CONTROL CENTER</span><h1 id="settings-title">设置</h1><p>按用途管理连接、阅读、外观和本地数据，不再连续滚动整张表单。</p></div><div className="settings-heading-right"><MinimalCyberEye activeIndex={currentIndex} /><div className="save-state">配置保存在本机 · {current.label}</div></div></header>
    <div className="settings-layout">
      <div className="settings-nav-col">
        <nav className="settings-nav" aria-label="设置分组"><span>CONTROL / 08</span>{sections.map(item => <button key={item.id} type="button" className={section === item.id ? 'active' : ''} onClick={() => onSectionChange(item.id)}>{item.label}<small>{item.index}</small></button>)}<div className="settings-nav-foot"><i className="dot" />配置保存在本机</div></nav>
        <SettingsBlueprintPlate activeIndex={currentIndex} sectionCode={current.index} />
        <RevolverCylinderWidget activeIndex={currentIndex} />
      </div>
      <div className="settings-content">
        {section === 'overview' && <>
          <section className="settings-section-head">
            <div><span>OVERVIEW / WORKSPACE</span><h2>工作区概览</h2><p>先确认当前状态，再进入对应分组。关键内容保持在一屏内。</p></div>
            <SettingsOverviewTelemetry
              agentReady={Boolean(agentStatus?.hasKey && agentStatus.model)}
              bridgeConnected={Boolean(bridgeStatus?.connected)}
              toolsReady={readyCount === 4}
              toolsLoading={toolsLoading}
              hasCache={workspace !== null}
            />
          </section>
          <div className="settings-overview-status">
            <OverviewStatus index="01" title="Agent 服务" value={agentStatus?.hasKey && agentStatus.model ? '已配置' : '未配置'} detail={agentStatus?.model || '需要模型 ID 与密钥'} state={agentStatus?.hasKey && agentStatus.model ? 'online' : 'idle'} />
            <OverviewStatus index="02" title="浏览器桥接" value={bridgeStatus?.connected ? '已连接' : '未连接'} detail={bridgeStatus?.error || 'Chrome / Edge 捕获'} state={bridgeStatus?.connected ? 'online' : 'idle'} />
            <OverviewStatus index="03" title="视频工具" value={toolsLoading ? '检测中' : `${readyCount} / 4 可用`} detail={toolsLoading ? '正在检查本机工具' : readyCount === 4 ? '分镜与指定时间取帧' : '存在缺失工具'} state={toolsLoading ? 'idle' : readyCount === 4 ? 'online' : 'warn'} />
            <OverviewStatus index="04" title="本地缓存" value={formatBytes(workspace?.cacheBytes ?? 0)} detail="可查看数据与存储明细" state="online" />
          </div>
          <div className="overview-grid">
            <section className="quick-settings gn-card"><HudCornerBrackets /><div className="overview-section-title"><strong>常用设置</strong><span>04 个分组</span></div>
              <QuickLink index="02" title="Agent 服务" description="模型地址、密钥和工具兼容性测试" status={agentStatus?.hasKey && agentStatus.model ? '已配置' : '未配置'} onClick={() => onSectionChange('agent')} />
              <QuickLink index="03" title="阅读与检索" description="自动阅读、任务预算和语义检索" status="标准预算" onClick={() => onSectionChange('reading')} />
              <QuickLink index="04" title="人格与表达" description="称呼、语气与人格文件管理" status="知识管家" onClick={() => onSectionChange('persona')} />
              <QuickLink index="05" title="界面与动效" description="机械传动、简约黑白和三档动效" status={theme === 'hud' ? '机械传动' : '简约黑白'} onClick={() => onSectionChange('appearance')} accent />
              <div className="current-config"><span>当前生效配置</span><p>{agentStatus?.model || '模型未配置'} · 动效{motionLabel} · 所有设置均保存在本机</p></div>
            </section>
            <aside className="diagnostics-card gn-card"><HudCornerBrackets /><MechanicalGear className="mech-diagnostics-gear" teeth={20} speed="26s" reverse /><div className="overview-section-title"><strong>系统诊断</strong><span>{desktop ? '本机实时' : '预览数据'}</span></div><div className="diagnostic-list">{toolsLoading && tools.length === 0 ? <div className="gn-diag-row"><span className="gn-diag-label"><i className="gn-led gn-led--idle" />正在检测本机工具…</span><strong className="gn-diag-value gn-tabular">稍候</strong></div> : tools.map(tool => <ToolDiagRow key={tool.name} tool={tool} />)}</div><div className="diagnostics-cache"><VernierGauge label="应用缓存" valueText={`${formatBytes(workspace?.cacheBytes ?? 0)} / 512 MB`} value={workspace?.cacheBytes ?? 0} max={512 * 1024 * 1024} hint="以 512MB 为参考刻度，不是配额。数据与导出文件均保存在本机。" /></div><div className="diagnostic-actions"><HudButton variant="primary" type="button" onClick={() => onSectionChange('about')}>查看完整诊断</HudButton><HudButton type="button" onClick={() => { void openPath('data') }}>打开数据目录</HudButton></div></aside>
          </div>
          <div className="overview-footer"><span><i className="dot" />配置已保存到本机</span><span>不会自动清理资料 · 诊断不包含密钥和 Cookie</span></div>
        </>}
        {section === 'agent' && <AgentSetup section="connection" onStatusChange={onAgentStatusChange} />}
        {section === 'reading' && <AgentSetup section="reading" onStatusChange={onAgentStatusChange} />}
        {section === 'persona' && <PersonaSetup />}
        {section === 'appearance' && <ThemeSettings />}
        {section === 'data' && <>
          <section className="settings-section-head"><div><span>LOCAL DATA / STORAGE</span><h2>数据与存储</h2><p>查看真实的数据目录与缓存占用；这里不会自动删除或清理任何内容。</p></div></section>
          <div className="settings-status-grid"><StatusCard title="主数据库" value={formatBytes(workspace?.databaseBytes ?? 0)} detail="knowflow.db" ok /><StatusCard title="应用缓存" value={formatBytes(workspace?.cacheBytes ?? 0)} detail="视频、分镜与工具缓存" ok /></div>
          <section className="form-card"><div className="form-card-title"><strong>存储位置</strong><span>仅打开目录，不修改文件</span></div><label>应用数据目录<code className="path-value">{workspace?.dataDir ?? '请在桌面程序中查看'}</code></label><label>应用缓存目录<code className="path-value">{workspace?.cacheDir ?? '请在桌面程序中查看'}</code></label><div className="form-actions"><HudButton type="button" onClick={() => { void openPath('data') }}>打开数据目录</HudButton><HudButton type="button" onClick={() => { void openPath('cache') }}>打开缓存目录</HudButton><HudButton type="button" onClick={() => { void openPath('exports') }}>打开导出目录</HudButton></div></section>
          <div className="install-rules"><strong>安全边界</strong><ul><li>诊断页面不读取或显示 API 密钥、配对码和 Cookie 内容</li><li>清理缓存、恢复备份等写操作将在带确认与备份保护的独立流程中提供</li><li>不要直接把应用数据目录当作源码目录修改</li></ul></div>
        </>}
        {section === 'shortcuts' && <><section className="settings-section-head"><div><span>KEYBOARD / SHORTCUTS</span><h2>快捷键</h2><p>当前已启用的应用内快捷键。</p></div></section><section className="form-card shortcut-row"><span>查找记录并聚焦右侧搜索</span><kbd>CTRL K</kbd></section></>}
        {section === 'about' && <>
          <section className="settings-section-head"><div><span>ABOUT / DIAGNOSTICS</span><h2>关于与诊断</h2><p>GearNote v{packageInfo.version} · 本地知识工作台</p></div></section>
          <section className="form-card"><div className="form-card-title"><strong>工具诊断</strong><span>只包含非敏感状态</span></div><div className="tool-list">{tools.map(tool => <ToolDiagRow key={tool.name} tool={tool} className="tool-row" />)}</div><div className="form-actions"><HudButton type="button" onClick={() => { void getToolchainStatus().then(setTools) }}>重新检测</HudButton><HudButton type="button" onClick={() => window.open('https://github.com/grvrself/GearNote', '_blank', 'noopener')}>项目仓库</HudButton></div></section>
          <div className="install-rules"><strong>诊断信息边界</strong><p>当前页面仅显示应用版本、工具可用性和版本。不会自动收集或上传使用数据，也不会把 API 密钥、配对码和 Cookie 写入诊断内容。</p></div>
        </>}
        {error && <p className="extension-error" role="alert">{error}</p>}
      </div>
    </div>
  </section>
}

function OverviewStatus({ index, title, value, detail, state = 'online' }: { index: string; title: string; value: string; detail: string; state?: 'online' | 'idle' | 'warn' }) {
  return <div className="overview-status-item gn-card" data-state={state}><i className={`gn-led gn-led--${state}`} /><span>{title}</span><strong className="gn-tabular">{value}</strong><small>{detail}</small><span className="hud-kpi-index gn-tabular" aria-hidden="true">{index}</span><span className="hud-kpi-ruler" aria-hidden="true" /></div>
}

function ToolDiagRow({ tool, className = '' }: { tool: ToolStatus; className?: string }) {
  const raw = tool.found ? tool.version || '已就绪' : tool.error || '未找到'
  const display = tool.found ? formatToolVersion(tool.version) || '已就绪' : raw
  return <div className={`gn-diag-row ${className}`.trim()} data-found={tool.found}>
    <span className="gn-diag-label"><i className={`gn-led ${tool.found ? 'gn-led--online' : 'gn-led--warn'}`} />{tool.name}</span>
    <strong className="gn-diag-value gn-tabular" title={raw}>{display}</strong>
  </div>
}

function QuickLink({ index, title, description, status, onClick, accent = false }: { index: string; title: string; description: string; status: string; onClick: () => void; accent?: boolean }) {
  return <button type="button" onClick={onClick}><span className="quick-index gn-tabular">{index}</span><div><strong>{title}</strong><p>{description}</p></div><span className={`quick-status${accent ? ' accent' : ' ok'}`}>{status}</span><b>→</b></button>
}

function StatusCard({ title, value, detail, ok, onClick }: { title: string; value: string; detail: string; ok?: boolean; onClick?: () => void }) {
  const content = <><i className={`status-dot${ok ? ' ok' : ''}`} /><div><span>{title}</span><strong>{value}</strong><small>{detail}</small></div></>
  return onClick ? <button className="settings-status-card" type="button" onClick={onClick}>{content}</button> : <div className="settings-status-card">{content}</div>
}

function formatBytes(value: number) {
  if (!value) return '0 KB'
  if (value < 1024) return `${value} B`
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`
  if (value < 1024 * 1024 * 1024) return `${(value / 1024 / 1024).toFixed(1)} MB`
  return `${(value / 1024 / 1024 / 1024).toFixed(2)} GB`
}
