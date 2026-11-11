import { useEffect, useState } from 'react'
import type { KnowledgeRecord } from '../data'
import type { Note, NoteDraft, NoteEditProposal, NoteFolder, NoteSourceInput, NoteWorkflowEvent } from '../notes'
import { HudButton } from './HudButton'
import { PanelHeading } from './PanelHeading'
import './notes.css'

type Props = {
  notes: Note[]
  folders: NoteFolder[]
  records: KnowledgeRecord[]
  incomingDraft: NoteDraft | null
  onDraftConsumed: () => void
  onSave: (draft: NoteDraft, id?: string) => Promise<Note>
  onExport: (id: string) => Promise<string>
  onRevealRecord: (id: string, atSeconds?: number) => void
  onProposeEdit: (noteId: string, instruction: string) => Promise<NoteEditProposal>
  onApplyEdit: (proposalId: string) => Promise<Note>
  onUndoEdit: (proposalId: string) => Promise<Note>
  onRejectEdit: (proposalId: string) => Promise<void>
  onListWorkflowEvents: (noteId: string) => Promise<NoteWorkflowEvent[]>
  onGetPendingEdit: (noteId: string) => Promise<NoteEditProposal | null>
  onCreateFolder: (name: string) => Promise<NoteFolder>
  onRenameFolder: (id: string, name: string) => Promise<void>
  onDeleteFolder: (id: string) => Promise<number>
}

const emptyDraft = (folderId: string | null = null): NoteDraft => ({ title: '', body: '', sources: [], folderId })
const folderForFilter = (filter: string) => filter === 'all' || filter === 'uncategorized' ? null : filter
const keyOf = (source: NoteSourceInput) => `${source.recordId}:${source.atSeconds ?? ''}`
const mergeSources = (left: NoteSourceInput[], right: NoteSourceInput[]) => {
  const merged = [...left]
  right.forEach(source => { if (!merged.some(item => keyOf(item) === keyOf(source))) merged.push(source) })
  return merged
}
const timeOf = (seconds: number) => `${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`
const latestUndoable = (events: NoteWorkflowEvent[]) => events.find(event => event.undoable)?.proposalId ?? null

export function NotesPanel({ notes, folders, records, incomingDraft, onDraftConsumed, onSave, onExport, onRevealRecord, onProposeEdit, onApplyEdit, onUndoEdit, onRejectEdit, onListWorkflowEvents, onGetPendingEdit, onCreateFolder, onRenameFolder, onDeleteFolder }: Props) {
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [draft, setDraft] = useState<NoteDraft>(emptyDraft)
  const [folderFilter, setFolderFilter] = useState('all')
  const [folderMode, setFolderMode] = useState<'create' | 'rename' | null>(null)
  const [folderName, setFolderName] = useState('')
  const [folderError, setFolderError] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [exportPath, setExportPath] = useState('')
  const [editInstruction, setEditInstruction] = useState('')
  const [proposal, setProposal] = useState<NoteEditProposal | null>(null)
  const [lastAppliedId, setLastAppliedId] = useState<string | null>(null)
  const [workflowEvents, setWorkflowEvents] = useState<NoteWorkflowEvent[]>([])
  const saved = notes.find(note => note.id === selectedId)
  const hasUnsavedChanges = Boolean(saved && (saved.title !== draft.title || saved.body !== draft.body ||
    (saved.folderId ?? null) !== (draft.folderId ?? null) || JSON.stringify(saved.sources.map(keyOf)) !== JSON.stringify(draft.sources.map(keyOf))))
  const hasNewDraft = !selectedId && Boolean(draft.title.trim() || draft.body.trim() || draft.sources.length)
  const visibleNotes = notes.filter(note => folderFilter === 'all' ||
    (folderFilter === 'uncategorized' ? !note.folderId : note.folderId === folderFilter))
  const selectedFolder = folders.find(folder => folder.id === folderFilter)

  const confirmDiscard = () => !hasUnsavedChanges && !hasNewDraft || window.confirm('当前笔记有未保存的内容，确定放弃并切换吗？')
  const changeFolderFilter = (filter: string) => {
    if (filter === folderFilter || !confirmDiscard()) return
    setFolderFilter(filter)
    setSelectedId(null)
    setDraft(emptyDraft(folderForFilter(filter)))
    setProposal(null); setLastAppliedId(null); setError(''); setNotice(''); setExportPath('')
    setFolderMode(null); setFolderError('')
  }
  const newNote = () => {
    if (!confirmDiscard()) return
    setSelectedId(null)
    setDraft(emptyDraft(folderForFilter(folderFilter)))
    setError(''); setNotice(''); setExportPath(''); setProposal(null); setLastAppliedId(null)
  }
  const submitFolder = async () => {
    if (!folderMode || !folderName.trim()) return
    setBusy(true); setFolderError('')
    try {
      if (folderMode === 'create') await onCreateFolder(folderName)
      else if (selectedFolder) await onRenameFolder(selectedFolder.id, folderName)
      setFolderMode(null); setFolderName('')
    } catch (cause) { setFolderError(String(cause)) } finally { setBusy(false) }
  }
  const removeFolder = async () => {
    if (!selectedFolder || hasUnsavedChanges || hasNewDraft) return
    const count = notes.filter(note => note.folderId === selectedFolder.id).length
    if (!window.confirm(`删除文件夹“${selectedFolder.name}”？其中 ${count} 篇笔记会移到“未分类”，笔记内容不会删除。`)) return
    setBusy(true); setFolderError('')
    try {
      const moved = await onDeleteFolder(selectedFolder.id)
      setFolderFilter('uncategorized'); setSelectedId(null); setDraft(emptyDraft()); setProposal(null); setLastAppliedId(null)
      setFolderMode(null); setNotice(`文件夹已删除，${moved} 篇笔记已移到未分类。`)
    } catch (cause) { setFolderError(String(cause)) } finally { setBusy(false) }
  }

  useEffect(() => {
    if (!selectedId) { setWorkflowEvents([]); return }
    let active = true
    void Promise.all([onListWorkflowEvents(selectedId), onGetPendingEdit(selectedId)])
      .then(([events, pending]) => { if (active) { setWorkflowEvents(events); setLastAppliedId(latestUndoable(events)); setProposal(pending) } })
      .catch(() => { if (active) { setWorkflowEvents([]); setLastAppliedId(null); setProposal(null) } })
    return () => { active = false }
  }, [selectedId])

  const refreshEvents = async (noteId: string) => {
    const events = await onListWorkflowEvents(noteId)
    setWorkflowEvents(events)
    setLastAppliedId(latestUndoable(events))
  }

  useEffect(() => {
    if (!incomingDraft) return
    setSelectedId(null)
    setDraft({ ...incomingDraft, folderId: incomingDraft.folderId ?? folderForFilter(folderFilter) })
    setError('')
    setNotice('请检查并修改草稿，确认后保存。')
    setExportPath('')
    setProposal(null)
    setLastAppliedId(null)
    setEditInstruction('')
    onDraftConsumed()
  }, [incomingDraft, onDraftConsumed, folderFilter])

  const selectNote = (note: Note) => {
    if ((selectedId !== note.id || hasUnsavedChanges) && !confirmDiscard()) return
    setSelectedId(note.id)
    setDraft({ title: note.title, body: note.body, sources: note.sources.map(source => ({ recordId: source.recordId, atSeconds: source.atSeconds })), folderId: note.folderId ?? null })
    setError(''); setNotice(''); setExportPath('')
    setProposal(proposal?.noteId === note.id ? proposal : null); setLastAppliedId(null); setEditInstruction('')
  }
  const appendTo = (note: Note) => {
    setSelectedId(note.id)
    setDraft({
      title: note.title,
      body: [note.body.trim(), draft.body.trim()].filter(Boolean).join('\n\n---\n\n'),
      sources: mergeSources(note.sources, draft.sources),
      folderId: note.folderId ?? null,
    })
    setNotice(`已合并到《${note.title}》的草稿，保存前可继续修改。`)
    setError(''); setExportPath('')
    setProposal(null); setLastAppliedId(null)
  }
  const save = async () => {
    setBusy(true); setError(''); setNotice('')
    try {
      if (proposal) { await onRejectEdit(proposal.id); setProposal(null) }
      const saved = await onSave(draft, selectedId ?? undefined)
      setSelectedId(saved.id)
      setDraft({ title: saved.title, body: saved.body, sources: saved.sources, folderId: saved.folderId ?? null })
      if (folderFilter !== 'all' && (saved.folderId ?? null) !== folderForFilter(folderFilter)) setFolderFilter(saved.folderId ?? 'uncategorized')
      setLastAppliedId(null)
      setNotice('已保存到本地笔记。')
      await refreshEvents(saved.id)
    } catch (cause) { setError(String(cause)) } finally { setBusy(false) }
  }
  const exportCurrent = async () => {
    if (!selectedId) return
    setBusy(true); setError(''); setExportPath('')
    try { setExportPath(await onExport(selectedId)); setNotice('Markdown 已导出。') }
    catch (cause) { setError(String(cause)) } finally { setBusy(false) }
  }
  const generateEdit = async () => {
    if (!selectedId || proposal) return
    setBusy(true); setError(''); setNotice(''); setLastAppliedId(null)
    try {
      setProposal(await onProposeEdit(selectedId, editInstruction))
      await refreshEvents(selectedId)
    } catch (cause) { setError(String(cause)) } finally { setBusy(false) }
  }
  const applyEdit = async () => {
    if (!proposal || proposal.noteId !== selectedId) return
    setBusy(true); setError('')
    try {
      const saved = await onApplyEdit(proposal.id)
      setDraft({ title: saved.title, body: saved.body, sources: saved.sources, folderId: saved.folderId ?? null })
      setLastAppliedId(proposal.id)
      setProposal(null)
      setNotice('已按确认的建议修改笔记，可撤销这次操作。')
      await refreshEvents(selectedId)
    } catch (cause) { setError(String(cause)); await refreshEvents(selectedId).catch(() => {}) }
    finally { setBusy(false) }
  }
  const rejectEdit = async () => {
    if (!proposal || proposal.noteId !== selectedId) return
    setBusy(true); setError('')
    try { await onRejectEdit(proposal.id); setProposal(null); setNotice('已放弃建议，笔记未修改。'); await refreshEvents(selectedId) }
    catch (cause) { setError(String(cause)) } finally { setBusy(false) }
  }
  const undoEdit = async () => {
    if (!lastAppliedId || !selectedId) return
    setBusy(true); setError('')
    try {
      const restored = await onUndoEdit(lastAppliedId)
      setDraft({ title: restored.title, body: restored.body, sources: restored.sources, folderId: restored.folderId ?? null })
      setLastAppliedId(null)
      setNotice('已撤销这次修改。')
      await refreshEvents(selectedId)
    } catch (cause) { setError(String(cause)) } finally { setBusy(false) }
  }
  const sourceTitle = (source: NoteSourceInput) => records.find(record => record.id === source.recordId)?.title
    ?? notes.flatMap(note => note.sources).find(item => keyOf(item) === keyOf(source))?.title ?? '来源已移除'

  return <section className="notes-panel" aria-label="资料库笔记">
    <PanelHeading id="sec-01-library" serial="SEC-01" overline="library" title="资料库 / 笔记" count={notes.length} />
    <div className="notes-layout">
      <aside className="notes-index" aria-label="已保存的笔记">
        <div className="notes-folder-head"><span>文件夹</span><button type="button" disabled={busy} onClick={() => { setFolderMode('create'); setFolderName(''); setFolderError('') }}>＋ 新建</button></div>
        <nav className="notes-folder-list" aria-label="笔记文件夹">
          <button type="button" disabled={busy} aria-current={folderFilter === 'all' ? 'page' : undefined} onClick={() => changeFolderFilter('all')}><span>全部笔记</span><small>{notes.length}</small></button>
          <button type="button" disabled={busy} aria-current={folderFilter === 'uncategorized' ? 'page' : undefined} onClick={() => changeFolderFilter('uncategorized')}><span>未分类</span><small>{notes.filter(note => !note.folderId).length}</small></button>
          {folders.map(folder => <button type="button" key={folder.id} disabled={busy} aria-current={folderFilter === folder.id ? 'page' : undefined} onClick={() => changeFolderFilter(folder.id)}><span>{folder.name}</span><small>{notes.filter(note => note.folderId === folder.id).length}</small></button>)}
        </nav>
        {selectedFolder && <div className="notes-folder-actions"><button type="button" disabled={busy} onClick={() => { setFolderMode('rename'); setFolderName(selectedFolder.name); setFolderError('') }}>重命名</button><button type="button" disabled={busy || hasUnsavedChanges || hasNewDraft} onClick={() => { void removeFolder() }}>删除文件夹</button></div>}
        {folderMode && <form className="notes-folder-form" onSubmit={event => { event.preventDefault(); void submitFolder() }}><label htmlFor="note-folder-name">{folderMode === 'create' ? '新文件夹名称' : '重命名文件夹'}</label><input id="note-folder-name" autoFocus value={folderName} maxLength={60} onChange={event => setFolderName(event.target.value)} /><div><button type="submit" disabled={busy || !folderName.trim()}>保存</button><button type="button" disabled={busy} onClick={() => { setFolderMode(null); setFolderError('') }}>取消</button></div></form>}
        {folderError && <p className="notes-folder-error" role="alert">{folderError}</p>}
        <div className="notes-index-head"><span>笔记 <small>{visibleNotes.length}</small></span><button type="button" disabled={busy} onClick={newNote}>＋ 新建笔记</button></div>
        {visibleNotes.length === 0 && <p className="notes-empty">{notes.length === 0 ? '暂无笔记。分析资料后可从回答中生成草稿。' : '这里还没有笔记。'}</p>}
        {visibleNotes.map(note => <button className="notes-index-item" type="button" key={note.id} disabled={busy} data-selected={selectedId === note.id} onClick={() => selectNote(note)}>
          <strong>{note.title}</strong><small>{new Date(note.updatedAt).toLocaleString('zh-CN')} · {note.sources.length} 个引用</small>
        </button>)}
      </aside>
      <div className="notes-editor">
        <div className="notes-editor-head"><span>{selectedId ? '编辑笔记' : '笔记草稿'}</span><span>{selectedId ? '已保存' : '未保存'}</span></div>
        <label htmlFor="note-title">标题</label>
        <input id="note-title" value={draft.title} maxLength={120} onChange={event => setDraft(current => ({ ...current, title: event.target.value }))} placeholder="输入笔记标题" />
        <label htmlFor="note-folder-select">文件夹</label>
        <select id="note-folder-select" value={draft.folderId ?? ''} onChange={event => setDraft(current => ({ ...current, folderId: event.target.value || null }))}><option value="">未分类</option>{folders.map(folder => <option key={folder.id} value={folder.id}>{folder.name}</option>)}</select>
        <label htmlFor="note-body">正文</label>
        <textarea id="note-body" value={draft.body} onChange={event => setDraft(current => ({ ...current, body: event.target.value }))} placeholder="记录要保留的结论、问题与行动项" />
        <div className="notes-source-head">来源 <span>{draft.sources.length}</span></div>
        <ul className="notes-sources">{draft.sources.map(source => <li key={keyOf(source)}><button type="button" title="查看资料" onClick={() => onRevealRecord(source.recordId, source.atSeconds)}>{sourceTitle(source)}{source.atSeconds == null ? '' : ` · ${timeOf(source.atSeconds)}`}</button><button type="button" aria-label={`移除来源：${sourceTitle(source)}`} onClick={() => setDraft(current => ({ ...current, sources: current.sources.filter(item => keyOf(item) !== keyOf(source)) }))}>×</button></li>)}</ul>
        {!selectedId && draft.body.trim() && notes.length > 0 && <div className="notes-append"><label htmlFor="note-append">追加到已有笔记</label><select id="note-append" disabled={busy} defaultValue="" onChange={event => { const note = notes.find(item => item.id === event.target.value); if (note) appendTo(note) }}><option value="">选择一篇笔记</option>{notes.map(note => <option value={note.id} key={note.id}>{note.title}</option>)}</select></div>}
        {error && <p className="notes-error" role="alert">{error}</p>}
        {notice && <p className="notes-notice" role="status">{notice}</p>}
        {exportPath && <div className="notes-export"><span>导出文件：{exportPath}</span><button type="button" onClick={() => { void navigator.clipboard.writeText(exportPath).then(() => setNotice('路径已复制。')).catch(() => setError('复制路径失败，请手动选择路径。')) }}>复制路径</button></div>}
        {hasUnsavedChanges && <p className="notes-notice">保存修改后可以导出最新内容。</p>}
        <div className="notes-actions"><HudButton type="button" variant="primary" disabled={busy || !draft.title.trim() || !draft.body.trim()} onClick={() => { void save() }}>{busy ? '处理中' : '保存笔记'}</HudButton><HudButton type="button" disabled={busy || !selectedId || hasUnsavedChanges} onClick={() => { void exportCurrent() }}>导出 Markdown</HudButton></div>
        {selectedId && <details className="note-workflow"><summary>Agent 修改建议与操作记录</summary><p>生成建议会把笔记标题、正文和修改要求发给当前配置的模型。模型只提出一处正文修改；核对原文与新文字后，点击确认才会写入。标题和来源不会由模型修改。</p><label htmlFor="note-edit-instruction">修改要求</label><textarea id="note-edit-instruction" value={editInstruction} maxLength={500} onChange={event => setEditInstruction(event.target.value)} placeholder="例如：把第二段改得更清楚，不增加新事实" /><HudButton type="button" disabled={busy || !!proposal || hasUnsavedChanges || !editInstruction.trim()} onClick={() => { void generateEdit() }}>{busy ? '处理中' : '生成一处建议'}</HudButton>{hasUnsavedChanges && <p className="notes-notice">请先保存或放弃当前手动修改，再生成建议。</p>}{proposal?.noteId === selectedId && <div className="note-proposal"><strong>{proposal.mode === 'append' ? '末尾追加' : '替换一处原文'}</strong><p>{proposal.reason}</p><div><span>原文位置</span><pre>{proposal.beforeText}</pre></div><div><span>建议文字</span><pre>{proposal.afterText}</pre></div><div className="notes-actions"><HudButton type="button" variant="primary" disabled={busy || hasUnsavedChanges} onClick={() => { void applyEdit() }}>确认并写入</HudButton><HudButton type="button" disabled={busy} onClick={() => { void rejectEdit() }}>放弃建议</HudButton></div></div>}{lastAppliedId && <HudButton type="button" disabled={busy || hasUnsavedChanges} onClick={() => { void undoEdit() }}>撤销本次修改</HudButton>}<div className="note-workflow-events"><strong>最近操作</strong>{workflowEvents.length === 0 ? <p>暂无记录</p> : workflowEvents.map(event => <p key={event.id}><time>{new Date(Number(event.at)).toLocaleString('zh-CN')}</time> · {event.detail}</p>)}</div></details>}
      </div>
    </div>
  </section>
}
