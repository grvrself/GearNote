import { useEffect, useMemo, useState, type RefObject } from 'react'
import { recordFixtures, type KnowledgeRecord, type RecordType } from '../data'
import { ChamferCard } from './ChamferCard'
import { HudButton } from './HudButton'
import { PanelHeading } from './PanelHeading'
import { StatusBadge } from './StatusBadge'
import { StoryboardSection } from './StoryboardSection'
import './history.css'

type Filter = 'all' | RecordType

const savedAtFormatter = new Intl.DateTimeFormat('zh-CN', {
  year: 'numeric', month: '2-digit', day: '2-digit',
  hour: '2-digit', minute: '2-digit', hour12: false,
})

function formatElapsed(seconds: number) {
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor(seconds / 60)
  const remainder = seconds % 60
  return hours > 0
    ? `${String(hours).padStart(2, '0')}:${String(minutes % 60).padStart(2, '0')}:${String(remainder).padStart(2, '0')}`
    : `${String(minutes).padStart(2, '0')}:${String(remainder).padStart(2, '0')}`
}

type Props = {
  records?: KnowledgeRecord[]
  selectedId: string | null
  referencedIds: string[]
  focusRequest?: { id: string; atSeconds?: number } | null
  onSelect: (id: string) => void
  onReference: (id: string) => void
  onRemoveReference: (id: string) => void
  onCollapse: () => void
  miniAvailable: boolean
  onOpenMini: () => void
  searchRef?: RefObject<HTMLInputElement | null>
  onRecordsChanged: () => Promise<void>
}

export function HistoryPanel({ records = recordFixtures, selectedId, referencedIds, focusRequest, onSelect, onReference, onRemoveReference, onCollapse, miniAvailable, onOpenMini, searchRef, onRecordsChanged }: Props) {
  const [filter, setFilter] = useState<Filter>('all')
  const [query, setQuery] = useState('')
  const [activeMoment, setActiveMoment] = useState<number | null>(null)

  useEffect(() => {
    if (!focusRequest) return
    setFilter('all')
    setQuery('')
    setActiveMoment(focusRequest.atSeconds ?? null)
    const frame = window.requestAnimationFrame(() => document.getElementById(focusRequest.id)?.scrollIntoView({ block: 'nearest' }))
    return () => window.cancelAnimationFrame(frame)
  }, [focusRequest])
  const visibleRecords = useMemo(() => records.filter(record => {
    const matchesType = filter === 'all' || record.type === filter
    const matchesQuery = `${record.title} ${record.site}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase())
    return matchesType && matchesQuery
  }), [records, filter, query])

  return <aside className="history-panel" id="rec-02" aria-labelledby="rec-02-title">
    <div className="history-head">
      <PanelHeading id="rec-02-title" serial="REC-02" overline="history" title="历史记录" count={records.length} />
      <button className="history-collapse" type="button" aria-label="收起记录" onClick={onCollapse}>收起记录</button>
    </div>
    <div className="history-tools">
      <div className="history-filter-row">
        <div className="history-filters" role="group" aria-label="资料类型筛选">
          {([['all', '全部'], ['video', '视频'], ['article', '文章']] as const).map(([id, label]) =>
            <button key={id} type="button" aria-pressed={filter === id} onClick={() => setFilter(id)}>{filter === id && <span aria-hidden="true">⌜</span>}{label}</button>)}
        </div>
        <HudButton type="button" disabled={!miniAvailable} onClick={onOpenMini}>□ 小窗模式</HudButton>
      </div>
      <label className="history-search">
        <span className="history-search-label">搜索</span>
        <span className="history-search-field"><svg viewBox="0 0 20 20" aria-hidden="true"><circle cx="8" cy="8" r="5"/><path d="m12 12 5 5"/></svg><input ref={searchRef} value={query} onChange={event => setQuery(event.target.value)} placeholder="标题或来源网站" /><kbd>CTRL K</kbd></span>
      </label>
      <div className="status-showcase" aria-label="状态图例与组件样张">
        <span className="history-mini-label">状态示例</span>
        <div className="status-showcase-grid">
          <StatusBadge status="extracting" progress={36} />
          <StatusBadge status="summarizing" progress={68} />
          <StatusBadge status="ready" />
          <StatusBadge status="fault" reason="页面正文不可读取" />
        </div>
      </div>
    </div>
    <div className="history-list" role="list" aria-label="已保存记录">
      {visibleRecords.length === 0 && <p className="history-empty">无匹配记录</p>}
      {visibleRecords.map(record => <div role="listitem" key={record.id}>
        <RecordCard
          record={record}
          expanded={selectedId === record.id}
          referenced={referencedIds.includes(record.id)}
          activeMoment={activeMoment}
          onToggle={() => { onSelect(record.id); setActiveMoment(null) }}
          onMoment={setActiveMoment}
          onReference={() => referencedIds.includes(record.id) ? onRemoveReference(record.id) : onReference(record.id)}
          onRecordsChanged={onRecordsChanged}
        />
      </div>)}
    </div>
    <div className="history-footer"><span>已保存 {records.filter(record => !record.demo).length}</span><span>{String(visibleRecords.length).padStart(2, '0')} / {String(records.length).padStart(2, '0')}</span></div>
  </aside>
}

type CardProps = {
  record: KnowledgeRecord
  expanded: boolean
  referenced: boolean
  activeMoment: number | null
  onToggle: () => void
  onMoment: (seconds: number) => void
  onReference: () => void
  onRecordsChanged: () => Promise<void>
}

function RecordCard({ record, expanded, referenced, activeMoment, onToggle, onMoment, onReference, onRecordsChanged }: CardProps) {
  const [sourceMessage, setSourceMessage] = useState('')
  const hasMatchingCue = record.transcript?.some(cue => cue.atSeconds === activeMoment) ?? false
  const previewCues = hasMatchingCue
    ? record.transcript?.filter(cue => cue.atSeconds === activeMoment)
    : record.transcript?.slice(0, 80)
  const copySource = async () => {
    if (!record.sourceUrl) return
    try {
      await navigator.clipboard.writeText(record.sourceUrl)
      setSourceMessage('链接已复制')
    } catch {
      setSourceMessage('复制失败，请稍后重试')
    }
  }
  return <ChamferCard className={`record-card ${expanded ? 'is-expanded' : ''}`} id={record.id}>
    <button className="record-summary" type="button" aria-expanded={expanded} aria-controls={`${record.id}-details`} onClick={onToggle}>
      <span className="record-topline"><span className="record-kind">{record.type === 'video' ? '■ VIDEO' : '◆ ARTICLE'}</span>{record.demo && <span className="record-demo">DEMO</span>}{record.durationSeconds != null && <time className="record-duration" dateTime={`PT${record.durationSeconds}S`}>{formatElapsed(record.durationSeconds)}</time>}<span className="record-expand" aria-hidden="true">{expanded ? '−' : '+'}</span></span>
      <strong className="record-title">{expanded && <span aria-hidden="true">⌜</span>}{record.title}</strong>
      <span className="record-origin"><span>{record.site}</span><time dateTime={record.savedAt}>{savedAtFormatter.format(new Date(record.savedAt))}</time></span>
      {record.captionLanguage && <span className="record-caption-language">SUBTITLE / {record.captionLanguage}</span>}
    </button>
    <div className="record-status-row"><StatusBadge status={record.status} progress={record.progress} reason={record.faultReason} /></div>
    <div className="record-details" id={`${record.id}-details`} hidden={!expanded}>
      {record.moments && <section className="record-section"><h3>{record.demo ? 'KEY TIMES / 关键时间点' : 'TIME INDEX / 字幕时间点'}</h3><div className="moment-list">{record.moments.map(moment => <button key={moment.atSeconds} type="button" className={activeMoment === moment.atSeconds ? 'is-active' : ''} onClick={() => onMoment(moment.atSeconds)}>{activeMoment === moment.atSeconds && <span aria-hidden="true">⌜</span>}<time dateTime={`PT${moment.atSeconds}S`}>{formatElapsed(moment.atSeconds)}</time> {moment.title}</button>)}</div></section>}
      {record.transcript && <section className="record-section"><h3>TRANSCRIPT / 字幕摘录 · {record.transcript.length} 条</h3><div className="transcript-list">{previewCues?.map((cue, index) => <p key={`${cue.atSeconds}-${index}`}><time dateTime={`PT${cue.atSeconds}S`}>{formatElapsed(cue.atSeconds)}</time><span>{cue.text}</span></p>)}</div>{record.transcript.length > 80 && !hasMatchingCue && <p className="record-source-message">此处预览前 80 条；展开下方“阅读已保存字幕”可查看全文。</p>}</section>}
      {Boolean(record.excerpt?.length) && <section className="record-section"><h3>EXCERPT / 正文摘录</h3><div className="transcript-list excerpt-list">{record.excerpt?.map((text, index) => <p key={index}>{text}</p>)}</div></section>}
      {record.contentText && <details className="record-full"><summary>{record.type === 'video' ? '阅读已保存字幕' : '阅读已保存正文'}</summary><pre>{record.contentText}</pre></details>}
      {record.type === 'video' && !record.demo && <StoryboardSection record={record} onChanged={onRecordsChanged} />}
      {record.status === 'extracting' && <p className="record-process-note">字幕提取进行中，摘录完成后显示。</p>}
      <div className="record-actions">
        <HudButton variant={record.status === 'ready' ? 'primary' : 'default'} type="button" disabled={record.status !== 'ready'} onClick={onReference}>{referenced ? '已加入参考' : '加入参考'}</HudButton>
        <HudButton type="button" disabled={!record.sourceUrl} onClick={() => { void copySource() }}>{record.type === 'video' ? '复制视频链接' : '复制原文链接'}</HudButton>
        {record.status === 'fault' && <HudButton type="button" disabled title="请返回原页面再次点击浏览器扩展图标">重试</HudButton>}
      </div>
      {record.status === 'fault' && record.type === 'video' && !record.demo && <p className="record-source-message">请返回视频页面，再次点击扩展图标重试。</p>}
      {sourceMessage && <p className="record-source-message" role="status">{sourceMessage}</p>}
    </div>
  </ChamferCard>
}
