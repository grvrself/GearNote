import { useEffect, useState } from 'react'
import type { WorkingContext, WorkingSuggestion, WorkingWorkflowEvent } from '../agent'

type Props = {
  sessionId?: string
  workingContext: WorkingContext
  sending: boolean
  busy: boolean
  onBusyChange: (busy: boolean) => void
  onApply?: (proposalId: string) => Promise<void>
  onUndo?: (proposalId: string) => Promise<void>
  onReject?: (proposalId: string) => Promise<void>
  onGetPending?: (sessionId: string) => Promise<WorkingSuggestion | null>
  onListEvents?: (sessionId: string) => Promise<WorkingWorkflowEvent[]>
}

const fields = [
  { key: 'goal', label: '目标' },
  { key: 'completed', label: '已完成' },
  { key: 'nextStep', label: '下一步' },
] as const

export function WorkingSuggestionPanel({ sessionId, workingContext, sending, busy, onBusyChange, onApply, onUndo, onReject, onGetPending, onListEvents }: Props) {
  const [proposal, setProposal] = useState<WorkingSuggestion | null>(null)
  const [events, setEvents] = useState<WorkingWorkflowEvent[]>([])
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const visibleProposal = proposal?.sessionId === sessionId ? proposal : null
  const undoable = events.find(event => event.undoable)?.proposalId

  useEffect(() => {
    setProposal(null)
    setEvents([])
    setError('')
    setNotice('')
    if (!sessionId || !onGetPending || !onListEvents) return
    let active = true
    void Promise.all([onGetPending(sessionId), onListEvents(sessionId)])
      .then(([pending, recent]) => { if (active) { setProposal(pending); setEvents(recent) } })
      .catch(cause => { if (active) setError(`操作记录读取失败：${String(cause)}`) })
    return () => { active = false }
  }, [sessionId, workingContext.updatedAt, onGetPending, onListEvents])

  const refresh = async () => {
    if (sessionId && onListEvents) setEvents(await onListEvents(sessionId))
  }

  const run = async (action: () => Promise<void>) => {
    setError('')
    setNotice('')
    onBusyChange(true)
    try { await action() }
    catch (cause) { setError(String(cause)); await refresh().catch(() => {}) }
    finally { onBusyChange(false) }
  }

  if (!visibleProposal && events.length === 0 && !error) return null
  return <details className="working-flow"><summary>会话计划变更记录</summary>
    {visibleProposal && <p>这条旧计划提案仍待确认。逐项核对后，才会修改当前会话的计划。</p>}
    {visibleProposal && <div className="working-suggestion">
      {fields.filter(field => visibleProposal.before[field.key] !== visibleProposal.after[field.key]).map(field =>
        <div className="working-diff" key={field.key}><strong>{field.label}</strong><small>原内容</small><pre>{visibleProposal.before[field.key] || '（空）'}</pre><small>建议改为</small><pre>{visibleProposal.after[field.key] || '（空）'}</pre></div>)}
      {workingContext.updatedAt !== visibleProposal.before.updatedAt && <p>会话计划已变化，这条建议不能直接确认。</p>}
      <button type="button" disabled={sending || busy || !onApply || workingContext.updatedAt !== visibleProposal.before.updatedAt}
        onClick={() => { void run(async () => { await onApply?.(visibleProposal.id); setProposal(null); setNotice('已确认并更新会话计划。'); await refresh() }) }}>确认更新</button>
      <button type="button" disabled={sending || busy || !onReject}
        onClick={() => { void run(async () => { await onReject?.(visibleProposal.id); setProposal(null); setNotice('已放弃建议。'); await refresh() }) }}>放弃建议</button>
    </div>}
    {undoable && <button type="button" disabled={sending || busy || !onUndo}
      onClick={() => { void run(async () => { await onUndo?.(undoable); setNotice('已撤销这次会话计划修改。'); await refresh() }) }}>撤销上次确认的修改</button>}
    <div className="working-history"><strong>最近操作</strong>{events.length === 0 ? <p>暂无记录</p> : events.map(event =>
      <p key={event.id}><time>{new Date(Number(event.at)).toLocaleString('zh-CN')}</time> · {event.detail}</p>)}</div>
    {notice && <p role="status">{notice}</p>}
    {error && <p className="chat-error" role="alert">{error}</p>}
  </details>
}
