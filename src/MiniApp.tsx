import { isTauri } from '@tauri-apps/api/core'
import { emitTo, listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useEffect, useRef, useState } from 'react'
import { askAgent, getAgentStatus, getChatState, setChatReferences, type AgentStatus } from './agent'
import type { ChatMessage } from './chat-data'
import { ChatPanel } from './components/ChatPanel'
import { recordFixtures } from './data'
import type { KnowledgeRecord } from './data'
import { loadSavedRecords } from './records'
import {
  MINI_CONTEXT_EVENT, MINI_READY_EVENT, MINI_REMOVE_REFERENCE_EVENT, MINI_REVEAL_EVENT,
  returnToMainWindow,
  type MiniContextPayload, type MiniRemoveReferencePayload, type MiniRevealPayload,
} from './mini-window'
import './mini.css'

export default function MiniApp() {
  const [referencedIds, setReferencedIds] = useState<string[]>([])
  const [error, setError] = useState('')
  const [savedRecords, setSavedRecords] = useState<KnowledgeRecord[]>([])
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [activeSessionId, setActiveSessionId] = useState('')
  const [agentStatus, setAgentStatus] = useState<AgentStatus | null>(null)
  const [sending, setSending] = useState(false)
  const [streaming, setStreaming] = useState<{ question: string; answer: string; activity?: string } | null>(null)
  const sendingRef = useRef(false)
  const desktop = isTauri()
  const records = desktop ? savedRecords : recordFixtures.map(record => ({ ...record, demo: true }))

  useEffect(() => {
    if (!desktop) return
    let disposed = false
    let stop: UnlistenFn | undefined
    void Promise.all([getAgentStatus(), getChatState()]).then(([status, chat]) => {
      if (!disposed) { setAgentStatus(status); setActiveSessionId(chat.activeId); setMessages(chat.messages); setReferencedIds(chat.referencedIds) }
    }).catch(() => { if (!disposed) setError('对话或模型配置读取失败') })
    void listen('knowflow:chat-changed', () => {
      if (!sendingRef.current) void getChatState().then(chat => { if (!disposed) { setActiveSessionId(chat.activeId); setMessages(chat.messages); setReferencedIds(chat.referencedIds) } })
    }).then(unlisten => { if (disposed) unlisten(); else stop = unlisten })
    return () => { disposed = true; stop?.() }
  }, [desktop])

  const sendQuestion = async (question: string, frameId?: string, explore = false, messageIds: string[] = []) => {
    if (sendingRef.current) return false
    sendingRef.current = true
    setSending(true)
    setStreaming({ question, answer: '' })
    setError('')
    try {
      await askAgent(question, referencedIds, delta => setStreaming(current => current && { ...current, answer: current.answer + delta }), frameId, explore,
        activity => setStreaming(current => current && { ...current, activity }), messageIds)
      const chat = await getChatState()
      setActiveSessionId(chat.activeId)
      setMessages(chat.messages)
      setReferencedIds(chat.referencedIds)
      setStreaming(null)
      return true
    } catch (cause) {
      setStreaming(null)
      setError(String(cause))
      return false
    } finally { sendingRef.current = false; setSending(false) }
  }

  useEffect(() => {
    if (!desktop) return
    let disposed = false
    let unlisten: UnlistenFn | undefined
    const refresh = async () => {
      try {
        const saved = await loadSavedRecords()
        if (!disposed) setSavedRecords(saved)
      } catch {
        if (!disposed) setError('本地资料读取失败')
      }
    }
    void refresh()
    void listen('knowflow:records-changed', () => { void refresh() })
      .then(stop => { if (disposed) stop(); else unlisten = stop })
      .catch(() => { if (!disposed) setError('资料更新监听失败') })
    return () => { disposed = true; unlisten?.() }
  }, [desktop])

  useEffect(() => {
    if (!desktop) return
    let disposed = false
    let unlisten: UnlistenFn | undefined
    void listen<MiniContextPayload>(MINI_CONTEXT_EVENT, event => setReferencedIds(event.payload.referencedIds))
      .then(stop => {
        if (disposed) stop()
        else {
          unlisten = stop
          void emitTo('main', MINI_READY_EVENT).catch(() => setError('主窗口暂不可用'))
        }
      })
      .catch(() => setError('无法同步本轮参考资料'))
    return () => { disposed = true; unlisten?.() }
  }, [desktop])

  const showMain = async () => {
    if (!desktop) return
    try {
      if (!await returnToMainWindow()) setError('主窗口已关闭')
    } catch {
      setError('无法返回主窗口')
    }
  }

  const revealRecord = async (id: string, atSeconds?: number) => {
    if (!desktop) return
    try {
      await emitTo<MiniRevealPayload>('main', MINI_REVEAL_EVENT, { id, atSeconds })
      await showMain()
    } catch {
      setError('无法在主窗口定位资料')
    }
  }

  const removeReference = (id: string) => {
    const next = referencedIds.filter(value => value !== id)
    setReferencedIds(next)
    if (desktop && activeSessionId) void setChatReferences(activeSessionId, next).catch(() => setError('引用资料未能保存'))
    if (desktop) void emitTo<MiniRemoveReferencePayload>('main', MINI_REMOVE_REFERENCE_EVENT, { id }).catch(() => setError('引用资料未能同步'))
  }

  return <div className="mini-shell" id="mini-01">
    <header className="mini-titlebar" data-tauri-drag-region>
      <span className="mini-serial" data-tauri-drag-region>KF-001 / MINI-01</span>
      <span className="mini-title" data-tauri-drag-region>提问小窗</span>
      <span className="mini-pin" data-tauri-drag-region>TOP / ON</span>
      <div className="mini-window-controls" role="group" aria-label="小窗控制">
        <button type="button" aria-label="返回主窗口" title="返回主窗口" disabled={!desktop} onClick={() => { void showMain() }}><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 4h10v8H3zM6 8h7M8 6 6 8l2 2" /></svg></button>
        <button className="mini-window-close" type="button" aria-label="关闭小窗" title="关闭小窗" disabled={!desktop} onClick={() => { void getCurrentWindow().close() }}><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 3.5l9 9m0-9l-9 9" /></svg></button>
      </div>
    </header>
    {error && <div className="mini-error" role="alert"><span>{error}</span><button type="button" aria-label="关闭错误提示" onClick={() => setError('')}>×</button></div>}
    <ChatPanel compact records={records} sessionId={activeSessionId} referencedIds={referencedIds} historyOpen={false} agentAvailable={Boolean(agentStatus?.hasKey && agentStatus.model)} sending={sending} streaming={streaming} onSend={sendQuestion} onOpenAgentSetup={() => { void showMain() }} onRemoveReference={removeReference} onRevealRecord={revealRecord} onToggleHistory={() => { void showMain() }} onShowHistory={() => { void showMain() }} messages={messages} />
  </div>
}
