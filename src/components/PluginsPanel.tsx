import { useEffect, useState } from 'react'
import { invoke, isTauri } from '@tauri-apps/api/core'
import type { BridgeStatus, CaptureJob } from '../records'
import { getToolchainStatus, openWorkspacePath, type ToolStatus } from '../extensions'
import { GearIcon } from './GearIcon'
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

  return <section className="extension-page" aria-labelledby="plugins-title">
    <header className="extension-page-heading"><div><span>PLUGIN-01 / CONNECTIONS</span><h1 id="plugins-title">连接与插件</h1><p>管理外部能力、访问权限和每一项实际执行记录。</p></div><div className="extension-page-actions"><button className="text-action" type="button" onClick={() => setProtocolOpen(true)}>插件协议</button><HudButton variant="primary" type="button" onClick={onGoSkills}>安装技能包</HudButton></div><GearIcon kind="center" size={20} /></header>
    <div className="connection-bus" aria-hidden="true"><MechanicalGear teeth={12} speed="10s" /><i /><MechanicalGear teeth={16} speed="14s" reverse /><i /><MechanicalGear teeth={12} speed="10s" /><i /><MechanicalGear teeth={16} speed="14s" reverse /><span>LIVE BUS</span></div>
    <div className="health-grid"><div className={`health-card${status?.connected ? ' ok' : ' warning'}`}><span className="health-icon">{status?.connected ? '✓' : '!'}</span><div><strong>{status?.error ? '浏览器桥接异常' : status?.connected ? '浏览器桥接正常' : '等待浏览器扩展连接'}</strong><span>{status?.connected ? '本机心跳正常' : '检查桌面程序与扩展配对'}</span></div></div><div className={`health-card${readyCount === 4 ? ' ok' : ' warning'}`}><span className="health-icon">{readyCount === 4 ? '✓' : '!'}</span><div><strong>视频工具 {readyCount} / 4 可用</strong><span>Node / FFmpeg / ffprobe / yt-dlp</span></div></div><div className="health-card"><span className="health-icon">i</span><div><strong>第三方代码默认不执行</strong><span>安装、启用和权限变更均需确认</span></div></div></div>
    <div className="extension-toolbar"><div className="extension-tabs">{([['all', '全部连接'], ['official', '官方'], ['community', '社区']] as const).map(([id, label]) => <button key={id} className={filter === id ? 'active' : ''} type="button" onClick={() => { setFilter(id); if (id === 'community') select('community'); else if (active === 'community') select('browser') }}>{label}</button>)}</div><span className="toolbar-note">官方连接器 {tools.length ? '02' : '01'} · 社区包 00</span></div>
    <div className="plugin-layout">
      <div className="plugin-list">
        {filter !== 'community' && <>
          <button className={`plugin-item${active === 'browser' ? ' selected' : ''}`} type="button" onClick={() => select('browser')}><GearIcon kind="brand" size={16} /><div><strong>网页与字幕桥接</strong><span>浏览器捕获 · 官方</span></div><i className={`status-dot${status?.connected ? ' ok' : ''}`} /></button>
          <button className={`plugin-item${active === 'tools' ? ' selected' : ''}`} type="button" onClick={() => select('tools')}><GearIcon kind="working" size={16} /><div><strong>视频工具箱</strong><span>分镜与取帧 · 官方</span></div><i className={`status-dot${readyCount === 4 ? ' ok' : ''}`} /></button>
        </>}
        {filter !== 'official' && <button className={`plugin-item${active === 'community' ? ' selected' : ''}`} type="button" onClick={() => select('community')}><GearIcon kind="brand" size={16} /><div><strong>社区连接器</strong><span>声明式协议 · 下一阶段</span></div><span className="extension-badge">未开放</span></button>}
        <div className="plugin-list-note"><strong>权限不是界面装饰</strong><p>文件、网络、模型与进程访问都由后端现有命令约束。第三方 JavaScript 和可执行文件目前不会运行。</p></div>
      </div>
      <aside className="extension-detail">
        {active === 'browser' && <>
          <span className="detail-eyebrow">OFFICIAL CONNECTOR / BROWSER</span><div className="detail-title"><GearIcon kind="brand" size={20} /><div><h2>网页与字幕桥接</h2><span>GearNote 浏览器扩展 · v0.2.3</span></div><span className={`status-pill${status?.connected ? ' ok' : ''}`}>{status?.connected ? '已连接' : '未连接'}</span></div>
          <p className="detail-description">只有点击浏览器工具栏图标时，才会把当前文章或播放器已有字幕保存到这台电脑。</p>
          {status?.error && <p className="extension-error" role="alert">{status.error}</p>}
          <div className="connection-grid"><div><span>连接地址</span><strong className="mono">127.0.0.1:45147</strong></div><div><span>心跳状态</span><strong>{status?.connected ? '连接正常 · 30 秒周期' : '尚未检测到扩展'}</strong></div><div><span>浏览器</span><strong>Chrome / Edge</strong></div><div><span>来源</span><strong>随应用安装 · 官方</strong></div></div>
          <section className="pair-box"><div><span>本机配对码</span><code>{desktop && status?.pairCode ? `${status.pairCode.slice(0, 8)}••••••••••••••••••••••••` : '请在桌面程序中查看'}</code></div><HudButton type="button" disabled={!status?.pairCode} onClick={() => { void copyCode() }}>复制</HudButton></section>
          <section className="permission-box compact"><strong>允许与限制</strong><ul><li>✓ 接收你主动捕获的文章和字幕</li><li>✓ 保存到本机资料库</li><li>× 不读取浏览历史</li><li>× 不上传浏览器 Cookie</li></ul></section>
          <div className="detail-actions"><HudButton variant="primary" type="button" onClick={() => { void openFolder() }}>打开扩展文件夹</HudButton></div>
          <section className="run-history"><h3>最近捕获任务</h3>{captureJobs.length === 0 ? <p>当前没有采集任务。</p> : captureJobs.slice(0, 4).map(job => <div key={job.id}><i className="dot" /><span>{job.title}</span><small>{job.status === 'ready' ? '成功' : job.status === 'fault' ? job.faultReason || '失败' : '处理中'}</small></div>)}</section>
        </>}
        {active === 'tools' && <>
          <span className="detail-eyebrow">OFFICIAL CONNECTOR / VIDEO</span><div className="detail-title"><GearIcon kind="working" size={20} /><div><h2>视频工具箱</h2><span>分镜、下载与指定时间取帧</span></div><span className={`status-pill${readyCount === 4 ? ' ok' : ''}`}>{readyCount} / 4 可用</span></div>
          <p className="detail-description">所有命令都通过既有工具路径和受限参数执行；这里显示的是本机真实检测结果。</p>
          <div className="tool-list">{tools.map(tool => <div className="tool-row" key={tool.name}><i className={`status-dot${tool.found ? ' ok' : ''}`} /><div><strong>{tool.name}</strong><span>{tool.found ? tool.version || '版本已确认' : tool.error || '未找到'}</span><code>{tool.path}</code></div></div>)}</div>
          <section className="permission-box compact"><strong>允许与限制</strong><ul><li>✓ 处理你指定的视频与本地缓存</li><li>✓ 调用经过模板限制的媒体命令</li><li>× 不开放任意 shell、路径或参数</li></ul></section>
          <div className="detail-actions"><HudButton variant="primary" type="button" disabled={busy} onClick={() => { void refresh() }}>{busy ? '检测中…' : '重新检测'}</HudButton><HudButton type="button" onClick={() => { void openPath('cache') }}>打开缓存</HudButton><HudButton type="button" onClick={() => { void openPath('exports') }}>打开导出目录</HudButton></div>
        </>}
        {active === 'community' && <div className="community-placeholder"><GearIcon kind="center" size={20} /><span className="detail-eyebrow">COMMUNITY / CONNECTOR SCHEMA</span><h2>连接器协议尚未开放执行</h2><p>这一版已经支持从 GitHub 安装声明式技能包；第三方连接器和原生插件仍不会执行。后续会先开放域名、输入和输出都可验证的声明式连接器，再评估代码沙箱。</p><div className="install-rules"><strong>当前可用</strong><ul><li>声明式技能的安装、权限校验与运行</li><li>官方浏览器桥接与视频工具的诊断</li></ul><strong>暂不开放</strong><ul><li>第三方 JavaScript、EXE、DLL 和安装脚本</li><li>任意网络、文件、shell 或自动更新</li></ul></div><HudButton variant="primary" type="button" onClick={onGoSkills}>查看可用的社区技能</HudButton></div>}
        {message && <p className="extension-message" role="status">{message}</p>}{error && <p className="extension-error" role="alert">{error}</p>}
      </aside>
    </div>
    {protocolOpen && <div className="extension-modal-backdrop" role="presentation" onClick={event => { if (event.target === event.currentTarget) setProtocolOpen(false) }}><section className="extension-modal" role="dialog" aria-modal="true" aria-labelledby="plugin-protocol-title"><div className="extension-modal-head"><div><span>PLUGIN MANIFEST / DRAFT</span><h2 id="plugin-protocol-title">插件协议与当前边界</h2></div><button type="button" onClick={() => setProtocolOpen(false)}>×</button></div><div className="install-rules"><strong>技能包（已支持）</strong><p><code>gearnote-skill</code>：JSON + Markdown 提示词，只允许选择资料、模型问答和会话写入。</p><strong>声明式连接器（下一阶段）</strong><p><code>gearnote-connector</code>：计划支持精确域名、受限资料写入、输入输出 Schema，不允许任意代码。</p><strong>原生代码插件（未开放）</strong><p>普通 Node 子进程不等于安全沙箱。官方审核与用户信任提示不会替代权限隔离。</p></div><div className="extension-modal-actions"><HudButton type="button" onClick={() => setProtocolOpen(false)}>关闭</HudButton></div></section></div>}
  </section>
}
