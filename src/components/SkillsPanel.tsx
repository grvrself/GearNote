import { useEffect, useMemo, useState } from 'react'
import { isTauri } from '@tauri-apps/api/core'
import type { KnowledgeRecord } from '../data'
import type { Note } from '../notes'
import { analyzeVideoStoryboard } from '../records'
import { getSkillPrompt, installSkillFromGithub, listInstalledSkills, uninstallSkill, type SkillManifest } from '../extensions'
import { useConfirm } from '../hooks/useConfirm'
import { GearIcon } from './GearIcon'
import { MechanicalGear } from './MechanicalGear'
import { CircuitBoardBackdrop, ExtrudedGear3D, GearWatermark } from './mechanical'
import { HudButton } from './HudButton'
import './extension-workspace.css'

type OfficialId = 'summary' | 'deep-read' | 'compose' | 'advanced' | 'storyboard'
type OfficialSkill = { id: OfficialId; name: string; description: string; input: string; cost: string; version: string }
type Props = {
  records: KnowledgeRecord[]
  notes: Note[]
  agentAvailable: boolean
  onRunQuestion: (question: string, recordIds: string[], deepMode: boolean) => void
  onCompose: (recordIds: string[]) => void
  onOpenStudio: (noteId: string) => void
  onRecordsChanged: () => Promise<void>
  onRevealRecord: (id: string) => void
  onOpenAgentSetup: () => void
}

const officialSkills: OfficialSkill[] = [
  { id: 'summary', name: '资料总结', description: '按结论、依据和行动项整理本轮参考资料。', input: '1–8 份资料', cost: '模型调用', version: '内置 · v1.0' },
  { id: 'deep-read', name: '完整阅读', description: '按阅读预算处理长文与字幕，随时停止并回答。', input: '长文 / 长视频', cost: '可停止', version: '内置 · v1.0' },
  { id: 'compose', name: '生成普通笔记', description: '最多选择三份资料，生成可核对的笔记草稿。', input: '1–3 份资料', cost: '保存前确认', version: '内置 · v1.0' },
  { id: 'advanced', name: '生成高级笔记', description: '保留引用和事实边界，重新组织笔记版式。', input: '一篇已有笔记', cost: '预览后保存', version: '内置 · v1.0' },
  { id: 'storyboard', name: '视频分镜', description: '下载可访问的视频，生成带时间索引的分镜图。', input: '一条视频记录', cost: '无需模型', version: '本地执行' },
]

export function SkillsPanel({ records, notes, agentAvailable, onRunQuestion, onCompose, onOpenStudio, onRecordsChanged, onRevealRecord, onOpenAgentSetup }: Props) {
  const { confirm, confirmDialog } = useConfirm()
  const desktop = isTauri()
  const [active, setActive] = useState('official:summary')
  const [filter, setFilter] = useState<'all' | 'official' | 'community'>('all')
  const [query, setQuery] = useState('')
  const [installed, setInstalled] = useState<SkillManifest[]>([])
  const [selectedIds, setSelectedIds] = useState<string[]>([])
  const [selectedNoteId, setSelectedNoteId] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [message, setMessage] = useState('')
  const [installOpen, setInstallOpen] = useState(false)
  const [installUrl, setInstallUrl] = useState('https://github.com/')
  const [installing, setInstalling] = useState(false)
  const [installError, setInstallError] = useState('')

  const refreshInstalled = async () => {
    if (!desktop) return
    try { setInstalled(await listInstalledSkills()) }
    catch (cause) { setError(`已安装技能读取失败：${String(cause)}`) }
  }
  useEffect(() => { void refreshInstalled() }, [desktop])
  useEffect(() => {
    if (!selectedNoteId && notes[0]) setSelectedNoteId(notes[0].id)
  }, [notes, selectedNoteId])

  const official = officialSkills.find(skill => active === `official:${skill.id}`)
  const community = installed.find(skill => active === `community:${skill.id}@${skill.version}`)
  const activeName = official?.name ?? community?.name ?? ''
  const visibleOfficial = officialSkills.filter(skill => filter !== 'community' && `${skill.name}${skill.description}`.toLowerCase().includes(query.toLowerCase()))
  const visibleCommunity = installed.filter(skill => filter !== 'official' && `${skill.name}${skill.description}${skill.author}`.toLowerCase().includes(query.toLowerCase()))
  const availableRecords = useMemo(() => records.filter(record => record.status === 'ready' && !record.demo), [records])
  const selectedRecords = availableRecords.filter(record => selectedIds.includes(record.id))
  const required = official?.id === 'compose' ? { min: 1, max: 3 } : official?.id === 'storyboard' ? { min: 1, max: 1 } : official?.id === 'advanced' ? { min: 0, max: 0 } : community?.inputs[0] ?? { min: 1, max: 8 }
  const needsModel = official?.id !== 'storyboard'
  const hasRecords = selectedRecords.length >= required.min && selectedRecords.length <= required.max
  const skillIndex = (key: string) => {
    const officialIndex = officialSkills.findIndex(skill => key === `official:${skill.id}`)
    if (officialIndex >= 0) return String(officialIndex + 1).padStart(2, '0')
    const communityIndex = installed.findIndex(skill => key === `community:${skill.id}@${skill.version}`)
    return communityIndex >= 0 ? String(officialSkills.length + communityIndex + 1).padStart(2, '0') : '00'
  }
  const canRun = !busy && (!needsModel || agentAvailable) && (official?.id === 'advanced' ? Boolean(selectedNoteId) : hasRecords)

  const selectSkill = (key: string) => {
    setActive(key); setSelectedIds([]); setError(''); setMessage('')
  }
  const toggleRecord = (record: KnowledgeRecord) => {
    setError(''); setMessage('')
    setSelectedIds(current => {
      if (current.includes(record.id)) return current.filter(id => id !== record.id)
      if (current.length >= required.max) {
        setError(`该技能最多选择 ${required.max} 份资料`)
        return current
      }
      if (official?.id === 'storyboard' && record.type !== 'video') {
        setError('视频分镜只能选择视频记录')
        return current
      }
      return [...current, record.id]
    })
  }

  const run = async () => {
    setError(''); setMessage('')
    if (!desktop) { setError('请在 Tauri 桌面程序中运行技能；浏览器预览不会执行任务。'); return }
    if (needsModel && !agentAvailable) { setError('该技能需要模型服务，请先完成 Agent 配置。'); return }
    if (official?.id === 'advanced') {
      if (!selectedNoteId) { setError('请先新建一篇笔记，再使用高级排版。'); return }
      onOpenStudio(selectedNoteId)
      return
    }
    if (!hasRecords) { setError(`该技能需要选择 ${required.min}–${required.max} 份可用资料。`); return }
    if (official?.id === 'summary') {
      onRunQuestion('请总结本轮参考资料的核心内容，并标注来源。', selectedIds, false)
    } else if (official?.id === 'deep-read') {
      onRunQuestion('请完整阅读本轮参考资料，提炼关键结论、依据和未覆盖的部分，并标注来源。', selectedIds, selectedRecords.length === 1 && selectedRecords[0].type === 'video')
    } else if (official?.id === 'compose') {
      onCompose(selectedIds)
    } else if (official?.id === 'storyboard') {
      setBusy(true)
      try {
        const result = await analyzeVideoStoryboard(selectedIds[0])
        setMessage(`分镜已生成：${result.imageCount} 张图片，点击记录可查看。`)
        await onRecordsChanged()
        onRevealRecord(selectedIds[0])
      } catch (cause) { setError(String(cause)) }
      finally { setBusy(false) }
    } else if (community) {
      setBusy(true)
      try {
        const prompt = await getSkillPrompt(community.id, community.version)
        onRunQuestion(`${prompt}\n\n请仅依据本轮选中的资料执行上述技能要求。`, selectedIds, false)
      } catch (cause) { setError(String(cause)) }
      finally { setBusy(false) }
    }
  }

  const install = async () => {
    setInstalling(true); setInstallError('')
    try {
      const skill = await installSkillFromGithub(installUrl)
      await refreshInstalled()
      setActive(`community:${skill.id}@${skill.version}`)
      setInstallOpen(false)
      setMessage(`《${skill.name}》已通过校验并安装；不会自动运行。`)
    } catch (cause) { setInstallError(String(cause)) }
    finally { setInstalling(false) }
  }

  const remove = async (skill: SkillManifest) => {
    if (!await confirm({ title: '卸载技能包', message: `卸载《${skill.name}》${skill.version}？不会删除它产生的对话或笔记。`, confirmLabel: '卸载', danger: true })) return
    setBusy(true); setError('')
    try {
      await uninstallSkill(skill.id, skill.version)
      await refreshInstalled()
      setActive('official:summary')
      setMessage('技能包已卸载，已有内容保持不变。')
    } catch (cause) { setError(String(cause)) }
    finally { setBusy(false) }
  }

  return <section className="extension-page skills-page" aria-labelledby="skills-title" data-busy={busy}>
    <header className="extension-page-heading"><div><span>SKILL-01 / WORKFLOWS</span><h1 id="skills-title">技能工作台</h1><p>把已有能力组合成可复用、可审查的工作流。</p></div><div className="extension-page-actions"><button className="text-action" type="button" onClick={() => window.open('https://github.com/grvrself/GearNote', '_blank', 'noopener')}>发布与贡献</button><HudButton variant="primary" type="button" onClick={() => { setInstallError(''); setInstallOpen(true) }}>从 GitHub 安装</HudButton></div><GearWatermark active={busy || installing} /><CircuitBoardBackdrop /></header>
    <div className="extension-stats"><div><span>官方技能</span><strong className="gn-tabular">{String(officialSkills.length).padStart(2, '0')}</strong></div><div><span>社区技能</span><strong className="gn-tabular">{String(installed.length).padStart(2, '0')}</strong></div><div><span>运行中</span><strong className="gn-tabular">{busy ? '01' : '00'}</strong></div><div className="hud-stats-led-cell" data-running={busy} aria-hidden="true"><i className="hud-stats-led" /></div><p>所有技能都显示输入与权限；安装包不会自动执行。</p></div>
    <div className="extension-toolbar"><div className="extension-tabs">{([['all', '内置与已安装'], ['official', '官方'], ['community', '社区']] as const).map(([id, label]) => <button key={id} className={filter === id ? 'active' : ''} type="button" onClick={() => setFilter(id)}>{label}</button>)}</div><label className="extension-search"><input value={query} onChange={event => setQuery(event.target.value)} placeholder="搜索技能或作者" /></label></div>
    <div className="skills-layout">
      <div className="skill-grid">
        {visibleOfficial.map(skill => <article className={`skill-card gn-card${active === `official:${skill.id}` ? ' selected gn-card--active' : ''}`} key={skill.id}>
          <div className="skill-card-top"><span className="skill-index gn-tabular">{skillIndex(`official:${skill.id}`)}</span><span className="extension-badge">官方</span></div><h3>{skill.name}</h3><p>{skill.description}</p><div className="skill-meta"><span>{skill.input}</span><span>{skill.cost}</span></div><div className="skill-card-foot"><span>{skill.version}</span><button type="button" onClick={() => selectSkill(`official:${skill.id}`)}>使用 →</button></div>
        </article>)}
        {visibleCommunity.map(skill => <article className={`skill-card gn-card${active === `community:${skill.id}@${skill.version}` ? ' selected gn-card--active' : ''}`} key={`${skill.id}@${skill.version}`}>
          <div className="skill-card-top"><span className="skill-index gn-tabular">{skillIndex(`community:${skill.id}@${skill.version}`)}</span><span className="extension-badge">GitHub</span></div><h3>{skill.name}</h3><p>{skill.description}</p><div className="skill-meta"><span>{skill.inputs[0] ? `${skill.inputs[0].min}–${skill.inputs[0].max} 份资料` : '直接提问'}</span><span>社区包</span></div><div className="skill-card-foot"><span>v{skill.version} · 已安装</span><button type="button" onClick={() => selectSkill(`community:${skill.id}@${skill.version}`)}>使用 →</button></div>
        </article>)}
        {filter === 'community' && visibleCommunity.length === 0 && <div className="extension-empty"><strong>还没有社区技能</strong><p>从 GitHub 安装一个声明式技能包，或按贡献指南发布自己的技能。</p><HudButton type="button" onClick={() => setInstallOpen(true)}>安装第一个技能</HudButton></div>}
      </div>
      <aside className="extension-detail gn-dot-grid">
        <ExtrudedGear3D />
        <MechanicalGear className="mech-detail-watermark" teeth={20} speed="32s" reverse />
        <span className="detail-eyebrow">{skillIndex(active)} // {community ? 'COMMUNITY PACKAGE' : 'OFFICIAL SKILL'}</span>
        <div className="detail-title"><span className="detail-gear" data-running={busy || undefined}><GearIcon kind="brand" size={20} /></span><div><h2>{activeName}</h2><span>{community ? `${community.author} · ${community.homepage}` : '随 GearNote 提供'}</span></div></div>
        <p className="detail-description">{official?.description ?? community?.description}</p>
        {official?.id === 'advanced' ? <label className="detail-field">选择笔记<select value={selectedNoteId} onChange={event => setSelectedNoteId(event.target.value)}><option value="">请选择</option>{notes.map(note => <option value={note.id} key={note.id}>{note.title}</option>)}</select></label> : <RecordPicker records={availableRecords} selectedIds={selectedIds} onToggle={toggleRecord} max={required.max} />}
        <section className="permission-box"><strong>{community ? '此技能可以' : '执行边界'}</strong>{community ? <ul>{community.permissions.map(permission => <li key={permission}>✓ {permissionLabel(permission)}</li>)}</ul> : <ul><li>✓ 只使用你在上方选择的输入</li><li>{official?.id === 'storyboard' ? '✓ 仅调用本地视频工具' : '✓ 调用现有受控功能'}</li><li>× 不获得额外文件或命令权限</li></ul>}</section>
        {needsModel && !agentAvailable && <p className="extension-warning">该技能需要模型服务。<button type="button" onClick={onOpenAgentSetup}>前往配置 →</button></p>}
        <div className="detail-actions"><HudButton variant="primary" className={`gn-skill-run${canRun ? ' gn-skill-run--ready' : ''}`} type="button" disabled={busy} onClick={() => { void run() }}><span>{busy ? '执行中…' : official?.id === 'advanced' ? '打开高级工作台' : '使用技能'}</span><span className="hud-switch-cap" aria-hidden="true"><GearIcon kind="brand" size={16} /></span></HudButton>{community && <HudButton type="button" disabled={busy} onClick={() => { void remove(community) }}>卸载</HudButton>}</div>
        {error && <p className="extension-error" role="alert">{error}</p>}{message && <p className="extension-message" role="status">{message}</p>}
      </aside>
    </div>
    {installOpen && <div className="extension-modal-backdrop" role="presentation" onClick={event => { if (event.target === event.currentTarget && !installing) setInstallOpen(false) }}><section className="extension-modal" role="dialog" aria-modal="true" aria-labelledby="install-skill-title">
      <div className="extension-modal-head"><div><span>GITHUB PACKAGE / VALIDATE</span><h2 id="install-skill-title">从 GitHub 安装技能</h2></div><button type="button" disabled={installing} onClick={() => setInstallOpen(false)}>×</button></div>
      <label>仓库地址<input value={installUrl} disabled={installing} onChange={event => setInstallUrl(event.target.value)} spellCheck={false} placeholder="https://github.com/owner/repository" /></label>
      <div className="install-rules"><strong>下载后由本机 Rust 后端检查</strong><ul><li>仅接受声明式 <code>gearnote-skill</code> 包，不下载或执行代码</li><li>仅允许读取选中资料、调用模型、写入当前会话</li><li>提示词必须是包内相对 Markdown 路径，拒绝路径穿越</li><li>校验通过才写入应用数据目录；安装和更新不会自动运行</li></ul></div>
      {installError && <p className="extension-error" role="alert">{installError}</p>}
      <div className="extension-modal-actions"><HudButton type="button" disabled={installing} onClick={() => setInstallOpen(false)}>取消</HudButton><HudButton variant="primary" type="button" disabled={installing || !installUrl.trim()} onClick={() => { void install() }}>{installing ? '正在下载与校验…' : '校验并安装'}</HudButton></div>
    </section></div>}
    {confirmDialog}
  </section>
}

function RecordPicker({ records, selectedIds, onToggle, max }: { records: KnowledgeRecord[]; selectedIds: string[]; onToggle: (record: KnowledgeRecord) => void; max: number }) {
  return <div className="record-picker"><div className="record-picker-head"><strong>选择输入资料</strong><span className="gn-tabular">{selectedIds.length} / {max}</span></div>{records.length === 0 ? <p>暂无可用于技能的资料，请先在浏览器中捕获文章或字幕。</p> : <div className="record-picker-list">{records.map(record => <label key={record.id} data-selected={selectedIds.includes(record.id)}><input type="checkbox" checked={selectedIds.includes(record.id)} onChange={() => onToggle(record)} /><span>{record.title}</span></label>)}</div>}</div>
}

function permissionLabel(permission: string) {
  if (permission === 'records.read.selected') return '读取你主动选择的资料'
  if (permission === 'agent.chat') return '调用当前配置的模型服务'
  if (permission === 'chat.write') return '把回答写入当前会话'
  return permission
}
