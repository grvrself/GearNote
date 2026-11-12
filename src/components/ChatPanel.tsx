import { useEffect, useRef, useState } from 'react'
import type { ChatFrame, ChatMessage } from '../chat-data'
import type { KnowledgeRecord } from '../data'
import type { ContextPreview, ProjectMemory, WorkingContext, WorkingSuggestion, WorkingWorkflowEvent } from '../agent'
import { isCaptureCommand } from '../capture-command'
import { CapturedFrame } from './CapturedFrame'
import { ChamferCard } from './ChamferCard'
import { GearDivider, GearIcon, PaperPlaneIcon } from './GearIcon'
import { HudButton } from './HudButton'
import { WorkingSuggestionPanel } from './WorkingSuggestionPanel'
import './chat.css'

type Props = {
  records: KnowledgeRecord[]
  sessionId?: string
  referencedIds: string[]
  workingContext?: WorkingContext
  projectMemory?: ProjectMemory[]
  compactedSummary?: string
  compactedCount?: number
  onCompactContext?: () => Promise<void>
  onSuggestWorkingContext?: () => Promise<WorkingSuggestion>
  onApplyWorkingSuggestion?: (proposalId: string) => Promise<void>
  onUndoWorkingSuggestion?: (proposalId: string) => Promise<void>
  onRejectWorkingSuggestion?: (proposalId: string) => Promise<void>
  onGetPendingWorkingSuggestion?: (sessionId: string) => Promise<WorkingSuggestion | null>
  onListWorkingWorkflowEvents?: (sessionId: string) => Promise<WorkingWorkflowEvent[]>
  onSaveWorkingContext?: (goal: string, completed: string, nextStep: string) => Promise<void>
  onConfirmMemory?: (content: string, source: string, expiresAt?: string) => Promise<void>
  onRevokeMemory?: (id: string) => Promise<void>
  historyOpen: boolean
  agentAvailable: boolean
  sending?: boolean
  streaming?: { question: string; answer: string; activity?: string } | null
  error?: string
  onSend?: (question: string, frameId?: string, messageIds?: string[]) => Promise<boolean>
  onPreviewContext?: (question: string, frameId?: string, messageIds?: string[]) => Promise<ContextPreview>
  onOpenAgentSetup?: () => void
  onSummarize?: () => void
  onRemoveReference: (id: string) => void
  onRevealRecord: (id: string, atSeconds?: number) => void
  onToggleHistory: () => void
  onShowHistory: () => void
  onCreateNoteDraft?: (message: ChatMessage) => void
  messages?: ChatMessage[]
  compact?: boolean
}

const timeFormatter = new Intl.DateTimeFormat('zh-CN', { hour: '2-digit', minute: '2-digit', hour12: false })

function formatElapsed(seconds: number) {
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor(seconds / 60)
  const remainder = seconds % 60
  return hours > 0
    ? `${String(hours).padStart(2, '0')}:${String(minutes % 60).padStart(2, '0')}:${String(remainder).padStart(2, '0')}`
    : `${String(minutes).padStart(2, '0')}:${String(remainder).padStart(2, '0')}`
}

export function ChatPanel({ records, sessionId, referencedIds, workingContext = { goal: '', completed: '', nextStep: '', updatedAt: '' }, projectMemory = [], compactedSummary = '', compactedCount = 0, onCompactContext, onSuggestWorkingContext, onApplyWorkingSuggestion, onUndoWorkingSuggestion, onRejectWorkingSuggestion, onGetPendingWorkingSuggestion, onListWorkingWorkflowEvents, onSaveWorkingContext = async () => {}, onConfirmMemory = async () => {}, onRevokeMemory = async () => {}, historyOpen, agentAvailable, sending = false, streaming = null, error = '', onSend, onPreviewContext, onOpenAgentSetup, onSummarize, onRemoveReference, onRevealRecord, onToggleHistory, onShowHistory, onCreateNoteDraft, messages = [], compact = false }: Props) {
  const [draft, setDraft] = useState('')
  const [attachedFrame, setAttachedFrame] = useState<ChatFrame | null>(null)
  const [selectedMessageIds, setSelectedMessageIds] = useState<string[]>([])
  const [dismissedNotes, setDismissedNotes] = useState<string[]>([])
  const [contextPreview, setContextPreview] = useState<ContextPreview | null>(null)
  const [contextPreviewOpen, setContextPreviewOpen] = useState(false)
  const [contextPreviewBusy, setContextPreviewBusy] = useState(false)
  const [contextPreviewError, setContextPreviewError] = useState('')
  const [memoryError, setMemoryError] = useState('')
  const [memoryPanelOpen, setMemoryPanelOpen] = useState(false)
  const [goal, setGoal] = useState(workingContext.goal)
  const [completed, setCompleted] = useState(workingContext.completed)
  const [nextStep, setNextStep] = useState(workingContext.nextStep)
  const [memoryDraft, setMemoryDraft] = useState('')
  const [memorySource, setMemorySource] = useState('')
  const [memoryExpiry, setMemoryExpiry] = useState('')
  const [contextBusy, setContextBusy] = useState(false)
  const draftRef = useRef<HTMLTextAreaElement>(null)
  const sessionIdRef = useRef(sessionId)
  sessionIdRef.current = sessionId
  const flowRef = useRef<HTMLOListElement>(null)
  const previewIdentity = `${sessionId ?? ''}\u0000${draft.trim()}\u0000${referencedIds.join('|')}\u0000${selectedMessageIds.join('|')}\u0000${attachedFrame?.imageId ?? ''}\u0000${workingContext.updatedAt}\u0000${projectMemory.map(item => item.id).join('|')}\u0000${compactedCount}`
  const previewIdentityRef = useRef(previewIdentity)
  previewIdentityRef.current = previewIdentity
  const referencedRecords = referencedIds.map(id => records.find(record => record.id === id)).filter((record): record is KnowledgeRecord => Boolean(record))
  const selectedMessages = selectedMessageIds.map(id => messages.find(message => message.id === id)).filter((message): message is ChatMessage => Boolean(message))
  const referenceCount = referencedRecords.length + selectedMessages.length
  const referenceLabel = referencedRecords[0]?.title ?? (selectedMessages[0] ? `${selectedMessages[0].role === 'agent' ? 'Agent 回答' : '你的消息'}：${selectedMessages[0].paragraphs.join(' ')}` : '')
  const usableMemory = projectMemory.filter(item => !item.expiresAt || Number(item.expiresAt) > Date.now())
  const hasBackground = usableMemory.length > 0 || Boolean(compactedSummary || workingContext.goal || workingContext.completed || workingContext.nextStep)
  const hasReference = referencedRecords.length > 0 || selectedMessages.length > 0 || Boolean(attachedFrame) || hasBackground
  const canSend = agentAvailable || isCaptureCommand(draft)

  useEffect(() => { setDraft(''); setAttachedFrame(null); setSelectedMessageIds([]); setDismissedNotes([]); setContextPreview(null); setContextPreviewOpen(false); setMemoryPanelOpen(false) }, [sessionId])
  useEffect(() => { setContextPreview(null); setContextPreviewOpen(false) }, [referencedIds.join('|')])
  useEffect(() => { setContextPreview(null); setContextPreviewOpen(false) }, [workingContext, projectMemory, compactedSummary])
  useEffect(() => { setGoal(workingContext.goal); setCompleted(workingContext.completed); setNextStep(workingContext.nextStep) }, [sessionId, workingContext])

  useEffect(() => {
    if (flowRef.current) flowRef.current.scrollTop = flowRef.current.scrollHeight
  }, [messages, streaming?.answer])

  const toggleMessageReference = (id: string) => {
    if (sending) return
    setSelectedMessageIds(current => current.includes(id) ? current.filter(value => value !== id) : current.length < 4 ? [...current, id] : current)
    setContextPreview(null)
    setContextPreviewOpen(false)
  }

  const send = async () => {
    const sendingSessionId = sessionId
    const question = draft.trim()
    const capture = isCaptureCommand(question)
    if (!question || !onSend || !(agentAvailable || capture) || sending || !hasReference || (capture && referencedRecords.length === 0)) return
    setDraft('')
    setContextPreview(null)
    setContextPreviewOpen(false)
    const sent = await onSend(question, capture ? undefined : attachedFrame?.imageId, capture ? [] : selectedMessageIds)
    if (sent && !capture) { setAttachedFrame(null); setSelectedMessageIds([]) }
    if (!sent && sessionIdRef.current === sendingSessionId) setDraft(question)
  }

  const previewContext = async () => {
    if (!onPreviewContext || !draft.trim() || contextPreviewBusy) return
    if (contextPreview && contextPreviewOpen) { setContextPreviewOpen(false); return }
    if (contextPreview) { setContextPreviewOpen(true); return }
    setContextPreviewBusy(true)
    setContextPreviewError('')
    const identity = previewIdentity
    try {
      const result = await onPreviewContext(draft.trim(), attachedFrame?.imageId, selectedMessageIds)
      if (previewIdentityRef.current !== identity) return
      setContextPreview(result)
      setContextPreviewOpen(true)
    } catch (cause) { if (previewIdentityRef.current === identity) setContextPreviewError(String(cause)) }
    finally { setContextPreviewBusy(false) }
  }

  return <section className="chat-panel" aria-label="Agent 对话">
    {(compact || referencedRecords.length > 0 || !historyOpen) && <div className="context-bar" id="ctx-01">
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
      {messages.length === 0 && <li className="chat-empty">从右侧选择已保存的文章或字幕，添加到本轮参考，然后在下方提问。</li>}
      {messages.map(message => <li className="message-row" data-role={message.role} key={message.id}>
        <ChamferCard className={`message-card${selectedMessageIds.includes(message.id) ? ' message-card-referenced' : ''}`} id={message.id}>
          <div className="message-meta"><span>{message.role === 'agent' ? 'GearNote' : '你'}</span><time dateTime={/^\d+$/.test(message.createdAt) ? new Date(Number(message.createdAt)).toISOString() : message.createdAt}>{timeFormatter.format(new Date(/^\d+$/.test(message.createdAt) ? Number(message.createdAt) : message.createdAt))}</time></div>
          <div className="message-reference-control"><button type="button" aria-pressed={selectedMessageIds.includes(message.id)} disabled={sending || (!selectedMessageIds.includes(message.id) && selectedMessageIds.length >= 4)} title={selectedMessageIds.includes(message.id) ? '移出本轮参考' : '将这条消息加入本轮参考'} onClick={() => toggleMessageReference(message.id)}>{selectedMessageIds.includes(message.id) ? '✓ 已选为参考 · 移除' : '＋ 选此消息为参考'}</button></div>
          <div className="message-body">{message.paragraphs.map((paragraph, index) => <p key={index}>{paragraph}</p>)}{message.points && <ol>{message.points.map(point => <li key={point}>{point}</li>)}</ol>}{message.frame && <CapturedFrame frame={message.frame} title={records.find(record => record.id === message.frame?.recordId)?.title ?? '视频'} selected={attachedFrame?.imageId === message.frame.imageId} onAttach={() => { setAttachedFrame(message.frame ?? null); setContextPreview(null); setContextPreviewOpen(false); draftRef.current?.focus() }} />}</div>
          {Boolean(message.toolSteps?.length) && <details className="tool-steps"><summary>资料探索 · {message.toolSteps?.length} 步</summary><ol>{message.toolSteps?.map((step, index) => <li key={index}>{step}</li>)}</ol></details>}
          {message.citations && <div className="message-citations"><span>引用</span><div>{message.citations.map((citation, index) => {
            const record = records.find(item => item.id === citation.recordId)
            if (citation.kind === 'chat' || citation.recordId.startsWith('chat:')) {
              const cited = messages.find(item => item.id === citation.recordId.slice(5))
              return cited && <button type="button" key={`${citation.recordId}-${citation.sourceIndex ?? index}`} title={`定位${cited.role === 'agent' ? 'Agent 回答' : '用户消息'}`} onClick={() => document.getElementById(cited.id)?.scrollIntoView({ behavior: 'smooth', block: 'center' })}><span aria-hidden="true">{citation.sourceIndex ? `[${citation.sourceIndex}]` : '↗'}</span>{cited.role === 'agent' ? 'Agent 回答' : '用户消息'}</button>
            }
            if (!record) return null
            return <button type="button" key={`${citation.recordId}-${citation.sourceIndex ?? index}`} title={`查看 ${record.title}`} onClick={() => onRevealRecord(citation.recordId, citation.atSeconds)}><span aria-hidden="true">{citation.sourceIndex ? `[${citation.sourceIndex}]` : '↗'}</span>{citation.atSeconds == null ? record.site : `${formatElapsed(citation.atSeconds)} ${citation.kind === 'image' ? '画面' : '字幕'}`}</button>
          })}</div></div>}
          {!compact && onCreateNoteDraft && message.role === 'agent' && !message.frame && Boolean(message.citations?.some(citation => records.some(record => record.id === citation.recordId && !record.demo))) && !dismissedNotes.includes(message.id) && <div className="message-note-offer"><span>将这次分析整理成笔记？</span><div><button type="button" onClick={() => onCreateNoteDraft(message)}>整理为笔记<span className="mono-note-gear"><GearIcon kind="center" /></span></button><button type="button" onClick={() => setDismissedNotes(current => [...current, message.id])}>暂不整理</button></div></div>}
        </ChamferCard>
      </li>)}
      {streaming && <>
        <li className="message-row" data-role="user" data-streaming="true"><ChamferCard className="message-card"><div className="message-meta"><span>你</span><span>刚刚</span></div><div className="message-body"><p>{streaming.question}</p></div></ChamferCard></li>
        <li className="message-row" data-role="agent" data-streaming="true"><ChamferCard className="message-card"><div className="message-meta"><span>GearNote</span><span role="status">{isCaptureCommand(streaming.question) ? '正在截取' : streaming.answer ? '正在回答' : streaming.activity || '等待回复'}</span></div><div className="message-body"><p>{isCaptureCommand(streaming.question) ? '正在从视频取帧…' : streaming.answer || streaming.activity || '等待模型正文…'}</p></div>{!isCaptureCommand(streaming.question) && !streaming.answer && <div className="mono-thinking-gear" aria-hidden="true"><GearIcon kind="center" size={20} /></div>}</ChamferCard></li>
      </>}
    </ol>

    {!compact && <GearDivider area="composer" />}
    <div className="chat-composer" id="cmd-01">
      <div className="composer-head"><label htmlFor="agent-draft">向 Agent 提问</label><div>{!compact && <button className="context-preview-toggle" type="button" aria-expanded={memoryPanelOpen} aria-controls="session-tools" onClick={() => setMemoryPanelOpen(current => !current)}>会话管理</button>}{onPreviewContext && <button className="context-preview-toggle" type="button" aria-expanded={contextPreviewOpen} disabled={sending || contextPreviewBusy || !draft.trim() || isCaptureCommand(draft) || !hasReference} onClick={() => { void previewContext() }}>{contextPreviewBusy ? '读取中' : contextPreviewOpen ? '收起上下文' : '本轮上下文'}</button>}<span data-available={canSend}>{sending ? isCaptureCommand(draft) ? '正在截取' : '正在回答' : canSend ? '可以发送' : '尚未接入'}</span></div></div>
      {attachedFrame && <div className="composer-frame"><span>已选截图 / {formatElapsed(attachedFrame.atSeconds)} · 发送时会交给当前模型</span><button type="button" disabled={sending} aria-label="移除已选截图" onClick={() => { setAttachedFrame(null); setContextPreview(null); setContextPreviewOpen(false) }}>移除</button></div>}
      {!compact && memoryPanelOpen && <div className="memory-panel" id="session-tools" aria-label="会话管理" onKeyDown={event => { if (event.key === 'Escape') setMemoryPanelOpen(false) }}>
        <div className="memory-panel-head"><strong>会话管理</strong><button type="button" aria-label="关闭会话管理" onClick={() => setMemoryPanelOpen(false)}>关闭</button></div>
        <div className="memory-controls">
        <details><summary>对话压缩 · 自动开启 · 已覆盖 {compactedCount} 条</summary><p>发送新问题前，较早对话积累较多时会自动压缩，并调用当前配置的模型。原消息仍保存在会话中；若自动压缩失败，本轮提问会继续使用近期对话。</p>{compactedSummary && <pre>{compactedSummary}</pre>}<button type="button" disabled={sending || contextBusy || !agentAvailable || messages.length - compactedCount <= 8 || !onCompactContext} onClick={() => { setMemoryError(''); setContextBusy(true); void onCompactContext?.().catch(cause => setMemoryError(String(cause))).finally(() => setContextBusy(false)) }}>{contextBusy ? '处理中' : '立即压缩较早对话'}</button></details>
        <WorkingSuggestionPanel sessionId={sessionId} workingContext={workingContext} messageCount={messages.length} agentAvailable={agentAvailable} sending={sending} busy={contextBusy} onBusyChange={setContextBusy} onSuggest={onSuggestWorkingContext} onApply={onApplyWorkingSuggestion} onUndo={onUndoWorkingSuggestion} onReject={onRejectWorkingSuggestion} onGetPending={onGetPendingWorkingSuggestion} onListEvents={onListWorkingWorkflowEvents} />
        <details><summary>当前会话工作状态</summary><label>目标<textarea value={goal} maxLength={1200} onChange={event => setGoal(event.target.value)} /></label><label>已完成<textarea value={completed} maxLength={1200} onChange={event => setCompleted(event.target.value)} /></label><label>下一步<textarea value={nextStep} maxLength={1200} onChange={event => setNextStep(event.target.value)} /></label><button type="button" disabled={sending} onClick={() => { setMemoryError(''); void onSaveWorkingContext(goal, completed, nextStep).catch(cause => setMemoryError(String(cause))) }}>保存工作状态</button></details>
        <details><summary>默认项目记忆 · {projectMemory.length} 条</summary><p>只有点击“确认保存”才会作为已确认记忆供其他会话使用。</p><textarea value={memoryDraft} maxLength={1000} placeholder="例如：GearNote 使用 Tauri 2，前端采用 React + TypeScript。" onChange={event => setMemoryDraft(event.target.value)} /><input value={memorySource} maxLength={500} placeholder="来源或说明（可选）" onChange={event => setMemorySource(event.target.value)} /><label>到期日（可选）<input type="date" value={memoryExpiry} onChange={event => setMemoryExpiry(event.target.value)} /></label><button type="button" disabled={!memoryDraft.trim() || sending} onClick={() => { setMemoryError(''); const expiresAt = memoryExpiry ? new Date(`${memoryExpiry}T23:59:59.999`).getTime().toString() : undefined; void onConfirmMemory(memoryDraft, memorySource, expiresAt).then(() => { setMemoryDraft(''); setMemorySource(''); setMemoryExpiry('') }).catch(cause => setMemoryError(String(cause))) }}>确认保存记忆</button>{projectMemory.map(item => <p key={item.id}><span>{item.content} · 用户确认于 {new Date(Number(item.confirmedAt)).toLocaleString('zh-CN')}{item.expiresAt ? ` · 到期 ${new Date(Number(item.expiresAt)).toLocaleDateString('zh-CN')}${Number(item.expiresAt) <= Date.now() ? '（已过期）' : ''}` : ''}</span><button type="button" disabled={sending} onClick={() => { setMemoryError(''); void onRevokeMemory(item.id).catch(cause => setMemoryError(String(cause))) }}>撤销</button></p>)}</details>
        </div>
        {memoryError && <p className="chat-error" role="alert">{memoryError}</p>}
      </div>}
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
      <textarea id="agent-draft" ref={draftRef} value={draft} readOnly={sending} onChange={event => { setDraft(event.target.value); setContextPreview(null); setContextPreviewOpen(false) }} onKeyDown={event => { if (event.ctrlKey && event.key === 'Enter') { event.preventDefault(); void send() } }} placeholder="提问，或输入「截取 01:20 的画面」" />
      {contextPreviewError && <p className="chat-error" role="alert">{contextPreviewError}</p>}
      {contextPreviewOpen && contextPreview && <div className="context-inspector" aria-label="本轮上下文预览">
        <div className="context-inspector-head"><strong>本轮上下文 / 发送前预览</strong><span>{contextPreview.textChars.toLocaleString()} / {contextPreview.limitChars.toLocaleString()} 字符{contextPreview.reservedChars > 0 ? ` · 预留 ${contextPreview.reservedChars.toLocaleString()}` : ''}</span></div>
        <p>参考资料 {contextPreview.referencedRecordCount} 条 · 对话消息 {contextPreview.referencedMessageCount} 条 · 实际片段 {contextPreview.sources.length} / 候选 {contextPreview.candidateChunkCount}{contextPreview.imageAttached ? ' · 附带图片' : ''}</p>
        {(contextPreview.omittedHistoryCount > 0 || contextPreview.shortenedHistoryCount > 0 || contextPreview.shortenedSourceCount > 0) && <p>较早消息未纳入 {contextPreview.omittedHistoryCount} 条 · 缩短历史 {contextPreview.shortenedHistoryCount} 条 · 缩短片段 {contextPreview.shortenedSourceCount} 条</p>}
        <p>字符上限用于控制文本长度，不等于模型的 token 数；图片另计。</p>
        <details><summary>人格与回答规则</summary><pre>{contextPreview.systemText}</pre></details>
        <details><summary>近期对话 · {contextPreview.history.length} 条</summary>{contextPreview.history.length === 0 ? <p>无近期对话</p> : contextPreview.history.map((item, index) => <p key={index}><strong>{item.role}</strong>（{item.createdAt}）：{item.text}</p>)}</details>
        <details><summary>项目记忆 · {contextPreview.projectMemory.length} 条</summary>{contextPreview.projectMemory.map(item => <p key={item.id}>{item.content}（用户确认于 {item.confirmedAt}）</p>)}</details>
        <details><summary>会话工作状态</summary><pre>目标：{contextPreview.workingContext.goal}\n已完成：{contextPreview.workingContext.completed}\n下一步：{contextPreview.workingContext.nextStep}</pre></details>
        <details><summary>较早对话摘要</summary><pre>{contextPreview.compactedSummary || '无'}</pre></details>
        <details open><summary>资料片段 · {contextPreview.sources.length} 条</summary>{contextPreview.sources.map(source => <div className="context-preview-source" key={source.index}><strong>[{source.index}] {source.title}{source.atSeconds == null ? '' : ` / ${formatElapsed(source.atSeconds)}`}{source.visual ? ' · 图片' : ''}</strong><small>捕获时间：{source.capturedAt || '未知'} · 来源：{source.sourceUrl || '未记录'}</small><pre>{source.text}</pre></div>)}</details>
      </div>}
      {error && <p className="chat-error" role="alert">{error}</p>}
      <div className="composer-foot"><span>{attachedFrame ? '图片已加入本次提问' : selectedMessages.length > 0 ? `${selectedMessages.length} 条对话消息已选中` : referencedRecords.length === 0 ? hasBackground ? '可依据记忆和工作状态提问' : '请先添加资料或选择消息' : `${referencedRecords.length} 条资料已选中`}</span><div><HudButton type="button" onClick={onShowHistory}>＋ 添加参考</HudButton>{onSummarize && <HudButton type="button" disabled={!agentAvailable || sending || referencedRecords.length === 0} onClick={onSummarize}>总结</HudButton>}{!agentAvailable && onOpenAgentSetup && <HudButton type="button" onClick={onOpenAgentSetup}>配置 Agent</HudButton>}<HudButton type="button" variant="primary" disabled={!canSend || sending || !draft.trim() || !hasReference || (isCaptureCommand(draft) && referencedRecords.length === 0)} onClick={() => { void send() }}>{sending ? isCaptureCommand(draft) ? '截取中' : '生成中' : <><span className="mono-send-plane"><PaperPlaneIcon /></span>发送<span className="hud-send-arrow"> →</span></>}</HudButton></div></div>
    </div>
  </section>
}
