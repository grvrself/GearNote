// 主窗与小窗共用的会话、Agent 发送流程和待确认操作控制器。
import { useCallback, useEffect, useRef, useState, type Dispatch, type SetStateAction } from 'react'
import { isTauri } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { askAgent, getAgentStatus, getChatState, resolveCommandProposal, resolveLibrarySearch, setChatReferences,
  suggestChatFollowups, type AgentStatus, type ChatSessionSummary, type ChatState } from '../agent'
import type { ChatFrame, ChatMessage, ChatStreaming } from '../chat-data'
import { rejectNoteEdit } from '../notes'

const emptyWorkingContext: ChatState['workingContext'] = { goal: '', completed: '', nextStep: '', updatedAt: '' }
const sameReferences = (left: string[], right: string[]) => left.length === right.length && left.every((id, index) => id === right[index])

export function useChat(openChat: () => void, applyNoteEditAndRefresh: (proposalId: string) => Promise<unknown>) {
  const [referencedIds, setReferencedIdsState] = useState<string[]>([])
  const [agentStatus, setAgentStatus] = useState<AgentStatus | null>(null)
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [pendingChoice, setPendingChoice] = useState<ChatState['pendingChoice']>(null)
  const [pendingLibrarySearch, setPendingLibrarySearch] = useState<ChatState['pendingLibrarySearch']>(null)
  const [pendingChatNoteEdit, setPendingChatNoteEdit] = useState<ChatState['pendingChatNoteEdit']>(null)
  const [pendingCommand, setPendingCommand] = useState<ChatState['pendingCommand']>(null)
  const [readContinuation, setReadContinuation] = useState<ChatState['readContinuation']>(null)
  const [sessions, setSessions] = useState<ChatSessionSummary[]>([])
  const [activeSessionId, setActiveSessionId] = useState('')
  const [sessionError, setSessionError] = useState('')
  const [workingContext, setWorkingContext] = useState<ChatState['workingContext']>(emptyWorkingContext)
  const [projectMemory, setProjectMemory] = useState<ChatState['projectMemory']>([])
  const [compactedSummary, setCompactedSummary] = useState('')
  const [compactedCount, setCompactedCount] = useState(0)
  const [sending, setSending] = useState(false)
  const [streaming, setStreaming] = useState<ChatStreaming | null>(null)
  const [suggestingMessageId, setSuggestingMessageId] = useState<string | null>(null)
  const [suggestionErrorId, setSuggestionErrorId] = useState<string | null>(null)
  const [agentError, setAgentError] = useState('')

  const mountedRef = useRef(true)
  const sendingRef = useRef(false)
  const activeSessionIdRef = useRef('')
  const referencedIdsRef = useRef<string[]>([])
  const referenceRevisionRef = useRef(0)
  const operationRef = useRef(0)
  const stateReadRef = useRef(0)
  const suggestionRef = useRef(0)
  const sessionActionRef = useRef(0)
  const pendingEventRef = useRef(false)
  const initializedRef = useRef(false)
  const pendingReferencePersistRef = useRef(false)
  const abortRef = useRef<AbortController | null>(null)

  const applyChatState = useCallback((state: ChatState, referenceRevision = referenceRevisionRef.current) => {
    if (!mountedRef.current) return
    // 只有初始化完成后的会话变化才算“切换”；首次加载（空→实际会话）不强制覆盖本地引用，
    // 以免把初始化前到达的小窗引用同步丢掉。
    const sessionChanged = initializedRef.current && activeSessionIdRef.current !== state.activeId
    if (sessionChanged) {
      operationRef.current += 1
      suggestionRef.current += 1
      abortRef.current?.abort()
      abortRef.current = null
      sendingRef.current = false
      setSending(false)
      setStreaming(null)
      setSuggestingMessageId(null)
    }
    activeSessionIdRef.current = state.activeId
    setActiveSessionId(state.activeId)
    setSessions(state.sessions)
    setMessages(state.messages)
    setPendingChoice(state.pendingChoice)
    setPendingLibrarySearch(state.pendingLibrarySearch)
    setPendingChatNoteEdit(state.pendingChatNoteEdit)
    setPendingCommand(state.pendingCommand)
    setReadContinuation(state.readContinuation ?? null)
    if (sessionChanged || referenceRevision === referenceRevisionRef.current) {
      referencedIdsRef.current = state.referencedIds
      setReferencedIdsState(state.referencedIds)
    }
    setWorkingContext(state.workingContext)
    setProjectMemory(state.projectMemory)
    setCompactedSummary(state.compactedSummary)
    setCompactedCount(state.compactedCount)
    initializedRef.current = true
  }, [])

  const refreshChatState = useCallback(async (expectedOperation = operationRef.current) => {
    const read = ++stateReadRef.current
    const referenceRevision = referenceRevisionRef.current
    const state = await getChatState()
    if (!mountedRef.current || read !== stateReadRef.current || expectedOperation !== operationRef.current) return null
    applyChatState(state, referenceRevision)
    return state
  }, [applyChatState])

  const setReferencedIds: Dispatch<SetStateAction<string[]>> = useCallback(action => {
    const current = referencedIdsRef.current
    const next = typeof action === 'function' ? action(current) : action
    if (sameReferences(current, next)) return
    referenceRevisionRef.current += 1
    referencedIdsRef.current = next
    setReferencedIdsState(next)
    const sessionId = activeSessionIdRef.current
    if (!isTauri()) return
    if (!sessionId) {
      // 会话尚未确认（初始化未完成）：保留最新本地值，待会话确认后补写，不直接丢弃。
      pendingReferencePersistRef.current = true
      return
    }
    void setChatReferences(sessionId, next).catch(error => {
      if (mountedRef.current && activeSessionIdRef.current === sessionId) setSessionError(String(error))
    })
  }, [])

  const updateStreaming = useCallback((operation: number, sessionId: string, update: (current: ChatStreaming | null) => ChatStreaming | null) => {
    if (!mountedRef.current || operation !== operationRef.current || activeSessionIdRef.current !== sessionId) return
    setStreaming(update)
  }, [])

  const startSuggestions = useCallback((sessionId: string, messageId: string, operation: number) => {
    const suggestion = ++suggestionRef.current
    setSuggestingMessageId(messageId)
    void suggestChatFollowups(sessionId, messageId)
      .then(() => {
        // 过时的建议不得发起新读取，避免把较新请求的状态回读挤掉。
        if (mountedRef.current && suggestion === suggestionRef.current && operation === operationRef.current) return refreshChatState(operation)
      })
      .catch(() => {
        if (mountedRef.current && suggestion === suggestionRef.current && activeSessionIdRef.current === sessionId) setSuggestionErrorId(messageId)
      })
      .finally(() => {
        if (mountedRef.current && suggestion === suggestionRef.current) {
          setSuggestingMessageId(current => current === messageId ? null : current)
        }
      })
  }, [refreshChatState])

  const sendQuestion = async (question: string, frameId?: string, messageIds: string[] = [], choiceId?: string, deepMode = false, referenceOverride?: string[], continueMessageId?: string) => {
    if (sendingRef.current || !mountedRef.current) return false
    const operation = ++operationRef.current
    suggestionRef.current += 1
    const sessionId = activeSessionIdRef.current
    const abort = new AbortController()
    abortRef.current = abort
    sendingRef.current = true
    setSending(true)
    setStreaming({ question, answer: '', startedAt: Date.now(), activities: [] })
    setSuggestionErrorId(null)
    setAgentError('')
    pendingEventRef.current = false
    try {
      const reply = await askAgent(question, referenceOverride ?? referencedIdsRef.current,
        delta => updateStreaming(operation, sessionId, current => current && { ...current, answer: current.answer + delta }), frameId,
        activity => updateStreaming(operation, sessionId, current => current && { ...current, activity, activities: [...current.activities, activity] }),
        messageIds, choiceId, deepMode, continueMessageId,
        (requestId, reading) => updateStreaming(operation, sessionId, current => current && { ...current, requestId, reading }), abort.signal)
      const saved = operation === operationRef.current ? await refreshChatState(operation) : null
      if (operation !== operationRef.current || !mountedRef.current) return true
      setStreaming(null)
      // 只用后端结构化结果决定是否生成后续建议：普通回答且完整/部分保留才触发；
      // 失败、选择题、截图和提案消息由 kind/answer 排除，展示文案修改不影响此判断。
      // 缺少 outcome 的历史回复保守跳过，不自动补发模型请求。
      const suggestable = reply.outcome?.kind === 'answer' && (reply.outcome.answer === 'completed' || reply.outcome.answer === 'partial')
      if (saved && saved.activeId === sessionId && suggestable
        && !(saved.pendingChoice?.status === 'open' && saved.pendingChoice.messageId === reply.id)
        && !(saved.pendingLibrarySearch?.status === 'open' && saved.pendingLibrarySearch.messageId === reply.id)
        && !(saved.pendingChatNoteEdit?.proposal.status === 'pending' && saved.pendingChatNoteEdit.messageId === reply.id)
        && !(saved.pendingCommand?.status === 'open' && saved.pendingCommand.messageId === reply.id)) {
        startSuggestions(sessionId, reply.id, operation)
      }
      return true
    } catch (error) {
      // 只有仍是当前操作时才补读状态并展示错误；过时请求不再发起读取。
      if (mountedRef.current && operation === operationRef.current) {
        try { await refreshChatState(operation) } catch { /* Keep the original send error visible. */ }
      }
      if (mountedRef.current && operation === operationRef.current) {
        setStreaming(null)
        setAgentError(String(error))
      }
      return false
    } finally {
      if (operation === operationRef.current) {
        sendingRef.current = false
        setSending(false)
        if (abortRef.current === abort) abortRef.current = null
        if (pendingEventRef.current) {
          pendingEventRef.current = false
          void refreshChatState().catch(() => { /* 后续事件会再次同步。 */ })
        }
      }
    }
  }

  const captureStoryboardFrame = async (recordId: string, atSeconds: number): Promise<ChatFrame> => {
    if (sendingRef.current) throw new Error('上一条提问仍在处理，请稍候')
    const seconds = Math.floor(atSeconds)
    if (!Number.isSafeInteger(seconds) || seconds < 0 || seconds > 864000) throw new Error('分镜时间无效')
    const stamp = seconds >= 3600
      ? `${String(Math.floor(seconds / 3600)).padStart(2, '0')}:${String(Math.floor(seconds / 60) % 60).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`
      : `${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`
    const question = `截取 ${stamp} 的画面`
    const operation = ++operationRef.current
    suggestionRef.current += 1
    const abort = new AbortController()
    abortRef.current = abort
    sendingRef.current = true
    setSending(true)
    setStreaming({ question, answer: '', startedAt: Date.now(), activities: [] })
    setAgentError('')
    try {
      const reply = await askAgent(question, [recordId], () => {}, undefined, undefined, [], undefined, false, undefined, undefined, abort.signal)
      if (!reply.frame) throw new Error('取帧没有返回图片')
      const saved = operation === operationRef.current ? await refreshChatState(operation) : null
      if (!saved) throw new Error('取帧后的会话状态已变化，请在原会话中查看图片')
      return reply.frame
    } catch (error) {
      if (mountedRef.current && operation === operationRef.current) setAgentError(String(error))
      throw error
    } finally {
      if (operation === operationRef.current) {
        setStreaming(null)
        setSending(false)
        sendingRef.current = false
        if (abortRef.current === abort) abortRef.current = null
        if (pendingEventRef.current) {
          pendingEventRef.current = false
          void refreshChatState().catch(() => { /* 后续事件会再次同步。 */ })
        }
      }
    }
  }

  const decideLibrarySearch = async (proposalId: string, accept: boolean, question: string) => {
    const operation = operationRef.current
    const referenceRevision = referenceRevisionRef.current
    try {
      const updated = await resolveLibrarySearch(proposalId, accept)
      if (!mountedRef.current || operation !== operationRef.current) return
      applyChatState(updated, referenceRevision)
      if (accept) {
        const ids = referenceRevision === referenceRevisionRef.current ? updated.referencedIds : referencedIdsRef.current
        await sendQuestion(question, undefined, [], undefined, false, ids)
      }
    } catch (cause) {
      if (mountedRef.current && operation === operationRef.current) setAgentError(String(cause))
    }
  }
  const decideChatNoteEdit = async (proposalId: string, accept: boolean) => {
    const operation = operationRef.current
    try {
      if (accept) await applyNoteEditAndRefresh(proposalId)
      else await rejectNoteEdit(proposalId)
      if (mountedRef.current && operation === operationRef.current) await refreshChatState(operation)
    } catch (cause) {
      if (mountedRef.current && operation === operationRef.current) setAgentError(String(cause))
    }
  }
  const decideCommand = async (proposalId: string, accept: boolean) => {
    const operation = operationRef.current
    const referenceRevision = referenceRevisionRef.current
    try {
      const updated = await resolveCommandProposal(proposalId, accept)
      if (mountedRef.current && operation === operationRef.current) applyChatState(updated, referenceRevision)
    } catch (cause) {
      if (mountedRef.current && operation === operationRef.current) {
        setAgentError(String(cause))
        try { await refreshChatState(operation) } catch { /* Preserve command error. */ }
      }
    }
  }
  const manageSession = async (action: () => Promise<ChatState>) => {
    const actionGeneration = ++sessionActionRef.current
    const operation = operationRef.current
    const referenceRevision = referenceRevisionRef.current
    setSessionError('')
    try {
      const updated = await action()
      if (!mountedRef.current || actionGeneration !== sessionActionRef.current || operation !== operationRef.current) return
      applyChatState(updated, referenceRevision)
      openChat()
    } catch (error) {
      if (mountedRef.current && actionGeneration === sessionActionRef.current) setSessionError(String(error))
    }
  }
  const clearAgentError = useCallback(() => setAgentError(''), [])

  useEffect(() => {
    mountedRef.current = true
    if (!isTauri()) return () => { mountedRef.current = false }
    let disposed = false
    const stops: UnlistenFn[] = []
    const initialRead = ++stateReadRef.current
    const initialReferences = referenceRevisionRef.current
    const initialOperation = operationRef.current
    void Promise.all([getAgentStatus(), getChatState()]).then(([status, saved]) => {
      if (disposed || initialRead !== stateReadRef.current || initialOperation !== operationRef.current) return
      setAgentStatus(status)
      applyChatState(saved, initialReferences)
      // 初始化前到达的引用同步（如小窗上下文事件）在会话确认后补写到后端。
      if (pendingReferencePersistRef.current && activeSessionIdRef.current) {
        pendingReferencePersistRef.current = false
        const sessionId = activeSessionIdRef.current
        void setChatReferences(sessionId, referencedIdsRef.current).catch(error => {
          if (mountedRef.current && activeSessionIdRef.current === sessionId) setSessionError(String(error))
        })
      }
    }).catch(() => { if (!disposed) setAgentError('对话或模型配置读取失败') })
    void listen('knowflow:chat-changed', () => {
      if (sendingRef.current) { pendingEventRef.current = true; return }
      void refreshChatState().catch(() => { if (!disposed) setAgentError('对话状态同步失败') })
    }).then(stop => { if (disposed) stop(); else stops.push(stop) })
      .catch(() => { if (!disposed) setAgentError('对话更新监听失败') })
    return () => {
      disposed = true
      mountedRef.current = false
      operationRef.current += 1
      suggestionRef.current += 1
      stateReadRef.current += 1
      sessionActionRef.current += 1
      abortRef.current?.abort()
      abortRef.current = null
      stops.forEach(stop => stop())
    }
  }, [applyChatState, refreshChatState])

  return { referencedIds, setReferencedIds, agentStatus, setAgentStatus, messages, pendingChoice, pendingLibrarySearch,
    pendingChatNoteEdit, pendingCommand, readContinuation, sessions, activeSessionId, sessionError, workingContext, projectMemory,
    compactedSummary, compactedCount, sending, streaming, suggestingMessageId, suggestionErrorId, agentError, clearAgentError,
    sendQuestion, captureStoryboardFrame, decideLibrarySearch, decideChatNoteEdit, decideCommand, manageSession }
}
