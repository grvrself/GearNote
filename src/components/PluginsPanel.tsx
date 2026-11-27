import { useEffect, useState } from 'react'
import { invoke, isTauri } from '@tauri-apps/api/core'
import type { BridgeStatus, CaptureJob } from '../records'
import { getToolchainStatus, openWorkspacePath, type ToolStatus } from '../extensions'
import { formatToolVersion } from '../utils/format'
import { PluginHudBackdrop, PocketWatchBackdrop } from './mechanical'
import { MechanicalGear } from './MechanicalGear'
import { HudButton } from './HudButton'
import './extension-workspace.css'

type Props = { status: BridgeStatus | null; captureJobs: CaptureJob[]; onGoSkills: () => void }

export function PluginsPanel({ status, captureJobs, onGoSkills }: Props) {
  const desktop = isTauri()
  const [active, setActive] = useState<'browser' | 'tools' | 'community'>('browser')
  const [filter, setFilter] = useState<'all' | 'official' | 'community'>('all')
  const [tools, setTools] = useState<ToolStatus[]>([])
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState('')
  const [error, setError] = useState('')
  const [protocolOpen, setProtocolOpen] = useState(false)

  const refresh = async () => {
    if (!desktop) return
    setBusy(true); setError('')
    try { setTools(await getToolchainStatus()); setMessage('已重新检测本机工具') }
    catch (cause) { setError(String(cause)) }
    finally { setBusy(false) }
  }
  useEffect(() => { void refresh() }, [desktop])

  const copyCode = async () => {
    if (!status?.pairCode) return
    try { await navigator.clipboard.writeText(status.pairCode); setMessage('配对码已复制；请勿分享给其他人。') }
    catch { setError('复制失败，请手动选择配对码。') }
  }
  const openFolder = async () => {
    try { await invoke('open_extension_folder'); setMessage('已打开扩展文件夹') }
    catch (cause) { setError(String(cause)) }
  }
  const openPath = async (kind: 'data' | 'cache' | 'exports') => {
    try { await openWorkspacePath(kind) }
    catch (cause) { setError(String(cause)) }
  }
  const readyCount = tools.filter(tool => tool.found).length
  const select = (id: typeof active) => { setActive(id); setMessage(''); setError('') }

  const busTracks = [
    { label: status?.connected ? 'TX // 30S' : 'TX // WAIT', value: status?.connected ? '127.0.0.1' : 'OFFLINE' },
    { label: 'IPC // PORT', value: '45147' },
    { label: 'TOOLS', value: `0${readyCount} / 04` },
  ]

  return <section className="extension-page plugins-page" aria-labelledby="plugins-title">
    <header className="extension-page-heading"><div><span>PLUGIN-01 / CONNECTIONS</span><h1 id="plugins-title">连接与插件</h1><p>管理外部能力、访问权限和每一项实际执行记录。</p></div><div className="extension-page-actions"><button className="text-action" type="button" onClick={() => setProtocolOpen(true)}>插件协议</button><HudButton variant="primary" type="button" onClick={onGoSkills}>安装技能包</HudButton></div><PocketWatchBackdrop connected={Boolean(status?.connected)} /><PluginHudBackdrop active={busy || Boolean(status?.connected)} /></header>
    <div className="connection-bus" data-live={Boolean(status?.connected)} aria-hidden="true">
      <MechanicalGear teeth={12} speed="10s" />
      <BusTrackSegment label={busTracks[0].label} value={busTracks[0].value} />
      <MechanicalGear teeth={16} speed="14s" reverse />
      <BusTrackSegment label={busTracks[1].label} value={busTracks[1].value} />
      <MechanicalGear teeth={12} speed="10s" />
      <BusTrackSegment label={busTracks[2].label} value={busTracks[2].value} />
      <MechanicalGear teeth={16} speed="14s" reverse />
      <span>LIVE BUS</span>
    </div>
    <div className="health-grid">
      <div className={`health-card gn-card health-card--bridge${status?.connected ? ' ok' : ' warning'}`}>
        <i className={`gn-led ${status?.connected ? 'gn-led--online' : 'gn-led--warn'}`} aria-hidden="true" />
        <span className="health-icon">{status?.connected ? '✓' : '!'}</span>
        <div><strong>{status?.error ? '浏览器桥接异常' : status?.connected ? '浏览器桥接正常' : '等待浏览器扩展连接'}</strong><span>{status?.connected ? '本机心跳正常' : '检查桌面程序与扩展配对'}</span></div>
        <span className={`hud-heartbeat-bars${status?.connected ? ' is-live' : ''}`} aria-hidden="true"><i /><i className="cyan" /><i /><i className="cyan" /><i /><i /></span>
      </div>
      <div className={`health-card gn-card health-card--tools${readyCount === 4 ? ' ok' : ' warning'}`}>
        {readyCount < 4 && <i className="hud-warn-bar" aria-hidden="true" />}
        <i className={`gn-led ${readyCount === 4 ? 'gn-led--online' : 'gn-led--warn'}`} aria-hidden="true" />
        <span className="health-icon">{readyCount === 4 ? '✓' : <><span className="hud-legacy-glyph">!</span><svg className="hud-warn-triangle" viewBox="0 0 24 22" aria-hidden="true"><path d="M12 1.5L23 20.5H1L12 1.5Z" fill="#FCEE0A" /><path d="M12 8v6.2M12 16.5v1.6" stroke="#07080A" strokeWidth="2.1" strokeLinecap="round" /></svg></>}</span>
        <div><strong className="gn-tabular">视频工具 {readyCount} / 4 可用</strong><span>Node / FFmpeg / ffprobe / yt-dlp</span></div>
        {readyCount < 4 && <span className="hud-warn-counter" aria-hidden="true">! {4 - readyCount}</span>}
      </div>
      <div className="health-card gn-card health-card--policy">
        <i className="gn-led gn-led--idle" aria-hidden="true" />
        <span className="health-icon">i</span>
        <div><strong>第三方代码默认不执行</strong><span>安装、启用和权限变更均需确认</span></div>
      </div>
    </div>
    <div className="extension-toolbar"><div className="extension-tabs">{([['all', '全部连接'], ['official', '官方'], ['community', '社区']] as const).map(([id, label]) => <button key={id} className={filter === id ? 'active' : ''} type="button" onClick={() => { setFilter(id); if (id === 'community') select('community'); else if (active === 'community') select('browser') }}>{label}</button>)}</div><span className="toolbar-note">官方连接器 {tools.length ? '02' : '01'} · 社区包 00</span></div>
    <div className="plugin-layout">
      <div className="plugin-list">
        {filter !== 'community' && <>
          <button className={`plugin-item gn-card${active === 'browser' ? ' selected gn-card--active' : ''}`} type="button" onClick={() => select('browser')}><span className="plugin-index gn-tabular" aria-hidden="true">01</span><div><strong>网页与字幕桥接</strong><span>浏览器捕获 · 官方</span></div><span className={`hud-tri-dots${status?.connected ? ' ok' : ''}`} aria-hidden="true"><i /><i /><i /></span><i className={`status-dot${status?.connected ? ' ok' : ''}`} /></button>
          <button className={`plugin-item gn-card${active === 'tools' ? ' selected gn-card--active' : ''}`} type="button" onClick={() => select('tools')}><span className="plugin-index gn-tabular" aria-hidden="true">02</span><div><strong>视频工具箱</strong><span>分镜与取帧 · 官方</span></div><span className={`hud-tri-dots${readyCount === 4 ? ' ok' : ''}`} aria-hidden="true"><i /><i /><i /></span><i className={`status-dot${readyCount === 4 ? ' ok' : ''}`} /></button>
        </>}
        {filter !== 'official' && <button className={`plugin-item gn-card${active === 'community' ? ' selected gn-card--active' : ''}`} type="button" onClick={() => select('community')}><span className="plugin-index gn-tabular" aria-hidden="true">03</span><div><strong>社区连接器</strong><span>声明式协议 · 下一阶段</span></div><span className="extension-badge">未开放</span></button>}
        <div className="plugin-list-note"><strong>权限不是界面装饰</strong><p>文件、网络、模型与进程访问都由后端现有命令约束。第三方 JavaScript 和可执行文件目前不会运行。</p></div>
      </div>
      <aside className="extension-detail">
        <span className="hud-detail-scan-tag" aria-hidden="true">{status?.connected ? 'ACTIVE' : 'STANDBY'}</span>
        {active === 'browser' && <>
          <span className="detail-eyebrow">OFFICIAL CONNECTOR / BROWSER</span><div className="detail-title"><span className="detail-index gn-tabular" aria-hidden="true">01</span><div><h2>网页与字幕桥接</h2><span>GearNote 浏览器扩展 · v0.2.3</span></div><span className={`status-pill${status?.connected ? ' ok' : ''}`}>{status?.connected ? '已连接' : '未连接'}</span></div>
          <p className="detail-description">只有点击浏览器工具栏图标时，才会把当前文章或播放器已有字幕保存到这台电脑。</p>
          {status?.error && <p className="extension-error" role="alert">{status.error}</p>}
          <div className="connection-grid"><div><span>连接地址</span><strong className="mono gn-tabular">127.0.0.1:45147</strong><svg className="hud-cell-glyph" viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M8 1.5L13.5 3.8V8.1C13.5 11.4 11.2 13.8 8 14.8C4.8 13.8 2.5 11.4 2.5 8.1V3.8L8 1.5Z" stroke="currentColor" strokeWidth="1.2" /><path d="M5.8 8.2L7.3 9.7L10.3 6.7" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" /></svg></div><div><span>心跳状态</span><strong className="gn-tabular">{status?.connected ? '连接正常 · 30 秒周期' : '尚未检测到扩展'}</strong><svg className="hud-cell-glyph" viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M1.5 8.5H4.2L5.9 4.2L8.1 12L10.1 6.6L11.5 8.5H14.5" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" /></svg></div><div><span>浏览器</span><strong className="gn-tabular">Chrome / Edge</strong></div><div><span>来源</span><strong className="gn-tabular">随应用安装 · 官方</strong></div></div>
          <section className="pair-box gn-dot-grid"><div><span>本机配对码</span><code>{desktop && status?.pairCode ? `${status.pairCode.slice(0, 8)}••••••••••••••••••••••••` : '请在桌面程序中查看'}</code></div><HudButton type="button" disabled={!status?.pairCode} onClick={() => { void copyCode() }}>复制</HudButton></section>
          <section className="permission-box compact"><strong>允许与限制</strong><ul><li className="perm-allow">✓ 接收你主动捕获的文章和字幕</li><li className="perm-allow">✓ 保存到本机资料库</li><li className="perm-deny">× 不读取浏览历史</li><li className="perm-deny">× 不上传浏览器 Cookie</li></ul></section>
          <div className="detail-actions"><HudButton className="gn-connector-action" variant="primary" type="button" onClick={() => { void openFolder() }}>打开扩展文件夹</HudButton></div>
          <section className="run-history"><h3>最近捕获任务</h3>{captureJobs.length === 0 ? <p>当前没有采集任务。</p> : captureJobs.slice(0, 4).map(job => <div key={job.id}><i className="dot" /><span>{job.title}</span><small>{job.status === 'ready' ? '成功' : job.status === 'fault' ? job.faultReason || '失败' : '处理中'}</small></div>)}</section>
        </>}
        {active === 'tools' && <>
          <span className="detail-eyebrow">OFFICIAL CONNECTOR / VIDEO</span><div className="detail-title"><span className="detail-index gn-tabular" aria-hidden="true">02</span><div><h2>视频工具箱</h2><span>分镜、下载与指定时间取帧</span></div><span className={`status-pill gn-tabular${readyCount === 4 ? ' ok' : ''}`}>{readyCount} / 4 可用</span></div>
          <p className="detail-description">所有命令都通过既有工具路径和受限参数执行；这里显示的是本机真实检测结果。</p>
          <div className="tool-list">{tools.map(tool => {
            const raw = tool.found ? tool.version || '版本已确认' : tool.error || '未找到'
            const display = tool.found ? formatToolVersion(tool.version) || '版本已确认' : raw
            return <div className="tool-row gn-diag-row" key={tool.name} data-found={tool.found}><span className="gn-diag-label"><i className={`gn-led ${tool.found ? 'gn-led--online' : 'gn-led--warn'}`} aria-hidden="true" />{tool.name}</span><strong className="gn-diag-value gn-tabular" title={tool.path ? `${raw}\n${tool.path}` : raw}>{display}</strong></div>
          })}</div>
          <section className="permission-box compact"><strong>允许与限制</strong><ul><li className="perm-allow">✓ 处理你指定的视频与本地缓存</li><li className="perm-allow">✓ 调用经过模板限制的媒体命令</li><li className="perm-deny">× 不开放任意 shell、路径或参数</li></ul></section>
          <div className="detail-actions"><HudButton className="gn-connector-action" variant="primary" type="button" disabled={busy} onClick={() => { void refresh() }}>{busy ? '检测中…' : '重新检测'}</HudButton><HudButton type="button" onClick={() => { void openPath('cache') }}>打开缓存</HudButton><HudButton type="button" onClick={() => { void openPath('exports') }}>打开导出目录</HudButton></div>
        </>}
        {active === 'community' && <div className="community-placeholder"><span className="detail-eyebrow">COMMUNITY / CONNECTOR SCHEMA</span><h2>连接器协议尚未开放执行</h2><p>这一版已经支持从 GitHub 安装声明式技能包；第三方连接器和原生插件仍不会执行。后续会先开放域名、输入和输出都可验证的声明式连接器，再评估代码沙箱。</p><div className="install-rules"><strong>当前可用</strong><ul><li>声明式技能的安装、权限校验与运行</li><li>官方浏览器桥接与视频工具的诊断</li></ul><strong>暂不开放</strong><ul><li>第三方 JavaScript、EXE、DLL 和安装脚本</li><li>任意网络、文件、shell 或自动更新</li></ul></div><HudButton className="gn-connector-action" variant="primary" type="button" onClick={onGoSkills}>查看可用的社区技能</HudButton></div>}
        {message && <p className="extension-message" role="status">{message}</p>}{error && <p className="extension-error" role="alert">{error}</p>}
      </aside>
    </div>
    {protocolOpen && <div className="extension-modal-backdrop" role="presentation" onClick={event => { if (event.target === event.currentTarget) setProtocolOpen(false) }}><section className="extension-modal" role="dialog" aria-modal="true" aria-labelledby="plugin-protocol-title"><div className="extension-modal-head"><div><span>PLUGIN MANIFEST / DRAFT</span><h2 id="plugin-protocol-title">插件协议与当前边界</h2></div><button type="button" onClick={() => setProtocolOpen(false)}>×</button></div><div className="install-rules"><strong>技能包（已支持）</strong><p><code>gearnote-skill</code>：JSON + Markdown 提示词，只允许选择资料、模型问答和会话写入。</p><strong>声明式连接器（下一阶段）</strong><p><code>gearnote-connector</code>：计划支持精确域名、受限资料写入、输入输出 Schema，不允许任意代码。</p><strong>原生代码插件（未开放）</strong><p>普通 Node 子进程不等于安全沙箱。官方审核与用户信任提示不会替代权限隔离。</p></div><div className="extension-modal-actions"><HudButton type="button" onClick={() => setProtocolOpen(false)}>关闭</HudButton></div></section></div>}
  </section>
}

function BusTrackSegment({ label, value }: { label: string; value: string }) {
  return <div className="hud-bus-segment">
    <div className="hud-bus-telemetry"><span>{label}</span><b>{value}</b></div>
    <svg className="hud-bus-track" viewBox="0 0 200 12" preserveAspectRatio="none" focusable="false">
      <line x1="0" y1="6" x2="200" y2="6" stroke="rgba(252, 238, 10, 0.45)" strokeWidth="1" strokeDasharray="4 4" />
      <line className="bus-cyan-pulse" x1="0" y1="6" x2="200" y2="6" stroke="#00F0FF" strokeWidth="1.6" strokeDasharray="24 176" strokeLinecap="round" />
    </svg>
  </div>
}
