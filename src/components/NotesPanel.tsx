import { useEffect, useState } from 'react'
import type { KnowledgeRecord } from '../data'
import type { Note, NoteDraft, NoteFolder, NoteSourceInput } from '../notes'
import { composeNoteFromSources } from '../notes'
import { HudButton } from './HudButton'
import { PanelHeading } from './PanelHeading'
import { AdvancedNotePanel } from './AdvancedNotePanel'
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
export function NotesPanel({ notes, folders, records, incomingDraft, onDraftConsumed, onSave, onDelete, onExport, onRevealRecord, onApplyEdit, onUndoEdit, onRejectEdit, onCreateFolder, onRenameFolder, onDeleteFolder }: Props) {
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
  const [designOpen, setDesignOpen] = useState(false)
  const [composeOpen, setComposeOpen] = useState(false)
  const [composeIds, setComposeIds] = useState<string[]>([])
  const [composeFocus, setComposeFocus] = useState('')
  const [composeCoverage, setComposeCoverage] = useState('')
  const [openDesignAfterSave, setOpenDesignAfterSave] = useState(false)
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
    setDesignOpen(false)
    setComposeOpen(false); setComposeCoverage(''); setOpenDesignAfterSave(false)
    setFolderFilter(filter)
    setSelectedId(null)
    setDraft(emptyDraft(folderForFilter(filter)))
    setError(''); setNotice(''); setExportPath('')
    setFolderMode(null); setFolderError('')
  }
  const newNote = () => {
    if (!confirmDiscard()) return
    setDesignOpen(false)
    setComposeOpen(false)
    setComposeCoverage(''); setOpenDesignAfterSave(false)
    setSelectedId(null)
    setDraft(emptyDraft(folderForFilter(folderFilter)))
    setError(''); setNotice(''); setExportPath('')
  }
  const startCompose = () => {
    if (!confirmDiscard()) return
    setDesignOpen(false); setComposeOpen(true); setComposeIds([]); setComposeFocus(''); setComposeCoverage('')
    setOpenDesignAfterSave(false)
    setSelectedId(null); setDraft(emptyDraft(folderForFilter(folderFilter)))
    setError(''); setNotice('')
  }
  const compose = async () => {
    setBusy(true); setError(''); setNotice('')
    try {
      const composed = await composeNoteFromSources(composeIds, composeFocus)
      setDraft({ title: composed.title, body: composed.body, sources: composed.sources, folderId: folderForFilter(folderFilter) })
      setComposeCoverage(composed.coverage); setComposeOpen(false)
      setOpenDesignAfterSave(true)
      setNotice('已生成待核对草稿。模型只读取列出的片段；请逐句检查引文，再保存笔记。')
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
    if (!window.confirm(`删除文件夹“${selectedFolder.name}”？其中 ${count} 篇笔记会移到“未分类”，笔记内容不会删除。`)) return
    setBusy(true); setFolderError('')
    try {
      const moved = await onDeleteFolder(selectedFolder.id)
      setFolderFilter('uncategorized'); setSelectedId(null); setDraft(emptyDraft())
      setFolderMode(null); setNotice(`文件夹已删除，${moved} 篇笔记已移到未分类。`)
    } catch (cause) { setFolderError(String(cause)) } finally { setBusy(false) }
  }

  useEffect(() => {
    if (!incomingDraft) return
    setDesignOpen(false)
    setComposeOpen(false); setComposeCoverage(''); setOpenDesignAfterSave(false)
    setSelectedId(null)
    setDraft({ ...incomingDraft, folderId: incomingDraft.folderId ?? folderForFilter(folderFilter) })
    setError('')
    setNotice('请检查并修改草稿，确认后保存。')
    setExportPath('')
    onDraftConsumed()
  }, [incomingDraft, onDraftConsumed, folderFilter])

  const selectNote = (note: Note) => {
    if ((selectedId !== note.id || hasUnsavedChanges) && !confirmDiscard()) return
    setDesignOpen(false)
    setComposeCoverage(''); setOpenDesignAfterSave(false)
    setSelectedId(note.id)
    setDraft({ title: note.title, body: note.body, sources: note.sources.map(source => ({ recordId: source.recordId, atSeconds: source.atSeconds })), folderId: note.folderId ?? null })
    setError(''); setNotice(''); setExportPath('')
  }
  const appendTo = (note: Note) => {
    setDesignOpen(false)
    setComposeOpen(false); setComposeCoverage(''); setOpenDesignAfterSave(false)
    setSelectedId(note.id)
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
      setDraft({ title: saved.title, body: saved.body, sources: saved.sources, folderId: saved.folderId ?? null })
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
    if (!window.confirm(`确定删除笔记《${saved.title}》？这会同时删除它的高级版和笔记截图，且无法撤销。`)) return
    setBusy(true); setError(''); setNotice('')
    try {
      await onDelete(saved.id)
      setSelectedId(null); setDraft(emptyDraft(folderForFilter(folderFilter)))
      setDesignOpen(false); setComposeCoverage(''); setOpenDesignAfterSave(false); setExportPath('')
      setNotice('笔记已删除。')
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
        <button className="notes-compose-entry" type="button" disabled={busy} onClick={startCompose}>从资料生成高级笔记</button>
        {visibleNotes.length === 0 && <p className="notes-empty">{notes.length === 0 ? '暂无笔记。分析资料后可从回答中生成草稿。' : '这里还没有笔记。'}</p>}
        {visibleNotes.map(note => <button className="notes-index-item" type="button" key={note.id} disabled={busy} data-selected={selectedId === note.id} onClick={() => selectNote(note)}>
          <strong>{note.title}</strong><small>{new Date(note.updatedAt).toLocaleString('zh-CN')} · {note.sources.length} 个引用</small>
        </button>)}
      </aside>
      <div className="notes-editor">
        <div className="notes-editor-head"><span>{selectedId ? '编辑笔记' : '笔记草稿'}</span><span>{selectedId ? '已保存' : '未保存'}</span></div>
        {designOpen && saved ? <AdvancedNotePanel note={saved} records={records} onClose={() => setDesignOpen(false)} onRevealRecord={onRevealRecord}
          onApplyEdit={onApplyEdit} onRejectEdit={onRejectEdit} onUndoEdit={onUndoEdit}
          onNoteChanged={updated => setDraft({ title: updated.title, body: updated.body, sources: updated.sources, folderId: updated.folderId ?? null })} /> : <>
        {composeOpen && <div className="notes-compose"><strong>选择资料生成草稿</strong><p>最多选择 3 条已完成的资料。模型仅会读取部分相关片段；生成后仍由你检查并保存。</p>
          <div className="notes-compose-records">{records.filter(record => record.status === 'ready' && !record.demo).map(record => <label key={record.id}>
            <input type="checkbox" checked={composeIds.includes(record.id)} disabled={busy || (!composeIds.includes(record.id) && composeIds.length >= 3)}
              onChange={event => setComposeIds(current => event.target.checked ? [...current, record.id] : current.filter(id => id !== record.id))} />{record.title}</label>)}</div>
          <label htmlFor="note-compose-focus">整理重点（可选）</label><input id="note-compose-focus" value={composeFocus} maxLength={300} onChange={event => setComposeFocus(event.target.value)} placeholder="例如：总结操作步骤" />
          <div className="notes-actions"><HudButton type="button" variant="primary" disabled={busy || composeIds.length === 0} onClick={() => { void compose() }}>{busy ? '生成中' : '生成待核对草稿'}</HudButton><HudButton type="button" disabled={busy} onClick={() => setComposeOpen(false)}>取消</HudButton></div>
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
        {error && <p className="notes-error" role="alert">{error}</p>}
        {notice && <p className="notes-notice" role="status">{notice}</p>}
        {exportPath && <div className="notes-export"><span>导出文件：{exportPath}</span><button type="button" onClick={() => { void navigator.clipboard.writeText(exportPath).then(() => setNotice('路径已复制。')).catch(() => setError('复制路径失败，请手动选择路径。')) }}>复制路径</button></div>}
        {hasUnsavedChanges && <p className="notes-notice">保存修改后可以导出最新内容。</p>}
        <div className="notes-actions"><HudButton type="button" variant="primary" disabled={busy || !draft.title.trim() || !draft.body.trim()} onClick={() => { void save() }}>{busy ? '处理中' : '保存笔记'}</HudButton><HudButton type="button" disabled={busy || !selectedId || hasUnsavedChanges} onClick={() => { void exportCurrent() }}>导出 Markdown</HudButton><HudButton type="button" disabled={busy || !selectedId || hasUnsavedChanges} onClick={() => setDesignOpen(true)}>生成高级版</HudButton>{selectedId && <HudButton type="button" disabled={busy} onClick={() => { void removeCurrent() }}>删除笔记</HudButton>}</div>
        </>}
      </div>
    </div>
  </section>
}
