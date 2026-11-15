import { useEffect, useState } from 'react'
import type { KnowledgeRecord } from '../data'
import type { Note } from '../notes'
import { previewNoteDesign } from '../note-design'
import { AdvancedNotePanel } from './AdvancedNotePanel'
import { GearIcon } from './GearIcon'
import './advanced-note-studio.css'

type Props = {
  notes: Note[]
  records: KnowledgeRecord[]
  initialNoteId: string | null
  onSelectNote: (note: Note) => void
  onClose: () => void
  onCreateFromSources: () => void
  onRevealRecord: (id: string, atSeconds?: number) => void
  onApplyEdit: (id: string) => Promise<Note>
  onRejectEdit: (id: string) => Promise<void>
  onUndoEdit: (id: string) => Promise<Note>
  onNoteChanged: (note: Note) => void
}

const gearPoints = (teeth: number) => Array.from({ length: teeth * 4 }, (_, step) => {
  const radius = [70, 70, 88, 88][step % 4]
  const angle = (step / (teeth * 4)) * Math.PI * 2 - Math.PI / 2
  return `${(100 + Math.cos(angle) * radius).toFixed(1)},${(100 + Math.sin(angle) * radius).toFixed(1)}`
}).join(' ')

function StudioGear({ index }: { index: number }) {
  return <svg className={`studio-gear studio-gear-${index}`} viewBox="0 0 200 200" aria-hidden="true" focusable="false">
    <polygon points={gearPoints(index === 3 ? 18 : 12)} /><circle cx="100" cy="100" r="48" />
  </svg>
}

const previewText = (note: Note) => note.body.split('\n').map(line => line.trim())
  .find(line => line && !line.startsWith('#'))?.replace(/^[-*>]\s*/, '').slice(0, 220) ?? '这篇笔记还没有可预览的正文。'

export function AdvancedNoteStudio({ notes, records, initialNoteId, onSelectNote, onClose, onCreateFromSources,
  onRevealRecord, onApplyEdit, onRejectEdit, onUndoEdit, onNoteChanged }: Props) {
  const [activeId, setActiveId] = useState(initialNoteId ?? notes[0]?.id ?? '')
  const [editing, setEditing] = useState(false)
  const [working, setWorking] = useState(false)
  const [error, setError] = useState('')
  const active = notes.find(note => note.id === activeId) ?? null

  useEffect(() => {
    if (!notes.some(note => note.id === activeId)) setActiveId(notes[0]?.id ?? '')
  }, [notes, activeId])

  const selectNote = (note: Note) => {
    if (working) return
    setActiveId(note.id); setError(''); onSelectNote(note)
  }
  const generate = async () => {
    if (!active || working) return
    setWorking(true); setError('')
    try {
      await Promise.all([previewNoteDesign(active.id, 'mono', []), new Promise(resolve => window.setTimeout(resolve, 900))])
      setEditing(true)
    } catch (cause) { setError(String(cause)) }
    finally { setWorking(false) }
  }

  if (editing && active) return <div className="studio-editor"><AdvancedNotePanel key={active.id} note={active} records={records}
    onClose={() => setEditing(false)} returnLabel="返回选择" onRevealRecord={onRevealRecord}
    onApplyEdit={onApplyEdit} onRejectEdit={onRejectEdit} onUndoEdit={onUndoEdit} onNoteChanged={onNoteChanged} /></div>

  return <section className="advanced-studio" aria-label="高级笔记工作台" data-working={working}>
    <div className="studio-canvas">
      <header className="studio-header"><span><GearIcon kind="brand" size={16} /> SEC-02 / 高级笔记工作台 <small>{String(notes.length).padStart(2, '0')}</small></span><button type="button" onClick={onClose}>返回资料库</button></header>
      <div className="studio-gears"><div className="studio-main-gear"><StudioGear index={0} />
        <div className="studio-center"><span>{working ? 'GENERATING' : 'SELECTED NOTE'}</span><strong>{active?.title ?? '选择一篇笔记'}</strong><small>{working ? '正在生成高级版预览' : active ? `${active.sources.length} 条来源 · ${active.body.length.toLocaleString('zh-CN')} 字` : '从右侧选择已有笔记'}</small></div>
      </div>{[1, 2, 3].map(index => <StudioGear key={index} index={index} />)}</div>
      <div className="studio-canvas-caption"><strong>让内容更易阅读</strong><p>选中笔记后生成预览，再调整主题、配图和版式。保存前可逐项核对。</p></div>
      {working && <div className="studio-progress" role="status">正在制作《{active?.title}》的预览…</div>}
    </div>
    <aside className="studio-rail" aria-label="选择笔记与预览">
      <header><span><GearIcon kind="brand" size={16} /> 笔记选择</span><small>READ ONLY</small></header>
      <div className="studio-rail-scroll">
        {notes.length === 0 && <p className="studio-empty">还没有笔记。先从资料生成草稿，或在资料库新建笔记。</p>}
        <div className="studio-note-list">{notes.map(note => <button type="button" key={note.id} className="studio-note-option" data-selected={activeId === note.id} disabled={working} onClick={() => selectNote(note)}>
          <span>{note.title}</span><small>{note.sources.length} 条来源 · {new Date(note.updatedAt).toLocaleDateString('zh-CN')}</small>
        </button>)}</div>
        {active && <section className="studio-preview" aria-label="所选笔记预览"><span className="studio-preview-label"><GearIcon kind="brand" size={16} /> 预览</span><h2>{active.title}</h2>
          <div className="studio-preview-meta"><time>{new Date(active.updatedAt).toLocaleString('zh-CN')}</time><span>{active.sources.length} 个来源</span><span>{active.body.length.toLocaleString('zh-CN')} 字</span></div>
          <p>{previewText(active)}</p>
          {active.sources.length > 0 && <div className="studio-preview-sources"><strong>来源资料</strong>{active.sources.slice(0, 4).map((source, index) => <span key={`${source.recordId}-${index}`}>{source.title}</span>)}{active.sources.length > 4 && <small>另有 {active.sources.length - 4} 条</small>}</div>}
        </section>}
        {error && <p className="studio-error" role="alert">{error}</p>}
      </div>
      <footer><button type="button" disabled={!active || working} onClick={() => { if (active) { onSelectNote(active); onClose() } }}>打开笔记</button><button type="button" className="studio-generate" disabled={!active || working} onClick={() => { void generate() }}>{working ? '生成中…' : '生成高级版'}</button><button type="button" onClick={onCreateFromSources} disabled={working}>从资料生成</button></footer>
    </aside>
  </section>
}
