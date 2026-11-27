import { useEffect, useRef, useState } from 'react'
import type { ChatFrame, ChatMessage, ChatStreaming } from '../chat-data'
import type { KnowledgeRecord } from '../data'
import type { ChatIntent, ContextPreview, PendingChoice, PendingLibrarySearch, PendingChatNoteEdit, PendingCommand, ProjectMemory, WorkingContext, ReadContinuationSummary } from '../agent'
import { getChatFrameImage, stopAgentReading } from '../agent'
import { isCaptureCommand } from '../capture-command'
import { useConfirm } from '../hooks/useConfirm'
import { CapturedFrame } from './CapturedFrame'
import { ChamferCard } from './ChamferCard'
import { GearDivider, GearIcon, PaperPlaneIcon } from './GearIcon'
import { AgentGothicHeader, AgentHeaderStatus } from './mechanical'
import { MechanicalGear } from './MechanicalGear'
import { HudButton } from './HudButton'
import './chat.css'

type Props = {
  pendingChoice?: PendingChoice | null
  pendingLibrarySearch?: PendingLibrarySearch | null
  onResolveLibrarySearch?: (proposalId: string, accept: boolean, question: string) => Promise<void>
  pendingChatNoteEdit?: PendingChatNoteEdit | null
  onResolveChatNoteEdit?: (proposalId: string, accept: boolean) => Promise<void>
  pendingCommand?: PendingCommand | null
  onResolveCommand?: (proposalId: string, accept: boolean) => Promise<void>
  records: KnowledgeRecord[]
  frameToAttach?: { frame: ChatFrame; sessionId: string } | null
  onFrameAttached?: () => void
  sessionId?: string
  referencedIds: string[]
  workingContext?: WorkingContext
  projectMemory?: ProjectMemory[]
  compactedSummary?: string
  compactedCount?: number
  historyOpen: boolean
  agentAvailable: boolean
  sending?: boolean
  streaming?: ChatStreaming | null
  suggestingMessageId?: string | null
  suggestionErrorId?: string | null
  error?: string
  onContinueReading?: (messageId: string) => Promise<boolean>
  onSend?: (question: string, frameId?: string, messageIds?: string[], choiceId?: string, deepMode?: boolean, intent?: ChatIntent) => Promise<boolean>
  onPreviewContext?: (question: string, frameId?: string, messageIds?: string[], deepMode?: boolean) => Promise<ContextPreview>
  onOpenAgentSetup?: () => void
  onSummarize?: (deepMode?: boolean) => void
  onRemoveReference: (id: string) => void
  onRevealRecord: (id: string, atSeconds?: number) => void
  onHoverCitation?: (recordId: string | null) => void
  onToggleHistory: () => void
  onShowHistory: () => void
  onCreateNoteDraft?: (message: ChatMessage) => void
  onOpenUrlImport?: () => void
  messages?: ChatMessage[]
  compact?: boolean
  /** 后端续读摘要；按钮须同时满足 hasProgress 与消息匹配，最终有效性仍由后端校验。 */
  readContinuation?: ReadContinuationSummary | null
}

const timeFormatter = new Intl.DateTimeFormat('zh-CN', { hour: '2-digit', minute: '2-digit', hour12: false })

type SlashActionId = ChatIntent | 'url_import'
type SlashEntry = { id: SlashActionId; slash: string; label: string; shortHint: string; aliases: string[] }

const intentEntries: { intent: ChatIntent; slash: string; label: string; shortHint: string; hint: string; aliases: string[] }[] = [
  { intent: 'library_search', slash: '/search', label: '搜资料库', shortHint: '全库搜索标题，确认后加入参考', hint: '在整个资料库搜索标题；结果确认后才加入本轮参考', aliases: ['search', 'library', 'lib', 'sou', 's', '搜', '资料', '搜资料库'] },
  { intent: 'note_edit', slash: '/note', label: '改笔记', shortHint: '生成修改建议，需写明笔记标题', hint: '生成笔记修改建议；确认后才写入。请在问题中写明笔记标题', aliases: ['note', 'edit', 'gai', 'n', '改', '笔记', '改笔记'] },
  { intent: 'command', slash: '/cmd', label: '运行命令', shortHint: '提出 ffmpeg / ffprobe / yt-dlp 提案', hint: '提出 ffmpeg / ffprobe / yt-dlp 命令；确认后才执行', aliases: ['cmd', 'command', 'run', 'ffmpeg', 'ffprobe', 'yt-dlp', 'c', '命令', '运行', '运行命令'] },
]

const urlImportSlashEntry: SlashEntry = {
  id: 'url_import',
  slash: '/url',
  label: '导入链接',
  shortHint: '链接直采：抓取网页文章或视频字幕并导入资料库',
  aliases: ['url', 'import', 'link', 'capture', 'web', 'lianjie', 'daoru', 'zhicai', '链接', '导入', '导入链接', '直采', '链接直采'],
}

function IntentIcon({ intent }: { intent: SlashActionId }) {
  return <svg className="intent-icon" viewBox="0 0 16 16" aria-hidden="true">
    {intent === 'library_search' && <><circle cx="7" cy="7" r="4.2" /><path d="m10.2 10.2 3.3 3.3" /></>}
    {intent === 'note_edit' && <path d="M3 13h10M4.5 10.2 10.8 3.9l1.8 1.8-6.3 6.3-2.2.4z" />}
    {intent === 'command' && <path d="M3 4.5 6.5 8 3 11.5M8 11.5h5" />}
    {intent === 'url_import' && <path d="M8 2.5v7m-2.5-2.5L8 9.5l2.5-2.5M3.5 11.5v1.5h9v-1.5" />}
  </svg>
}

function formatElapsed(seconds: number) {
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor(seconds / 60)
  const remainder = seconds % 60
  return hours > 0
    ? `${String(hours).padStart(2, '0')}:${String(minutes % 60).padStart(2, '0')}:${String(remainder).padStart(2, '0')}`
    : `${String(minutes).padStart(2, '0')}:${String(remainder).padStart(2, '0')}`
}

function formatRunElapsed(milliseconds: number) {
  if (milliseconds < 1000) return `${milliseconds}ms`
  const seconds = Math.floor(milliseconds / 1000)
  return seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${seconds % 60}s`
}

function MessageRun({ message, onRevealRecord }: { message: ChatMessage; onRevealRecord: Props['onRevealRecord'] }) {
  if (!message.run && !message.toolSteps?.length) return null
  const files = message.run?.changedFiles ?? []
  const readSources = message.run?.readSources ?? []
  const coverage = message.run?.readCoverage
  return <details className="message-run">
    <summary>{message.run ? <>用时 {formatRunElapsed(message.run.elapsedMs)} <span>· {coverage ? `深度阅读至 ${formatElapsed(coverage.readUntilSeconds)} / ${formatElapsed(coverage.totalSeconds)}` : readSources.length ? `已读 ${readSources.length} 段资料` : files.length ? `修改 ${files.length} 个项目文件` : '未修改项目文件'}</span></> : `资料探索 · ${message.toolSteps?.length} 步`}</summary>
    {coverage && <p>字幕已读至 {formatElapsed(coverage.readUntilSeconds)} / {formatElapsed(coverage.totalSeconds)} · {coverage.batchCount} 批 · {coverage.complete ? '已读完' : `未读完：${coverage.reason}`}</p>}
    {Boolean(message.toolSteps?.length) && <ol className="message-run-steps">{message.toolSteps?.map((step, index) => <li key={index}>{step}</li>)}</ol>}
    {readSources.length > 0 && <div className="message-run-files"><strong>本轮实际读取</strong><ul>{readSources.map(source => <li key={source.sourceIndex}><button type="button" onClick={() => onRevealRecord(source.recordId, source.atSeconds ?? undefined)}>[{source.sourceIndex}] {source.title}{source.atSeconds == null ? '' : ` · ${formatElapsed(source.atSeconds)}`}</button></li>)}</ul></div>}
    {message.frame && <p>已截取视频画面。</p>}
    {message.run && (files.length ? <div className="message-run-files"><strong>修改的文件</strong><ul>{files.map(file => <li key={file}>{file}</li>)}</ul></div> : <p>本轮没有修改项目文件。</p>)}
  </details>
}

function StreamingRun({ streaming }: { streaming: ChatStreaming }) {
  const [now, setNow] = useState(Date.now())
  useEffect(() => {
    setNow(Date.now())
    const timer = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(timer)
  }, [streaming.startedAt])
  return <div className="message-run-live">
    <span>已用时 {formatRunElapsed(Math.max(0, now - streaming.startedAt))}</span>
    {streaming.activities.length > 0 && <ol className="message-run-steps">{streaming.activities.map((step, index) => <li key={index}>{step}</li>)}</ol>}
  </div>
}

export function ChatPanel({ records, frameToAttach = null, onFrameAttached, pendingChoice = null, pendingLibrarySearch = null, onResolveLibrarySearch, pendingChatNoteEdit = null, onResolveChatNoteEdit, pendingCommand = null, onResolveCommand, sessionId, referencedIds, workingContext = { goal: '', completed: '', nextStep: '', updatedAt: '' }, projectMemory = [], compactedSummary = '', compactedCount = 0, historyOpen, agentAvailable, sending = false, streaming = null, suggestingMessageId = null, suggestionErrorId = null, error = '', onContinueReading, onSend, onPreviewContext, onOpenAgentSetup, onSummarize, onRemoveReference, onRevealRecord, onHoverCitation, onToggleHistory, onShowHistory, onCreateNoteDraft, onOpenUrlImport, messages = [], compact = false, readContinuation = null }: Props) {
  const [draft, setDraft] = useState('')
  const { confirm, confirmDialog } = useConfirm()
  const [stoppingReading, setStoppingReading] = useState(false)
  const [stopReadingError, setStopReadingError] = useState('')
  useEffect(() => { if (!streaming?.reading) { setStoppingReading(false); setStopReadingError('') } }, [streaming?.reading])
  const [deepMode, setDeepMode] = useState(false)
  const [intent, setIntent] = useState<ChatIntent | null>(null)
  const [slashDismissed, setSlashDismissed] = useState(false)
  const [slashIndex, setSlashIndex] = useState(0)
  const [choiceSubmitting, setChoiceSubmitting] = useState(false)
  const [librarySubmitting, setLibrarySubmitting] = useState(false)
  const [noteSubmitting, setNoteSubmitting] = useState(false)
  const [commandSubmitting, setCommandSubmitting] = useState(false)
  const [attachedFrame, setAttachedFrame] = useState<ChatFrame | null>(null)
  const [attachedFramePreview, setAttachedFramePreview] = useState('')
  const [selectedMessageIds, setSelectedMessageIds] = useState<string[]>([])
  const [dismissedNotes, setDismissedNotes] = useState<string[]>([])
  const [contextPreview, setContextPreview] = useState<ContextPreview | null>(null)
  const [contextPreviewOpen, setContextPreviewOpen] = useState(false)
  const [contextPreviewBusy, setContextPreviewBusy] = useState(false)
  const [contextPreviewError, setContextPreviewError] = useState('')
  const latestAgentId = messages.filter(message => message.role === 'agent').at(-1)?.id
  const draftRef = useRef<HTMLTextAreaElement>(null)
  const sessionIdRef = useRef(sessionId)
  sessionIdRef.current = sessionId
  const flowRef = useRef<HTMLOListElement>(null)
  const referencedRecords = referencedIds.map(id => records.find(record => record.id === id)).filter((record): record is KnowledgeRecord => Boolean(record))
  const selectedMessages = selectedMessageIds.map(id => messages.find(message => message.id === id)).filter((message): message is ChatMessage => Boolean(message))
  const deepEligible = referencedRecords.length === 1 && referencedRecords[0].type === 'video' && referencedRecords[0].status === 'ready' && !attachedFrame && selectedMessages.length === 0 && pendingChoice?.status !== 'open'
  const activeDeepMode = deepEligible && deepMode
  const previewIdentity = `${sessionId ?? ''}\u0000${draft.trim()}\u0000${referencedIds.join('|')}\u0000${selectedMessageIds.join('|')}\u0000${attachedFrame?.imageId ?? ''}\u0000${workingContext.updatedAt}\u0000${projectMemory.map(item => item.id).join('|')}\u0000${compactedCount}\u0000${activeDeepMode}`
  const previewIdentityRef = useRef(previewIdentity)
  previewIdentityRef.current = previewIdentity
  const referenceCount = referencedRecords.length + selectedMessages.length
  const referenceLabel = referencedRecords[0]?.title ?? (selectedMessages[0] ? `${selectedMessages[0].role === 'agent' ? 'Agent 回答' : '你的消息'}：${selectedMessages[0].paragraphs.join(' ')}` : '')
  const captureDraft = !intent && isCaptureCommand(draft)
  const canSend = agentAvailable || captureDraft
  const streamingCapture = streaming ? (streaming.capture ?? isCaptureCommand(streaming.question)) : false
  const intentsDisabled = sending || !agentAvailable || Boolean(attachedFrame) || selectedMessageIds.length > 0 || activeDeepMode
  const availableSlashEntries: SlashEntry[] = [
    ...(!intentsDisabled ? intentEntries.map(entry => ({ id: entry.intent, slash: entry.slash, label: entry.label, shortHint: entry.shortHint, aliases: entry.aliases })) : []),
    ...(!sending && onOpenUrlImport ? [urlImportSlashEntry] : []),
  ]
  const slashMatch = availableSlashEntries.length > 0 ? draft.match(/^\/([^\s]*)$/) : null
  const slashQuery = slashMatch?.[1].toLowerCase() ?? ''
  const slashOptions = slashMatch
    ? availableSlashEntries.filter(entry =>
        !slashQuery
        || entry.slash.slice(1).includes(slashQuery)
        || entry.label.toLowerCase().includes(slashQuery)
        || entry.aliases.some(alias => alias.includes(slashQuery)),
      )
    : []
  const slashOpen = Boolean(slashMatch) && !slashDismissed && slashOptions.length > 0
  const activeSlashIndex = slashOptions.length > 0 ? Math.min(slashIndex, slashOptions.length - 1) : 0
  const selectedIntent = intentEntries.find(entry => entry.intent === intent)

  const attachFrame = (frame: ChatFrame) => {
    setAttachedFrame(frame)
    setDeepMode(false)
    setIntent(null)
    setSlashDismissed(false)
    setSlashIndex(0)
    setContextPreview(null)
    setContextPreviewOpen(false)
    draftRef.current?.focus()
  }

  const applySlashAction = (chosen: SlashActionId) => {
    setDraft('')
    setSlashDismissed(false)
    setSlashIndex(0)
    setContextPreview(null)
    setContextPreviewOpen(false)
    if (chosen === 'url_import') {
      onOpenUrlImport?.()
      return
    }
    setIntent(chosen)
    draftRef.current?.focus()
  }

  useEffect(() => { setDraft(''); setAttachedFrame(null); setSelectedMessageIds([]); setDeepMode(false); setIntent(null); setSlashDismissed(false); setSlashIndex(0); setDismissedNotes([]); setContextPreview(null); setContextPreviewOpen(false) }, [sessionId])
  useEffect(() => {
    if (!frameToAttach || frameToAttach.sessionId !== sessionId) return
    attachFrame(frameToAttach.frame)
    onFrameAttached?.()
  }, [frameToAttach])
  useEffect(() => { setDeepMode(false); setContextPreview(null); setContextPreviewOpen(false) }, [referencedIds.join('|')])
  useEffect(() => {
    if (!attachedFrame) { setAttachedFramePreview(''); return }
    let active = true
    setAttachedFramePreview('')
    void getChatFrameImage(attachedFrame.imageId).then(image => { if (active) setAttachedFramePreview(image) })
    return () => { active = false }
  }, [attachedFrame?.imageId])
  useEffect(() => { setContextPreview(null); setContextPreviewOpen(false) }, [workingContext, projectMemory, compactedSummary])

  useEffect(() => {
    if (flowRef.current) flowRef.current.scrollTop = flowRef.current.scrollHeight
  }, [messages, streaming?.answer])

  const toggleMessageReference = (id: string) => {
    if (sending) return
    setIntent(null)
    setSlashDismissed(false)
    setSlashIndex(0)
    setSelectedMessageIds(current => current.includes(id) ? current.filter(value => value !== id) : current.length < 4 ? [...current, id] : current)
    setContextPreview(null)
    setContextPreviewOpen(false)
  }

  const send = async () => {
    const sendingSessionId = sessionId
    const question = draft.trim()
    const capture = !intent && isCaptureCommand(question)
    if (!question || !onSend || !(agentAvailable || capture) || sending || (capture && referencedRecords.length === 0)) return
    setDraft('')
    setContextPreview(null)
    setContextPreviewOpen(false)
    const sent = await onSend(question, capture ? undefined : attachedFrame?.imageId, capture ? [] : selectedMessageIds, undefined, !capture && !intent && activeDeepMode, intent ?? undefined)
    if (sent) setIntent(null)
    if (sent && !capture) { setAttachedFrame(null); setSelectedMessageIds([]) }
    if (!sent && sessionIdRef.current === sendingSessionId) setDraft(question)
  }

  const chooseSuggestion = async (prompt: string) => {
    if (draft.trim() && draft.trim() !== prompt && !await confirm({ title: '替换输入内容', message: '用这条建议替换当前输入内容？', confirmLabel: '替换' })) return
    setDraft(prompt)
    setContextPreview(null)
    setContextPreviewOpen(false)
    draftRef.current?.focus()
  }

  const selectChoice = async (label: string) => {
    if (!onSend || !pendingChoice || sending || streaming || choiceSubmitting) return
    setChoiceSubmitting(true)
    try { await onSend(`选择：${label}`, undefined, [], pendingChoice.id) }
    finally { setChoiceSubmitting(false) }
  }

  const previewContext = async () => {
    if (!onPreviewContext || !draft.trim() || contextPreviewBusy) return
    if (contextPreview && contextPreviewOpen) { setContextPreviewOpen(false); return }
    if (contextPreview) { setContextPreviewOpen(true); return }
    setContextPreviewBusy(true)
    setContextPreviewError('')
    const identity = previewIdentity
    try {
      const result = await onPreviewContext(draft.trim(), attachedFrame?.imageId, selectedMessageIds, activeDeepMode)
      if (previewIdentityRef.current !== identity) return
      setContextPreview(result)
      setContextPreviewOpen(true)
    } catch (cause) { if (previewIdentityRef.current === identity) setContextPreviewError(String(cause)) }
    finally { setContextPreviewBusy(false) }
  }

  return <section className="chat-panel" aria-label="Agent 对话">
    {!compact && <MechanicalGear className="mech-chat-watermark" teeth={20} speed="42s" reverse />}
    {!compact && <header className="chat-page-head"><div><span>AGENT / DIALOGUE</span><h1>Agent 对话</h1><p>只依据本轮资料回答，引用可回到原文。</p></div><AgentGothicHeader active={Boolean(sending || streaming)} /><AgentHeaderStatus ready={canSend} active={Boolean(sending || streaming)} /><MechanicalGear className="mech-heading-gear" teeth={16} speed="22s" reverse /></header>}
    {(compact || referencedRecords.length > 0 || !historyOpen) && <div className="context-bar" id="ctx-01">
      {!compact && <span className="context-bar-label">本轮参考</span>}
      <div className="context-items" aria-label="当前引用资料">
        {referencedRecords.map(record => <div className="context-item" key={record.id}>
          <button className="context-open" type="button" title={`查看 ${record.title}`} onClick={() => onRevealRecord(record.id)}><span aria-hidden="true">{record.type === 'video' ? '■' : '◆'}</span><span>{record.title}</span></button>
          <button className="context-remove" type="button" aria-label={`移除引用：${record.title}`} title="移除引用" onClick={() => onRemoveReference(record.id)}>×</button>
        </div>)}
      </div>
      {(compact || !historyOpen) && <button className="context-toggle" type="button" aria-expanded={compact ? undefined : false} onClick={onToggleHistory}>{compact ? '查看资料 ↗' : '展开记录'}</button>}
    </div>}

    <div className="dialog-head"><span>对话记录</span><span>{messages.length + (streaming ? 2 : 0)} 条消息</span></div>
    <ol className="message-flow" id="dlg-01" ref={flowRef} aria-label="对话消息">
      {messages.length === 0 && <li className="chat-empty"><GearIcon kind="center" size={20} /><div><strong>从资料开始一次可追溯的对话</strong><p>从右侧选择已保存的文章或字幕，添加到本轮参考，然后在下方提问。</p></div></li>}
      {messages.map(message => <li className="message-row" data-role={message.role} key={message.id}>
        <ChamferCard className={`message-card${selectedMessageIds.includes(message.id) ? ' message-card-referenced' : ''}`} id={message.id}>
          {message.role === 'agent' && <span className="message-agent-mark" aria-hidden="true"><GearIcon kind="brand" size={16} /></span>}
          <div className="message-meta"><span>{message.role === 'agent' ? 'GearNote' : '你'}</span><time dateTime={/^\d+$/.test(message.createdAt) ? new Date(Number(message.createdAt)).toISOString() : message.createdAt}>{timeFormatter.format(new Date(/^\d+$/.test(message.createdAt) ? Number(message.createdAt) : message.createdAt))}</time></div>
          <div className="message-body">{message.paragraphs.map((paragraph, index) => <p key={index}>{paragraph}</p>)}{message.points && <ol>{message.points.map(point => <li key={point}>{point}</li>)}</ol>}{message.frame && <CapturedFrame frame={message.frame} title={records.find(record => record.id === message.frame?.recordId)?.title ?? '视频'} selected={attachedFrame?.imageId === message.frame.imageId} onAttach={() => { if (message.frame) attachFrame(message.frame) }} />}</div>
          <div className="message-reference-control"><button type="button" aria-pressed={selectedMessageIds.includes(message.id)} disabled={sending || (!selectedMessageIds.includes(message.id) && selectedMessageIds.length >= 4)} title={selectedMessageIds.includes(message.id) ? '移出本轮参考' : '将这条消息加入本轮参考'} onClick={() => toggleMessageReference(message.id)}>{selectedMessageIds.includes(message.id) ? '✓ 已选为参考 · 移除' : '＋ 选此消息为参考'}</button></div>
          {message.role === 'agent' && <MessageRun message={message} onRevealRecord={onRevealRecord} />}
          {message.role === 'agent' && (message.outcome
            ? message.outcome.notices?.map((notice, index) => <p className="message-citation-warning" role="note" key={`${notice.code}-${index}`}>{notice.message}</p>)
            // 无结构化状态的历史消息保守沿用前缀筛选渲染提醒；仅影响展示，不参与行为判断。
            : message.toolSteps?.filter(step => step.startsWith('引用提醒：') || step.startsWith('回答提醒：')).map((step, index) => <p className="message-citation-warning" role="note" key={`${index}-${step}`}>{step}</p>))}
          {message.id === latestAgentId && readContinuation?.hasProgress && readContinuation.messageId === message.id && <div className="message-citation-warning"><span>本次自动阅读已结束；如需更多内容，可另开一轮阅读预算。</span><HudButton type="button" disabled={sending || Boolean(streaming) || !agentAvailable} onClick={() => { void onContinueReading?.(message.id) }}>继续读取</HudButton></div>}
          {message.citations && <div className="message-citations"><span>引用</span><div>{message.citations.map((citation, index) => {
            const record = records.find(item => item.id === citation.recordId)
            if (citation.kind === 'chat' || citation.recordId.startsWith('chat:')) {
              const cited = messages.find(item => item.id === citation.recordId.slice(5))
              return cited && <button type="button" key={`${citation.recordId}-${citation.sourceIndex ?? index}`} title={`定位${cited.role === 'agent' ? 'Agent 回答' : '用户消息'}`} onClick={() => document.getElementById(cited.id)?.scrollIntoView({ behavior: 'smooth', block: 'center' })}><span aria-hidden="true">{citation.label ?? (citation.sourceIndex ? `[${citation.sourceIndex}]` : '↗')}</span>{cited.role === 'agent' ? 'Agent 回答' : '用户消息'}</button>
            }
            if (!record) return null
            return <button type="button" key={`${citation.recordId}-${citation.sourceIndex ?? index}`} title={`查看 ${record.title}`} onClick={() => onRevealRecord(citation.recordId, citation.atSeconds)} onMouseEnter={() => onHoverCitation?.(citation.recordId)} onMouseLeave={() => onHoverCitation?.(null)} onFocus={() => onHoverCitation?.(citation.recordId)} onBlur={() => onHoverCitation?.(null)}><span aria-hidden="true">{citation.label ?? (citation.sourceIndex ? `[${citation.sourceIndex}]` : '↗')}</span>{citation.atSeconds == null ? record.site : `${formatElapsed(citation.atSeconds)} ${citation.kind === 'image' ? '画面' : '字幕'}`}</button>
          })}</div></div>}
          {message.id === latestAgentId && suggestingMessageId === message.id && <p className="chat-suggestion-status" role="status">正在准备下一步建议…</p>}
          {message.id === latestAgentId && suggestionErrorId === message.id && <p className="chat-suggestion-status">本轮建议暂未生成，回答不受影响。</p>}
          {message.id === latestAgentId && Boolean(message.suggestions?.length) && <div className="chat-suggestions"><span>接下来可以</span><div>{message.suggestions?.map(suggestion => <button type="button" key={suggestion.prompt} disabled={sending} title={suggestion.prompt} onClick={() => { void chooseSuggestion(suggestion.prompt) }}>{suggestion.label} ↗</button>)}</div></div>}
          {pendingChoice?.status === 'open' && pendingChoice.messageId === message.id && <div className="chat-choice" role="group" aria-label={pendingChoice.prompt}>
            {pendingChoice.options.map(option => <button type="button" key={option.id} disabled={sending || Boolean(streaming) || choiceSubmitting || !agentAvailable} onClick={() => { void selectChoice(option.label) }}><strong>{option.label}{pendingChoice.recommended === option.id && <span className="chat-choice-recommended">推荐</span>}</strong>{option.detail && <span>{option.detail}</span>}</button>)}
            <small>也可以在下方输入自己的选择；发送后会按你的回答继续。</small>
          </div>}
          {pendingLibrarySearch?.status === 'open' && pendingLibrarySearch.messageId === message.id && <div className="chat-library-proposal" role="group" aria-label="资料库搜索结果">
            <strong>要打开这几条吗？</strong>
            <ul>{pendingLibrarySearch.records.map(record => <li key={record.id}>{record.title}</li>)}</ul>
            <div><button type="button" disabled={sending || Boolean(streaming) || librarySubmitting} onClick={() => { if (!onResolveLibrarySearch) return; setLibrarySubmitting(true); void onResolveLibrarySearch(pendingLibrarySearch.id, true, pendingLibrarySearch.question).finally(() => setLibrarySubmitting(false)) }}>确认并继续提问</button>
              <button type="button" disabled={sending || Boolean(streaming) || librarySubmitting} onClick={() => { if (!onResolveLibrarySearch) return; setLibrarySubmitting(true); void onResolveLibrarySearch(pendingLibrarySearch.id, false, pendingLibrarySearch.question).finally(() => setLibrarySubmitting(false)) }}>暂不打开</button></div>
            <small>确认后加入本轮参考，再按原问题读取资料；跳过后不会读取正文。</small>
          </div>}
          {pendingChatNoteEdit?.proposal.status === 'pending' && pendingChatNoteEdit.messageId === message.id && <div className="chat-note-proposal" role="group" aria-label="待确认的笔记修改">
            <strong>笔记《{pendingChatNoteEdit.noteTitle}》修改预览</strong>
            <ol>{pendingChatNoteEdit.proposal.edits.map((edit, index) => <li key={index}>
              <p><b>{edit.mode === 'append' ? '追加' : '替换'}</b> · {edit.reason}</p>
              <p>原文：{edit.beforeText}</p><p>新文字：{edit.afterText}</p>
              {edit.quote && <p>资料原句 [{edit.sourceIndex}]：{edit.quote}</p>}
            </li>)}</ol>
            <div><button type="button" disabled={sending || Boolean(streaming) || noteSubmitting} onClick={() => { if (!onResolveChatNoteEdit) return; setNoteSubmitting(true); void onResolveChatNoteEdit(pendingChatNoteEdit.proposal.id, true).finally(() => setNoteSubmitting(false)) }}>确认并写入</button>
              <button type="button" disabled={sending || Boolean(streaming) || noteSubmitting} onClick={() => { if (!onResolveChatNoteEdit) return; setNoteSubmitting(true); void onResolveChatNoteEdit(pendingChatNoteEdit.proposal.id, false).finally(() => setNoteSubmitting(false)) }}>放弃</button></div>
            <small>确认前不会修改笔记。</small>
          </div>}
          {pendingCommand && pendingCommand.messageId === message.id && (pendingCommand.status === 'open' || pendingCommand.status === 'running') && <div className="chat-command-proposal" role="group" aria-label="待确认的命令">
            <strong>{pendingCommand.status === 'running' ? '命令正在运行' : '确认后运行命令'}</strong>
            <p>程序：<code>{pendingCommand.program}</code></p>
            <ol>{pendingCommand.effectiveArgs.map((arg, index) => <li key={index}><code>{arg}</code></li>)}</ol>
            <small>工作目录：应用命令缓存。最长运行 30 秒，聊天只保留前 8192 字节输出。</small>
            {pendingCommand.status === 'open' && <div><button type="button" disabled={sending || Boolean(streaming) || commandSubmitting} onClick={() => { if (!onResolveCommand) return; setCommandSubmitting(true); void onResolveCommand(pendingCommand.id, true).finally(() => setCommandSubmitting(false)) }}>确认运行</button>
              <button type="button" disabled={sending || Boolean(streaming) || commandSubmitting} onClick={() => { if (!onResolveCommand) return; setCommandSubmitting(true); void onResolveCommand(pendingCommand.id, false).finally(() => setCommandSubmitting(false)) }}>拒绝</button></div>}
          </div>}
          {!compact && onCreateNoteDraft && message.role === 'agent' && !message.frame && Boolean(message.citations?.some(citation => records.some(record => record.id === citation.recordId && !record.demo))) && !dismissedNotes.includes(message.id) && <div className="message-note-offer"><span>将这次分析整理成笔记？</span><div><button type="button" onClick={() => onCreateNoteDraft(message)}>整理为笔记<span className="mono-note-gear"><GearIcon kind="center" /></span></button><button type="button" onClick={() => setDismissedNotes(current => [...current, message.id])}>暂不整理</button></div></div>}
        </ChamferCard>
      </li>)}
      {streaming && <>
        <li className="message-row" data-role="user" data-streaming="true"><ChamferCard className="message-card"><div className="message-meta"><span>你</span><span>刚刚</span></div><div className="message-body"><p>{streaming.question}</p></div></ChamferCard></li>
        <li className="message-row" data-role="agent" data-streaming="true"><ChamferCard className="message-card"><div className="message-meta"><span>GearNote</span><span role="status">{streamingCapture ? '正在截取' : streaming.answer ? '正在回答' : streaming.activity || '等待回复'}</span></div><StreamingRun streaming={streaming} /><div className="message-body"><p>{streamingCapture ? '正在从视频取帧…' : streaming.answer || streaming.activity || '等待模型正文…'}</p></div>{!streamingCapture && !streaming.answer && <div className="mono-thinking-gear" aria-hidden="true"><GearIcon kind="center" size={20} /></div>}</ChamferCard></li>
      </>}
    </ol>

    {!compact && <GearDivider area="composer" />}
    <div className="chat-composer" id="cmd-01">
      <div className="composer-head"><label htmlFor="agent-draft"><MechanicalGear className="mech-inline-gear" teeth={12} speed="10s" />向 Agent 提问</label><div>{onPreviewContext && <button className="context-preview-toggle" type="button" aria-expanded={contextPreviewOpen} disabled={sending || contextPreviewBusy || !draft.trim() || Boolean(intent) || isCaptureCommand(draft)} onClick={() => { void previewContext() }}>{contextPreviewBusy ? '读取中' : contextPreviewOpen ? '收起上下文' : '本轮上下文'}</button>}<span data-available={canSend}>{sending ? streamingCapture ? '正在截取' : '正在回答' : canSend ? '可以发送' : '尚未接入'}</span></div></div>
      {selectedIntent && <div className="composer-intent" role="status">
        <span className="composer-intent-badge"><IntentIcon intent={selectedIntent.intent} />{selectedIntent.label}</span>
        <span className="composer-intent-hint">{selectedIntent.hint}</span>
        <button type="button" disabled={sending} aria-label="取消操作入口" onClick={() => setIntent(null)}>取消</button>
      </div>}
      {attachedFrame && <div className="composer-frame">{attachedFramePreview && <img src={attachedFramePreview} alt="待发送的视频截图" />}<span>已选截图 / {formatElapsed(attachedFrame.atSeconds)} · 发送时会交给当前模型</span><button type="button" disabled={sending} aria-label="移除已选截图" onClick={() => { setAttachedFrame(null); setContextPreview(null); setContextPreviewOpen(false) }}>移除</button></div>}
      {referenceCount > 0 && <details className="reference-review" aria-label="本轮参考">
        <summary className="reference-review-summary" title={referenceLabel}><strong>本轮参考</strong><span>{referenceLabel}</span><small aria-live="polite">{referenceCount} 条</small></summary>
        {referencedRecords.length > 0 && <div className="reference-review-section"><div className="reference-review-head"><strong>资料</strong><span>{referencedRecords.length} 条</span></div>
        <ul>{referencedRecords.map(record => <li key={record.id}>
          <button className="reference-review-open" type="button" title={`定位资料：${record.title}`} onClick={() => onRevealRecord(record.id)}><span aria-hidden="true">{record.type === 'video' ? '■' : '◆'}</span><span>{record.title}{record.status !== 'ready' && <small> · 当前不可用于问答</small>}</span></button>
          <button className="reference-review-remove" type="button" disabled={sending} aria-label={`移出本轮参考：${record.title}`} onClick={() => onRemoveReference(record.id)}>移除</button>
        </li>)}</ul></div>}
        {selectedMessages.length > 0 && <div className="reference-review-section"><div className="reference-review-head"><strong>对话消息</strong><span>{selectedMessages.length} / 4 条</span></div>
        <ul>{selectedMessages.map(message => <li key={message.id}>
          <button className="reference-review-open" type="button" title="定位这条消息" onClick={() => document.getElementById(message.id)?.scrollIntoView({ behavior: 'smooth', block: 'center' })}><span aria-hidden="true">◇</span><span><b>{message.role === 'agent' ? 'Agent 回答' : '你'}</b> · {message.paragraphs.join(' ').slice(0, 140)}{message.paragraphs.join(' ').length > 140 ? '…' : ''}</span></button>
          <button className="reference-review-remove" type="button" disabled={sending} aria-label="移出本轮参考" onClick={() => toggleMessageReference(message.id)}>移除</button>
        </li>)}</ul>
        <small className="message-reference-note">仅引用所选消息的文字。Agent 回答会标明来源于对话，不代表已核实的外部事实。</small></div>}
      </details>}
      {slashOpen && <div className="slash-menu" id="composer-slash-menu" role="listbox" aria-label="斜杠命令菜单">
        <div className="slash-menu-head"><span>操作模式</span><span>↑↓ 选择 · Enter 确认 · Esc 关闭</span></div>
        {slashOptions.map((entry, index) => <button
          key={entry.id}
          id={`slash-option-${entry.id}`}
          type="button"
          role="option"
          aria-selected={index === activeSlashIndex}
          className={`slash-option${index === activeSlashIndex ? ' is-active' : ''}`}
          onMouseDown={event => event.preventDefault()}
          onMouseEnter={() => setSlashIndex(index)}
          onClick={() => applySlashAction(entry.id)}
        >
          <IntentIcon intent={entry.id} />
          <strong>{entry.label}</strong>
          <code>{entry.slash}</code>
          <span>{entry.shortHint}</span>
        </button>)}
      </div>}
      <textarea
        id="agent-draft"
        ref={draftRef}
        value={draft}
        readOnly={sending}
        aria-expanded={slashOpen || undefined}
        aria-controls={slashOpen ? 'composer-slash-menu' : undefined}
        aria-activedescendant={slashOpen ? `slash-option-${slashOptions[activeSlashIndex].id}` : undefined}
        onChange={event => {
          const next = event.target.value
          setDraft(next)
          if (!next.startsWith('/')) setSlashDismissed(false)
          setSlashIndex(0)
          setContextPreview(null)
          setContextPreviewOpen(false)
        }}
        onKeyDown={event => {
          if (event.ctrlKey && event.key === 'Enter') {
            event.preventDefault()
            void send()
            return
          }
          if (!slashOpen) return
          if (event.key === 'ArrowDown') {
            event.preventDefault()
            setSlashIndex(current => (current + 1) % slashOptions.length)
          } else if (event.key === 'ArrowUp') {
            event.preventDefault()
            setSlashIndex(current => (current - 1 + slashOptions.length) % slashOptions.length)
          } else if ((event.key === 'Enter' || event.key === 'Tab') && !event.shiftKey && !event.altKey && !event.metaKey && !event.nativeEvent.isComposing) {
            event.preventDefault()
            applySlashAction(slashOptions[activeSlashIndex].id)
          } else if (event.key === 'Escape') {
            event.preventDefault()
            setSlashDismissed(true)
          }
        }}
        placeholder="提问、输入 / 选择操作入口，或输入「截取 01:20 的画面」"
      />
      {streaming?.reading && streaming.requestId && <div className="reading-stop-control"><span role="status">自动读取中，达到任务预算后会回答。停止后保留已读部分。</span><HudButton type="button" disabled={stoppingReading} onClick={() => {
        setStoppingReading(true); setStopReadingError('')
        void stopAgentReading(streaming.requestId!).catch(error => { setStopReadingError(String(error)); setStoppingReading(false) })
      }}>{stoppingReading ? '正在转入回答…' : '停止阅读并回答'}</HudButton>{stopReadingError && <p role="alert">{stopReadingError}</p>}</div>}
      {deepEligible && <label className="deep-mode-toggle"><input type="checkbox" checked={deepMode} disabled={sending} onChange={event => { setDeepMode(event.target.checked); if (event.target.checked) setIntent(null); setContextPreview(null); setContextPreviewOpen(false) }} /><span>深度模式</span><small>顺序自动读取字幕；阅读策略与整项预算可在设置中调整，可能增加 API 用量</small></label>}
      {contextPreviewError && <p className="chat-error" role="alert">{contextPreviewError}</p>}
      {contextPreviewOpen && contextPreview && <div className="context-inspector" aria-label="本轮上下文预览">
        <div className="context-inspector-head"><strong>本轮上下文 / 发送前预览</strong><span>{contextPreview.textChars.toLocaleString()} / {contextPreview.limitChars.toLocaleString()} 字符{contextPreview.reservedChars > 0 ? ` · 预留 ${contextPreview.reservedChars.toLocaleString()}` : ''}</span></div>
        <p>参考资料 {contextPreview.referencedRecordCount} 条 · 对话消息 {contextPreview.referencedMessageCount} 条 · {activeDeepMode ? `字幕约 ${contextPreview.candidateChunkCount} 批 · 待逐批读取` : contextPreview.explorationMode ? `目录候选 ${contextPreview.candidateChunkCount} 段 · 正文待工具读取` : `实际片段 ${contextPreview.sources.length} / 候选 ${contextPreview.candidateChunkCount}`}{contextPreview.imageAttached ? ' · 附带图片' : ''}</p>
        {contextPreview.semanticPending && <p>发送后会尝试语义排序；这里暂显示词法预览，实际片段可能变化，并会增加向量 API 用量。</p>}
        {(contextPreview.omittedHistoryCount > 0 || contextPreview.shortenedHistoryCount > 0 || contextPreview.shortenedSourceCount > 0) && <p>较早消息未纳入 {contextPreview.omittedHistoryCount} 条 · 缩短历史 {contextPreview.shortenedHistoryCount} 条 · 缩短片段 {contextPreview.shortenedSourceCount} 条</p>}
        <p>字符上限用于控制文本长度，不等于模型的 token 数；图片另计。</p>
        <details><summary>人格与回答规则</summary><pre>{contextPreview.systemText}</pre></details>
        <details><summary>近期对话 · {contextPreview.history.length} 条</summary>{contextPreview.history.length === 0 ? <p>无近期对话</p> : contextPreview.history.map((item, index) => <p key={index}><strong>{item.role}</strong>（{item.createdAt}）：{item.text}</p>)}</details>
        <details><summary>项目记忆 · {contextPreview.projectMemory.length} 条</summary>{contextPreview.projectMemory.map(item => <p key={item.id}>{item.content}（用户确认于 {item.confirmedAt}）</p>)}</details>
        <details><summary>会话计划</summary><pre>目标：{contextPreview.workingContext.goal}\n已完成：{contextPreview.workingContext.completed}\n下一步：{contextPreview.workingContext.nextStep}</pre></details>
        <details><summary>较早对话摘要</summary><pre>{contextPreview.compactedSummary || '无'}</pre></details>
        {contextPreview.explorationMode ? <details open><summary>{activeDeepMode ? '深度模式 · 字幕待逐批读取' : '资料目录 · 正文待读取'}</summary><pre>{contextPreview.directoryText}</pre></details> : <details open><summary>资料片段 · {contextPreview.sources.length} 条</summary>{contextPreview.sources.map(source => <div className="context-preview-source" key={source.index}><strong>{source.label} {source.title}{source.atSeconds == null ? '' : ` / ${formatElapsed(source.atSeconds)}`}{source.visual ? ' · 图片' : ''}{source.selectionReason === 'coverage' ? ' · 覆盖式选取' : ''}</strong><small>捕获时间：{source.capturedAt || '未知'} · 来源：{source.sourceUrl || '未记录'}</small><pre>{source.text}</pre></div>)}</details>}
      </div>}
      {error && <p className="chat-error" role="alert">{error}</p>}
      <div className="composer-foot">
        <div className="composer-intents" role="group" aria-label="操作入口">
          {intentEntries.map(entry => <button
            key={entry.intent}
            type="button"
            className={`intent-chip${intent === entry.intent ? ' is-active' : ''}`}
            aria-pressed={intent === entry.intent}
            title={`${entry.hint}（输入 ${entry.slash}）`}
            disabled={intentsDisabled}
            onClick={() => { setIntent(current => current === entry.intent ? null : entry.intent); setContextPreview(null); setContextPreviewOpen(false) }}
          ><IntentIcon intent={entry.intent} /><span>{entry.label}</span></button>)}
        </div>
        <div className="composer-actions">
          <HudButton type="button" onClick={onShowHistory}>＋ 添加参考</HudButton>
          {onSummarize && <HudButton type="button" disabled={!agentAvailable || sending || referencedRecords.length === 0} onClick={() => onSummarize?.(activeDeepMode)}>总结</HudButton>}
          {!agentAvailable && onOpenAgentSetup && <HudButton type="button" onClick={onOpenAgentSetup}>配置 Agent</HudButton>}
          <HudButton type="button" variant="primary" disabled={!canSend || sending || !draft.trim() || (captureDraft && referencedRecords.length === 0)} onClick={() => { void send() }}>{sending ? (streaming ? streamingCapture : captureDraft) ? '截取中' : '生成中' : <><span className="mono-send-plane"><PaperPlaneIcon /></span>发送<span className="hud-send-arrow"> →</span></>}</HudButton>
        </div>
      </div>
    </div>
    {confirmDialog}
  </section>
}
