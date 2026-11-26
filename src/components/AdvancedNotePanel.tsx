import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { isTauri } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type { Note, NoteEditProposal } from '../notes'
import type { KnowledgeRecord } from '../data'
import { getPendingNoteEdit, proposeNoteRestructure, reviseNoteRestructure } from '../notes'
import { captureNoteFrame, deleteNoteDesign, exportNoteDesign, getNoteDesign, getNoteFrameImage, listNoteFrames, previewNoteDesign, saveNoteDesign, type NoteDesign, type NoteFrameOption, type NoteImage, type NoteTheme } from '../note-design'
import { useConfirm } from '../hooks/useConfirm'
import { HudButton } from './HudButton'
import './advanced-note.css'

type Props = {
  note: Note
  records: KnowledgeRecord[]
  onClose: () => void
  returnLabel?: string
  onRevealRecord: (id: string, atSeconds?: number) => void
  onApplyEdit: (id: string) => Promise<Note>
  onRejectEdit: (id: string) => Promise<void>
  onUndoEdit: (id: string) => Promise<Note>
  onNoteChanged: (note: Note) => void
}

const themes: { id: NoteTheme; name: string; description: string }[] = [
  { id: 'mono', name: '黑白阅读', description: '清晰的层级与留白' },
  { id: 'paper', name: '柔和纸张', description: '适合长文的暖白底色' },
]

function FramePickerItem({ noteId, frame, title, selected, onToggle }: { noteId: string; frame: NoteFrameOption; title: string; selected: boolean; onToggle: () => void }) {
  const [image, setImage] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState(false)
  const preview = async () => {
    if (image || loading) return
    setLoading(true)
    try { setImage(await getNoteFrameImage(noteId, frame.imageId, frame.origin)) }
    catch { setError(true) } finally { setLoading(false) }
  }
  const time = String(Math.floor(frame.atSeconds / 60)).padStart(2, '0') + ':' + String(frame.atSeconds % 60).padStart(2, '0')
  return <div className="advanced-note-frame-option">
    <span>{title} · {time}</span>
    <div><button type="button" onClick={() => { void preview() }} disabled={loading}>{image ? '已预览' : loading ? '读取中' : '预览画面'}</button><button type="button" onClick={onToggle}>{selected ? '取消选择' : '加入笔记'}</button></div>
    {image && <img src={image} alt={'视频 ' + time + ' 的截图'} />}
    {error && <small>画面暂无法预览</small>}
  </div>
}

export function AdvancedNotePanel({ note, records, onClose, returnLabel = '返回笔记', onRevealRecord, onApplyEdit, onRejectEdit, onUndoEdit, onNoteChanged }: Props) {
  const { confirm, confirmDialog } = useConfirm()
  const [theme, setTheme] = useState<NoteTheme>('mono')
  const [html, setHtml] = useState('')
  const [saved, setSaved] = useState<NoteDesign | null>(null)
  const [previewIsCurrent, setPreviewIsCurrent] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [frames, setFrames] = useState<NoteFrameOption[]>([])
  const [images, setImages] = useState<NoteImage[]>([])
  const [restructure, setRestructure] = useState<NoteEditProposal | null>(null)
  const [lastRestructureId, setLastRestructureId] = useState('')
  const [exportPath, setExportPath] = useState('')
  const [focusMode, setFocusMode] = useState(false)
  const [focusError, setFocusError] = useState(false)
  const focusFrame = useRef<HTMLIFrameElement>(null)
  const focusExit = useRef<HTMLButtonElement>(null)
  const videos = note.sources.filter(source => records.some(record => record.id === source.recordId && record.type === 'video' && record.status === 'ready' && !record.demo))
  const [captureRecordId, setCaptureRecordId] = useState('')
  const [captureTime, setCaptureTime] = useState('')

  useEffect(() => {
    if (!focusMode) return
    let active = true
    let changedWindow = false
    const priorFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null
    const previousOverflow = document.body.style.overflow
    const appRoot = document.getElementById('root')
    const wasInert = appRoot?.inert ?? false
    document.body.style.overflow = 'hidden'
    if (appRoot) appRoot.inert = true
    focusExit.current?.focus()
    const windowHandle = isTauri() ? getCurrentWindow() : null
    if (windowHandle) {
      void windowHandle.isFullscreen().then(async wasFullscreen => {
        if (!wasFullscreen) {
          await windowHandle.setFullscreen(true)
          changedWindow = true
          if (!active) await windowHandle.setFullscreen(false)
        }
      }).catch(() => { if (active) setFocusError(true) })
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { event.preventDefault(); setFocusMode(false) }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => {
      active = false
      document.body.style.overflow = previousOverflow
      if (appRoot) appRoot.inert = wasInert
      window.removeEventListener('keydown', onKeyDown)
      if (changedWindow && windowHandle) void windowHandle.setFullscreen(false).catch(() => {})
      priorFocus?.focus()
    }
  }, [focusMode])

  const focusFrameLoaded = (frame: HTMLIFrameElement) => {
    frame.contentDocument?.addEventListener('keydown', event => {
      if (event.key === 'Escape') { event.preventDefault(); setFocusMode(false) }
    })
  }
  const jumpTo = (id: string) => focusFrame.current?.contentDocument?.getElementById(id)?.scrollIntoView({ behavior: 'smooth' })

  useEffect(() => {
    let active = true
    setBusy(true); setError(''); setHtml('')
    void getNoteDesign(note.id).then(async design => {
      if (!active) return
      if (design) {
        setSaved(design); setTheme(design.theme); setHtml(design.html); setImages(design.images); setPreviewIsCurrent(!design.stale)
      } else {
        const preview = await previewNoteDesign(note.id, 'mono', [])
        if (active) { setSaved(null); setTheme('mono'); setHtml(preview); setImages([]); setPreviewIsCurrent(true) }
      }
    }).catch(cause => { if (active) setError(String(cause)) })
      .finally(() => { if (active) setBusy(false) })
    return () => { active = false }
  }, [note.id, note.updatedAt])

  useEffect(() => {
    let active = true
    void getPendingNoteEdit(note.id).then(pending => { if (active && pending?.mode === 'restructure') setRestructure(pending) })
      .catch(() => {})
    return () => { active = false }
  }, [note.id])

  useEffect(() => {
    let active = true
    void listNoteFrames(note.id).then(found => { if (active) setFrames(found) })
      .catch(() => { if (active) setFrames([]) })
    return () => { active = false }
  }, [note.id])

  const updatePreview = async (nextTheme: NoteTheme) => {
    setBusy(true); setError(''); setNotice('')
    try {
      const preview = await previewNoteDesign(note.id, nextTheme, images)
      setTheme(nextTheme); setHtml(preview); setPreviewIsCurrent(true)
    } catch (cause) { setError(String(cause)) } finally { setBusy(false) }
  }

  const chooseTheme = async (next: NoteTheme) => {
    if (busy) return
    await updatePreview(next)
  }

  const save = async () => {
    if (!previewIsCurrent) return
    setBusy(true); setError(''); setNotice('')
    try {
      const design = await saveNoteDesign(note.id, theme, note.updatedAt, images)
      setSaved(design); setImages(design.images); setHtml(design.html); setNotice('高级版已保存，截图也已成为这篇笔记的附件。')
      void listNoteFrames(note.id).then(setFrames).catch(() => {})
    } catch (cause) { setError(String(cause)) } finally { setBusy(false) }
  }

  const savedCurrentTheme = saved?.theme === theme && !saved.stale && previewIsCurrent &&
    JSON.stringify(saved.images) === JSON.stringify(images)
  const changeImages = (next: NoteImage[]) => { setImages(next); setPreviewIsCurrent(false); setNotice('') }
  const toggleFrame = (frame: NoteFrameOption) => {
    const exists = images.some(image => image.origin === frame.origin && image.id === frame.imageId)
    changeImages(exists ? images.filter(image => !(image.origin === frame.origin && image.id === frame.imageId)) :
      [...images, { origin: frame.origin, id: frame.imageId, caption: '', position: 'after' }])
  }
  const capture = async () => {
    if (!captureRecordId || images.length >= 4) return
    const match = /^(\d{1,3}):(\d{2})$/.exec(captureTime.trim())
    if (!match || Number(match[2]) >= 60) { setError('时间请填写为 分:秒，例如 01:25。'); return }
    const seconds = Number(match[1]) * 60 + Number(match[2])
    setBusy(true); setError(''); setNotice('')
    try {
      const frame = await captureNoteFrame(note.id, captureRecordId, seconds)
      setFrames(current => [frame, ...current])
      changeImages([...images, { origin: 'capture', id: frame.imageId, caption: '', position: 'after' }])
      setNotice('画面已暂存。请核对预览；保存高级版后才会加入笔记。')
    } catch (cause) { setError(String(cause)) } finally { setBusy(false) }
  }
  const updateImage = (at: number, patch: Partial<NoteImage>) =>
    changeImages(images.map((image, index) => index === at ? { ...image, ...patch } : image))
  const makeRestructure = async () => {
    setBusy(true); setError(''); setNotice('')
    try { setRestructure(await proposeNoteRestructure(note.id)) }
    catch (cause) { setError(String(cause)) } finally { setBusy(false) }
  }
  const acceptRestructure = async () => {
    if (!restructure) return
    setBusy(true); setError('')
    try {
      await reviseNoteRestructure(restructure.id, restructure.afterText)
      const updated = await onApplyEdit(restructure.id)
      onNoteChanged(updated); setLastRestructureId(restructure.id); setRestructure(null)
      setPreviewIsCurrent(false); setNotice('正文已更新；旧高级版会标为过期，请重新预览并保存。')
    } catch (cause) { setError(String(cause)) } finally { setBusy(false) }
  }
  const discardRestructure = async () => {
    if (!restructure) return
    setBusy(true); setError('')
    try { await onRejectEdit(restructure.id); setRestructure(null); setNotice('已放弃整理，原文未改变。') }
    catch (cause) { setError(String(cause)) } finally { setBusy(false) }
  }
  const undoRestructure = async () => {
    if (!lastRestructureId) return
    setBusy(true); setError('')
    try { const restored = await onUndoEdit(lastRestructureId); onNoteChanged(restored); setLastRestructureId(''); setPreviewIsCurrent(false); setNotice('原文已恢复。') }
    catch (cause) { setError(String(cause)) } finally { setBusy(false) }
  }
  const exportDesign = async (format: 'html' | 'pdf') => {
    setBusy(true); setError(''); setExportPath('')
    try { setExportPath(await exportNoteDesign(note.id, format)); setNotice(format === 'pdf' ? 'PDF 已生成。' : '自包含 HTML 已生成。') }
    catch (cause) { setError(String(cause)) } finally { setBusy(false) }
  }
  const deleteDesign = async () => {
    if (!saved || !await confirm({ title: '删除高级版', message: '删除这篇笔记的高级展示版？普通笔记正文和已保存图片会保留。', confirmLabel: '删除', danger: true })) return
    setBusy(true); setError(''); setNotice(''); setExportPath('')
    try {
      await deleteNoteDesign(note.id)
      setSaved(null); setImages([]); setPreviewIsCurrent(true)
      setHtml(await previewNoteDesign(note.id, theme, []))
      void listNoteFrames(note.id).then(setFrames).catch(() => {})
      setNotice('高级版已删除，普通笔记和已保存图片仍在。')
    } catch (cause) { setError(String(cause)) } finally { setBusy(false) }
  }
  return <section className="advanced-note" aria-label="高级笔记预览">
    <div className="advanced-note-head">
      <div><span className="advanced-note-eyebrow">GEARNOTE / READING EDITION</span><h3>高级笔记</h3><p>原文美化只改变展示方式，不修改笔记正文。</p></div>
      <HudButton type="button" onClick={onClose}>{returnLabel}</HudButton>
    </div>
    <div className="advanced-note-restructure"><div><strong>整理内容并排版</strong><p>模型只重排这篇笔记已有的原文片段。核对删减和顺序后，由你确认写入；可撤销。</p></div>
      {!restructure && <HudButton type="button" disabled={busy || note.body.length > 3200} onClick={() => { void makeRestructure() }}>生成整理草稿</HudButton>}
      {note.body.length > 3200 && <small>正文超过 3200 字；可直接原文美化，或先在普通编辑器分段。</small>}
      {restructure && <div className="advanced-note-diff"><div><strong>原文</strong><pre>{restructure.beforeText}</pre></div><div><label htmlFor="advanced-note-restructure-body"><strong>整理草稿 · 可修改</strong></label><textarea id="advanced-note-restructure-body" value={restructure.afterText} maxLength={12000} onChange={event => setRestructure(current => current ? { ...current, afterText: event.target.value } : current)} /></div>
        <div className="advanced-note-diff-actions"><HudButton type="button" variant="primary" disabled={busy} onClick={() => { void acceptRestructure() }}>确认并更新正文</HudButton><HudButton type="button" disabled={busy} onClick={() => { void discardRestructure() }}>放弃</HudButton></div></div>}
      {lastRestructureId && <HudButton type="button" disabled={busy} onClick={() => { void undoRestructure() }}>撤销本次整理</HudButton>}
    </div>
    <fieldset className="advanced-note-themes" disabled={busy}>
      <legend>阅读主题</legend>
      {themes.map(item => <label key={item.id} data-selected={theme === item.id}>
        <input type="radio" name="advanced-note-theme" checked={theme === item.id} onChange={() => { void chooseTheme(item.id) }} />
        <strong>{item.name}</strong><span>{item.description}</span>
      </label>)}
    </fieldset>
    <div className="advanced-note-image-picker">
      <strong>笔记配图 <span>{images.length} / 4</span></strong>
      <p>可选择当前会话的关联截图，也可直接指定这篇笔记来源视频的时间。保存高级版后，图片会独立留在笔记里。</p>
      {videos.length > 0 && <div className="advanced-note-capture"><select aria-label="要截取的视频" value={captureRecordId} onChange={event => setCaptureRecordId(event.target.value)}><option value="">选择来源视频</option>{videos.map(source => <option key={source.recordId} value={source.recordId}>{source.title}</option>)}</select><input aria-label="截帧时间，分:秒" placeholder="01:25" value={captureTime} onChange={event => setCaptureTime(event.target.value)} /><HudButton type="button" disabled={busy || images.length >= 4 || !captureRecordId || !captureTime.trim()} onClick={() => { void capture() }}>截取画面</HudButton></div>}
      {frames.length === 0 && images.length === 0 && <p>当前没有可选截图。选择上方视频和时间截取画面。</p>}
      {frames.map(frame => <FramePickerItem key={frame.origin + ':' + frame.imageId} noteId={note.id} frame={frame} title={note.sources.find(source => source.recordId === frame.recordId)?.title ?? '视频'}
        selected={images.some(image => image.origin === frame.origin && image.id === frame.imageId)}
        onToggle={() => { if (images.length < 4 || images.some(image => image.origin === frame.origin && image.id === frame.imageId)) toggleFrame(frame) }} />)}
      {images.map((image, index) => <div className="advanced-note-image-edit" key={image.origin + ':' + image.id}>
        <span>图片 {index + 1}{image.origin === 'asset' ? ' · 已保存附件' : ' · 待保存截图'}</span>
        <input aria-label={'图片 ' + (index + 1) + ' 的说明'} maxLength={160} placeholder="为图片写一句说明" value={image.caption}
          onChange={event => updateImage(index, { caption: event.target.value })} />
        <select aria-label={'图片 ' + (index + 1) + ' 的位置'} value={image.position}
          onChange={event => updateImage(index, { position: event.target.value as NoteImage['position'] })}>
          <option value="before">正文前</option><option value="after">正文后</option>
        </select>
        <button type="button" onClick={() => changeImages(images.filter((_, at) => at !== index))}>移除</button>
      </div>)}
    </div>
    {saved?.stale && !previewIsCurrent && <p className="advanced-note-stale">这份高级版基于旧正文。请先预览当前内容，再保存新版。</p>}
    <div className="advanced-note-actions">
      {!previewIsCurrent && <HudButton type="button" disabled={busy} onClick={() => { void updatePreview(theme) }}>更新预览</HudButton>}
      <HudButton type="button" variant="primary" disabled={busy || !html || !previewIsCurrent || Boolean(savedCurrentTheme)} onClick={() => { void save() }}>{busy ? '处理中' : savedCurrentTheme ? '已保存' : '保存高级版'}</HudButton>
      <HudButton type="button" disabled={!html} onClick={() => { setFocusError(false); setFocusMode(true) }}>专注阅读</HudButton>
      <HudButton type="button" disabled={busy || !savedCurrentTheme} onClick={() => { void exportDesign('html') }}>导出 HTML</HudButton>
      <HudButton type="button" disabled={busy || !savedCurrentTheme} onClick={() => { void exportDesign('pdf') }}>导出 PDF</HudButton>
      {saved && <HudButton type="button" disabled={busy} onClick={() => { void deleteDesign() }}>删除高级版</HudButton>}
      {saved && <span className="advanced-note-saved">{saved.stale ? '旧版已保存' : '已保存到这篇笔记'}</span>}
    </div>
    {error && <p className="notes-error" role="alert">{error}</p>}
    {notice && <p className="notes-notice" role="status">{notice}</p>}
    {exportPath && <div className="notes-export"><span>导出文件：{exportPath}</span><button type="button" onClick={() => { void navigator.clipboard.writeText(exportPath).then(() => setNotice('路径已复制。')).catch(() => setError('复制路径失败。')) }}>复制路径</button></div>}
    {note.sources.length > 0 && <div className="advanced-note-sources"><strong>来源</strong>{note.sources.map((source, index) => <button type="button" key={source.recordId + ':' + String(source.atSeconds ?? '') + ':' + index} onClick={() => onRevealRecord(source.recordId, source.atSeconds)}>{source.title}{source.atSeconds == null ? '' : ' · ' + Math.floor(source.atSeconds / 60) + ':' + String(source.atSeconds % 60).padStart(2, '0')}</button>)}</div>}
    {html && <iframe className="advanced-note-frame" sandbox="" srcDoc={html} title={'《' + note.title + '》高级版预览'} />}
    {focusMode && html && createPortal(<div className="advanced-note-focus" role="dialog" aria-modal="true" aria-label={'《' + note.title + '》专注阅读'}>
      <header className="advanced-note-focus-top"><div className="advanced-note-focus-brand">GEARNOTE <span>/ FOCUS</span></div><span className="advanced-note-focus-title" title={note.title}>{note.title}</span><div className="advanced-note-focus-controls"><span className="advanced-note-focus-save">{savedCurrentTheme ? '● 已保存' : '○ 预览未保存'}</span><button ref={focusExit} type="button" onClick={() => setFocusMode(false)}>退出专注 · Esc</button></div></header>
      <nav className="advanced-note-focus-rail" aria-label="笔记章节"><button type="button" onClick={() => jumpTo('note-title')}>01 标题</button><button type="button" onClick={() => jumpTo('note-body')}>02 正文</button>{note.sources.length > 0 && <button type="button" onClick={() => jumpTo('note-sources')}>03 来源</button>}</nav>
      <iframe ref={focusFrame} className="advanced-note-focus-frame" sandbox="allow-same-origin" srcDoc={html} title={'《' + note.title + '》专注阅读'} onLoad={event => focusFrameLoaded(event.currentTarget)} />
      <footer className="advanced-note-focus-bottom"><span>字数 {Array.from(note.body).filter(char => !/\s|[，。！？、；：,.!?;:]/u.test(char)).length.toLocaleString('zh-CN')} · 来源 {note.sources.length}</span><span>{focusError ? '系统全屏未启用 · 窗口内阅读' : '本地阅读'} · {savedCurrentTheme ? '已保存' : '待保存'}</span></footer>
    </div>, document.body)}
    {confirmDialog}
  </section>
}
