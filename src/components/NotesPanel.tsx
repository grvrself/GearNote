import { useEffect, useRef, useState } from 'react'
import type { KnowledgeRecord } from '../data'
import type { Note, NoteDraft, NoteFolder, NoteSourceInput } from '../notes'
import { composeNoteFromSources } from '../notes'
import { useConfirm } from '../hooks/useConfirm'
import { HudButton } from './HudButton'
import { GearIcon } from './GearIcon'
import { PanelHeading } from './PanelHeading'
import { AdvancedNoteStudio } from './AdvancedNoteStudio'
import { NoteReader } from './NoteReader'
import './notes.css'

type Props = {
  notes: Note[]
  folders: NoteFolder[]
  records: KnowledgeRecord[]
  incomingDraft: NoteDraft | null
  onDraftConsumed: () => void
  onSave: (draft: NoteDraft, id?: string) => Promise<Note>
  onDelete: (id: string) => Promise<void>
  onExport: (id: string) => Promise<string>
  onRevealRecord: (id: string, atSeconds?: number) => void
  onApplyEdit: (proposalId: string) => Promise<Note>
  onUndoEdit: (proposalId: string) => Promise<Note>
  onRejectEdit: (proposalId: string) => Promise<void>
  onCreateFolder: (name: string) => Promise<NoteFolder>
  onRenameFolder: (id: string, name: string) => Promise<void>
  onDeleteFolder: (id: string) => Promise<number>
  designOpen?: boolean
  onDesignOpenChange?: (open: boolean) => void
  composeRequest?: { ids: string[]; nonce: number } | null
  onComposeRequestConsumed?: () => void
  requestedNoteId?: string | null
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
const noteDraft = (note: Note): NoteDraft => ({ title: note.title, body: note.body,
  sources: note.sources.map(source => ({ recordId: source.recordId, atSeconds: source.atSeconds })), folderId: note.folderId ?? null })
const notePreview = (note: Note) => note.body.split('\n').map(line => line.trim())
  .find(line => line && !line.startsWith('#') && !line.startsWith('['))?.replace(/^[-*>]\s*/, '').slice(0, 90) ?? '打开笔记查看内容'
export function NotesPanel({ notes, folders, records, incomingDraft, onDraftConsumed, onSave, onDelete, onExport, onRevealRecord, onApplyEdit, onUndoEdit, onRejectEdit, onCreateFolder, onRenameFolder, onDeleteFolder, designOpen: controlledDesignOpen, onDesignOpenChange, composeRequest, onComposeRequestConsumed, requestedNoteId }: Props) {
  const { confirm, confirmDialog } = useConfirm()
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [draft, setDraft] = useState<NoteDraft>(emptyDraft)
  const [viewMode, setViewMode] = useState<'read' | 'edit'>('edit')
  const initiallySelected = useRef(false)
  const [folderFilter, setFolderFilter] = useState('all')
  const [folderMode, setFolderMode] = useState<'create' | 'rename' | null>(null)
  const [folderName, setFolderName] = useState('')
  const [folderError, setFolderError] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [exportPath, setExportPath] = useState('')
  const [designOpenState, setDesignOpenState] = useState(false)
  const setDesignOpen = (open: boolean) => {
    setDesignOpenState(open)
    onDesignOpenChange?.(open)
  }
  const [composeOpen, setComposeOpen] = useState(false)
  const [composeIds, setComposeIds] = useState<string[]>([])
  const [composeFocus, setComposeFocus] = useState('')
  const [composeCoverage, setComposeCoverage] = useState('')
  const [openDesignAfterSave, setOpenDesignAfterSave] = useState(false)
  const saved = notes.find(note => note.id === selectedId)
  const hasUnsavedChanges = Boolean(viewMode === 'edit' && saved && (saved.title !== draft.title || saved.body !== draft.body ||
    (saved.folderId ?? null) !== (draft.folderId ?? null) || JSON.stringify(saved.sources.map(keyOf)) !== JSON.stringify(draft.sources.map(keyOf))))
  const hasNewDraft = !selectedId && Boolean(draft.title.trim() || draft.body.trim() || draft.sources.length)
  const visibleNotes = notes.filter(note => folderFilter === 'all' ||
    (folderFilter === 'uncategorized' ? !note.folderId : note.folderId === folderFilter))
  const selectedFolder = folders.find(folder => folder.id === folderFilter)

  useEffect(() => {
    if (initiallySelected.current || !notes.length || incomingDraft) return
    initiallySelected.current = true
    const first = notes.find(note => folderFilter === 'all' || (folderFilter === 'uncategorized' ? !note.folderId : note.folderId === folderFilter))
    setSelectedId(first?.id ?? null)
    setDraft(first ? noteDraft(first) : emptyDraft(folderForFilter(folderFilter)))
    setViewMode(first ? 'read' : 'edit')
  }, [notes, incomingDraft, folderFilter])

  useEffect(() => {
    if (saved && viewMode === 'read') setDraft(noteDraft(saved))
  }, [saved, viewMode])

  const confirmDiscard = async () => {
    if (!hasUnsavedChanges && !hasNewDraft) return true
    return confirm({ title: '放弃未保存内容', message: '当前笔记有未保存的内容，确定放弃并切换吗？', confirmLabel: '放弃并切换', danger: true })
  }
  useEffect(() => {
    if (controlledDesignOpen === undefined) return
    let cancelled = false
    void (async () => {
      if (controlledDesignOpen && !designOpenState && !await confirmDiscard()) {
        if (!cancelled) onDesignOpenChange?.(false)
        return
      }
      if (!cancelled) setDesignOpenState(controlledDesignOpen)
    })()
    return () => { cancelled = true }
  }, [controlledDesignOpen])
  useEffect(() => {
    if (!composeRequest) return
    let cancelled = false
    void (async () => {
      if (!await confirmDiscard()) { if (!cancelled) onComposeRequestConsumed?.(); return }
      if (cancelled) return
      initiallySelected.current = true
      setDesignOpen(false)
      setComposeOpen(true)
      setComposeIds(composeRequest.ids.slice(0, 3))
      setComposeFocus(''); setComposeCoverage(''); setOpenDesignAfterSave(false)
      setSelectedId(null); setDraft(emptyDraft(folderForFilter(folderFilter)))
      setViewMode('edit'); setError(''); setNotice('')
      onComposeRequestConsumed?.()
    })()
    return () => { cancelled = true }
  }, [composeRequest])
  // 只在 designOpenState（放弃确认通过后）才定位外部请求的笔记，避免确认前覆盖未保存草稿。
  useEffect(() => {
    if (!designOpenState || !requestedNoteId) return
    const note = notes.find(item => item.id === requestedNoteId)
    if (!note) return
    setSelectedId(note.id); setDraft(noteDraft(note)); setViewMode('read')
  }, [designOpenState, requestedNoteId, notes])
  const changeFolderFilter = async (filter: string) => {
    if (filter === folderFilter || !await confirmDiscard()) return
    setDesignOpen(false)
    setComposeOpen(false); setComposeCoverage(''); setOpenDesignAfterSave(false)
    setFolderFilter(filter)
    const first = notes.find(note => filter === 'all' || (filter === 'uncategorized' ? !note.folderId : note.folderId === filter))
    setSelectedId(first?.id ?? null)
    setDraft(first ? noteDraft(first) : emptyDraft(folderForFilter(filter)))
    setViewMode(first ? 'read' : 'edit')
    setError(''); setNotice(''); setExportPath('')
    setFolderMode(null); setFolderError('')
  }
  const newNote = async () => {
    if (!await confirmDiscard()) return
    initiallySelected.current = true
    setDesignOpen(false)
    setComposeOpen(false)
    setComposeCoverage(''); setOpenDesignAfterSave(false)
    setSelectedId(null)
    setDraft(emptyDraft(folderForFilter(folderFilter)))
    setViewMode('edit')
    setError(''); setNotice(''); setExportPath('')
  }
  const startCompose = async () => {
    if (!await confirmDiscard()) return
    initiallySelected.current = true
    setDesignOpen(false); setComposeOpen(true); setComposeIds([]); setComposeFocus(''); setComposeCoverage('')
    setOpenDesignAfterSave(false)
    setSelectedId(null); setDraft(emptyDraft(folderForFilter(folderFilter)))
    setViewMode('edit')
    setError(''); setNotice('')
  }
  const openStudio = async () => {
    if (!await confirmDiscard()) return
    setComposeOpen(false); setComposeCoverage(''); setOpenDesignAfterSave(false)
    setDesignOpen(true); setError(''); setNotice(''); setExportPath('')
  }
  const compose = async () => {
    setBusy(true); setError(''); setNotice('')
    try {
      const composed = await composeNoteFromSources(composeIds, composeFocus)
      setDraft({ title: composed.title, body: composed.body, sources: composed.sources, folderId: folderForFilter(folderFilter) })
      setComposeCoverage(composed.coverage); setComposeOpen(false)
      setOpenDesignAfterSave(true)
      setNotice(composed.warnings.length ? `草稿已保留。${composed.warnings.join('；')}。请核实后保存。` : '已生成待核对草稿。请检查后保存。')
    } catch (cause) { setError(String(cause)) } finally { setBusy(false) }
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
    if (!await confirm({ title: '删除文件夹', message: `删除文件夹“${selectedFolder.name}”？其中 ${count} 篇笔记会移到“未分类”，笔记内容不会删除。`, confirmLabel: '删除', danger: true })) return
    setBusy(true); setFolderError('')
    try {
      const moved = await onDeleteFolder(selectedFolder.id)
      const first = notes.find(note => note.folderId === selectedFolder.id || !note.folderId)
      setFolderFilter('uncategorized'); setSelectedId(first?.id ?? null); setDraft(first ? noteDraft({ ...first, folderId: null }) : emptyDraft()); setViewMode(first ? 'read' : 'edit')
      setFolderMode(null); setNotice(`文件夹已删除，${moved} 篇笔记已移到未分类。`)
    } catch (cause) { setFolderError(String(cause)) } finally { setBusy(false) }
  }

  useEffect(() => {
    if (!incomingDraft) return
    initiallySelected.current = true
    setDesignOpen(false)
    setComposeOpen(false); setComposeCoverage(''); setOpenDesignAfterSave(false)
    setSelectedId(null)
    setDraft({ ...incomingDraft, folderId: incomingDraft.folderId ?? folderForFilter(folderFilter) })
    setViewMode('edit')
    setError('')
    setNotice('请检查并修改草稿，确认后保存。')
    setExportPath('')
    onDraftConsumed()
  }, [incomingDraft, onDraftConsumed, folderFilter])

  const selectNote = async (note: Note) => {
    if ((selectedId !== note.id || hasUnsavedChanges) && !await confirmDiscard()) return
    setDesignOpen(false)
    setComposeOpen(false)
    setComposeCoverage(''); setOpenDesignAfterSave(false)
    setSelectedId(note.id)
    setDraft(noteDraft(note)); setViewMode('read')
    setError(''); setNotice(''); setExportPath('')
  }
  const appendTo = (note: Note) => {
    setDesignOpen(false)
    setComposeOpen(false); setComposeCoverage(''); setOpenDesignAfterSave(false)
    setSelectedId(note.id)
    setViewMode('edit')
    setDraft({
      title: note.title,
      body: [note.body.trim(), draft.body.trim()].filter(Boolean).join('\n\n---\n\n'),
      sources: mergeSources(note.sources, draft.sources),
      folderId: note.folderId ?? null,
    })
    setNotice(`已合并到《${note.title}》的草稿，保存前可继续修改。`)
    setError(''); setExportPath('')
  }
  const save = async () => {
    setBusy(true); setError(''); setNotice('')
    try {
      const saved = await onSave(draft, selectedId ?? undefined)
      setSelectedId(saved.id)
      setDraft(noteDraft(saved))
      setViewMode('read')
      if (folderFilter !== 'all' && (saved.folderId ?? null) !== folderForFilter(folderFilter)) setFolderFilter(saved.folderId ?? 'uncategorized')
      setNotice('已保存到本地笔记。')
      if (openDesignAfterSave) { setDesignOpen(true); setOpenDesignAfterSave(false) }
    } catch (cause) { setError(String(cause)) } finally { setBusy(false) }
  }
  const exportCurrent = async () => {
    if (!selectedId) return
    setBusy(true); setError(''); setExportPath('')
    try { setExportPath(await onExport(selectedId)); setNotice('Markdown 已导出。') }
    catch (cause) { setError(String(cause)) } finally { setBusy(false) }
  }
  const removeCurrent = async () => {
    if (!saved || busy) return
    if (!await confirm({ title: '删除笔记', message: `确定删除笔记《${saved.title}》？这会同时删除它的高级版和笔记截图，且无法撤销。`, confirmLabel: '删除', danger: true })) return
    setBusy(true); setError(''); setNotice('')
    try {
      await onDelete(saved.id)
      const next = visibleNotes.find(note => note.id !== saved.id)
      setSelectedId(next?.id ?? null); setDraft(next ? noteDraft(next) : emptyDraft(folderForFilter(folderFilter)))
      setViewMode(next ? 'read' : 'edit')
      setDesignOpen(false); setComposeCoverage(''); setOpenDesignAfterSave(false); setExportPath('')
      setNotice('笔记已删除。')
    } catch (cause) { setError(String(cause)) } finally { setBusy(false) }
  }
  const sourceTitle = (source: NoteSourceInput) => records.find(record => record.id === source.recordId)?.title
    ?? notes.flatMap(note => note.sources).find(item => keyOf(item) === keyOf(source))?.title ?? '来源已移除'

  // 渲染只看 designOpenState：受控打开须先通过放弃确认，确认前保持编辑器与草稿可见。
  if (designOpenState) return <section className="notes-panel" aria-label="高级笔记工作台"><AdvancedNoteStudio
    notes={notes} records={records} initialNoteId={selectedId} onClose={() => setDesignOpen(false)}
    onSelectNote={note => { setSelectedId(note.id); setDraft(noteDraft(note)); setViewMode('read') }}
    onCreateFromSources={() => { void startCompose() }} onRevealRecord={onRevealRecord}
    onApplyEdit={onApplyEdit} onRejectEdit={onRejectEdit} onUndoEdit={onUndoEdit}
    onNoteChanged={updated => setDraft(noteDraft(updated))} />{confirmDialog}</section>

  return <section className="notes-panel" aria-label="资料库笔记">
    <div className="notes-layout">
      <aside className="notes-index" aria-label="已保存的笔记">
        <PanelHeading id="sec-01-library" serial="SEC-01" overline="library" title="资料库" count={notes.length} />
        <div className="notes-index-scroll">
        <div className="notes-folder-head"><span>文件夹</span><button type="button" disabled={busy} onClick={() => { setFolderMode('create'); setFolderName(''); setFolderError('') }}>＋ 新建</button></div>
        <nav className="notes-folder-list" aria-label="笔记文件夹">
          <button type="button" disabled={busy} aria-current={folderFilter === 'all' ? 'page' : undefined} onClick={() => { void changeFolderFilter('all') }}><span>全部笔记</span><small>{notes.length}</small></button>
          <button type="button" disabled={busy} aria-current={folderFilter === 'uncategorized' ? 'page' : undefined} onClick={() => { void changeFolderFilter('uncategorized') }}><span>未分类</span><small>{notes.filter(note => !note.folderId).length}</small></button>
          {folders.map(folder => <button type="button" key={folder.id} disabled={busy} aria-current={folderFilter === folder.id ? 'page' : undefined} onClick={() => { void changeFolderFilter(folder.id) }}><span>{folder.name}</span><small>{notes.filter(note => note.folderId === folder.id).length}</small></button>)}
        </nav>
        {selectedFolder && <div className="notes-folder-actions"><button type="button" disabled={busy} onClick={() => { setFolderMode('rename'); setFolderName(selectedFolder.name); setFolderError('') }}>重命名</button><button type="button" disabled={busy || hasUnsavedChanges || hasNewDraft} onClick={() => { void removeFolder() }}>删除文件夹</button></div>}
        {folderMode && <form className="notes-folder-form" onSubmit={event => { event.preventDefault(); void submitFolder() }}><label htmlFor="note-folder-name">{folderMode === 'create' ? '新文件夹名称' : '重命名文件夹'}</label><input id="note-folder-name" autoFocus value={folderName} maxLength={60} onChange={event => setFolderName(event.target.value)} /><div><button type="submit" disabled={busy || !folderName.trim()}>保存</button><button type="button" disabled={busy} onClick={() => { setFolderMode(null); setFolderError('') }}>取消</button></div></form>}
        {folderError && <p className="notes-folder-error" role="alert">{folderError}</p>}
        <div className="notes-index-head"><span>笔记 <small>{visibleNotes.length}</small></span></div>
        {visibleNotes.length === 0 && <p className="notes-empty">{notes.length === 0 ? '暂无笔记。可新建笔记或从资料生成草稿。' : '这里还没有笔记。'}</p>}
        {visibleNotes.map(note => <button className="notes-index-item" type="button" key={note.id} disabled={busy} data-selected={selectedId === note.id} onClick={() => { void selectNote(note) }}>
          <span className="notes-index-item-title"><strong>{note.title}</strong><span className="notes-index-item-state"><GearIcon kind="brand" size={20} /><small>{selectedId === note.id && hasUnsavedChanges ? '草稿' : '已保存'}</small></span></span>
          <span className="notes-index-item-preview">{notePreview(note)}</span>
          <small>{new Date(note.updatedAt).toLocaleString('zh-CN')} · {note.sources.length} 个来源</small>
        </button>)}
        </div>
        <div className="notes-index-footer">
          <button className="notes-compose-entry" type="button" disabled={busy} onClick={() => { void openStudio() }}><span>从资料生成高级笔记</span><GearIcon kind="brand" size={16} /></button>
          <button className="notes-new-entry" type="button" disabled={busy} onClick={() => { void newNote() }}>＋ 新建笔记</button>
        </div>
      </aside>
      <div className="notes-workspace">
        <div className="notes-workspace-top"><span>文件夹 {folders.length} <i /> 来源 {viewMode === 'edit' ? draft.sources.length : saved?.sources.length ?? 0}</span><span>{saved ? hasUnsavedChanges ? '未保存的修改' : `已保存 ${new Date(saved.updatedAt).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}` : hasNewDraft ? '尚未保存' : '新建笔记'}</span></div>
        <div className="notes-editor">
        {saved && viewMode === 'read' ? <div className="notes-reading">
          <header className="note-reader-header"><span className="note-reader-kicker"><GearIcon kind="brand" size={16} /> 阅读笔记</span><h2>{saved.title}</h2><div className="note-reader-meta"><time>{new Date(saved.updatedAt).toLocaleString('zh-CN')}</time><span>{saved.sources.length} 个来源</span><span>{saved.body.length.toLocaleString('zh-CN')} 字</span><span>{folders.find(folder => folder.id === saved.folderId)?.name ?? '未分类'}</span></div></header>
          {saved.sources.length > 0 && <section className="note-reader-sources" aria-label="笔记来源"><div className="note-reader-section-label"><GearIcon kind="brand" size={16} /> 来源资料 <span>{saved.sources.length}</span></div><div>{saved.sources.map((source, index) => <button type="button" key={`${keyOf(source)}:${index}`} onClick={() => onRevealRecord(source.recordId, source.atSeconds)} title={source.title}><span>{source.atSeconds == null ? '资料' : timeOf(source.atSeconds)}</span>{source.title}</button>)}</div></section>}
          <section className="note-reader-main" aria-labelledby="note-main-heading"><h3 id="note-main-heading" className="note-reader-section-label"><GearIcon kind="brand" size={16} /> 主要内容</h3><NoteReader note={saved} /></section>
        </div> : <>
        <div className="notes-editor-head"><span>{selectedId ? '编辑笔记' : '笔记草稿'}</span><span>{selectedId ? hasUnsavedChanges ? '未保存的修改' : '已保存' : '未保存'}</span></div>
        {composeOpen && <div className="notes-compose"><strong>选择资料生成草稿</strong><p>最多选择 3 条已完成的资料。模型仅会读取部分相关片段；生成后仍由你检查并保存。</p>
          <div className="notes-compose-records">{records.filter(record => record.status === 'ready' && !record.demo).map(record => <label key={record.id} data-selected={composeIds.includes(record.id)} data-disabled={!composeIds.includes(record.id) && composeIds.length >= 3}>
            <input type="checkbox" checked={composeIds.includes(record.id)} disabled={busy || (!composeIds.includes(record.id) && composeIds.length >= 3)}
              onChange={event => setComposeIds(current => event.target.checked ? [...current, record.id] : current.filter(id => id !== record.id))} /><span>{record.title}</span></label>)}</div>
          <label htmlFor="note-compose-focus">整理重点（可选）</label><input id="note-compose-focus" value={composeFocus} maxLength={300} onChange={event => setComposeFocus(event.target.value)} placeholder="例如：总结操作步骤" />
          <div className="notes-actions notes-compose-actions"><HudButton type="button" variant="primary" disabled={busy || composeIds.length === 0} onClick={() => { void compose() }}>{busy ? '生成中' : '生成待核对草稿'}</HudButton><HudButton type="button" disabled={busy} onClick={() => setComposeOpen(false)}>取消</HudButton></div>
        </div>}
        {composeCoverage && <p className="notes-notice">本次读取范围：{composeCoverage}</p>}
        <label htmlFor="note-title">标题</label>
        <input id="note-title" value={draft.title} maxLength={120} onChange={event => setDraft(current => ({ ...current, title: event.target.value }))} placeholder="输入笔记标题" />
        <label htmlFor="note-folder-select">文件夹</label>
        <select id="note-folder-select" value={draft.folderId ?? ''} onChange={event => setDraft(current => ({ ...current, folderId: event.target.value || null }))}><option value="">未分类</option>{folders.map(folder => <option key={folder.id} value={folder.id}>{folder.name}</option>)}</select>
        <label htmlFor="note-body">正文</label>
        <textarea id="note-body" value={draft.body} onChange={event => setDraft(current => ({ ...current, body: event.target.value }))} placeholder="记录要保留的结论、问题与行动项" />
        <div className="notes-source-head">来源 <span>{draft.sources.length}</span></div>
        <ul className="notes-sources">{draft.sources.map(source => <li key={keyOf(source)}><button type="button" title="查看资料" onClick={() => onRevealRecord(source.recordId, source.atSeconds)}>{sourceTitle(source)}{source.atSeconds == null ? '' : ` · ${timeOf(source.atSeconds)}`}</button><button type="button" aria-label={`移除来源：${sourceTitle(source)}`} onClick={() => setDraft(current => ({ ...current, sources: current.sources.filter(item => keyOf(item) !== keyOf(source)) }))}>×</button></li>)}</ul>
        {!selectedId && draft.body.trim() && notes.length > 0 && <div className="notes-append"><label htmlFor="note-append">追加到已有笔记</label><select id="note-append" disabled={busy} defaultValue="" onChange={event => { const note = notes.find(item => item.id === event.target.value); if (note) appendTo(note) }}><option value="">选择一篇笔记</option>{notes.map(note => <option value={note.id} key={note.id}>{note.title}</option>)}</select></div>}
        {hasUnsavedChanges && <p className="notes-notice">保存修改后可以导出最新内容。</p>}
        </>}
        </div>
        {(error || notice || exportPath) && <div className="notes-workspace-feedback">{error && <p className="notes-error" role="alert">{error}</p>}{notice && <p className="notes-notice" role="status">{notice}</p>}{exportPath && <div className="notes-export"><span>导出文件：{exportPath}</span><button type="button" onClick={() => { void navigator.clipboard.writeText(exportPath).then(() => setNotice('路径已复制。')).catch(() => setError('复制路径失败，请手动选择路径。')) }}>复制路径</button></div>}</div>}
        {!designOpenState && <div className="notes-workspace-actions">{saved && viewMode === 'read' ? <>
          <HudButton type="button" disabled={busy} onClick={() => setViewMode('edit')}>编辑笔记</HudButton>
          <HudButton type="button" disabled={busy} onClick={() => { void exportCurrent() }}>导出 Markdown</HudButton>
          <HudButton type="button" variant="primary" disabled={busy} onClick={() => setDesignOpen(true)}>生成高级版</HudButton>
          <HudButton type="button" disabled={busy} onClick={() => { void removeCurrent() }}>删除笔记</HudButton>
        </> : <>
          <HudButton type="button" variant="primary" disabled={busy || !draft.title.trim() || !draft.body.trim()} onClick={() => { void save() }}>{busy ? '处理中' : '保存笔记'}</HudButton>
          {saved && <HudButton type="button" disabled={busy} onClick={() => { void (async () => { if (!await confirmDiscard()) return; setDraft(noteDraft(saved)); setViewMode('read'); setError(''); setNotice('') })() }}>返回阅读</HudButton>}
        </>}</div>}
      </div>
    </div>
    {confirmDialog}
  </section>
}
