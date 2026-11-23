// 管理会话、Agent 发送流程和待确认操作。
import { useCallback, useEffect, useRef, useState } from 'react'
import { isTauri } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { askAgent, getAgentStatus, getChatState, resolveCommandProposal, resolveLibrarySearch, setChatReferences,
  suggestChatFollowups, type AgentStatus, type ChatSessionSummary, type ChatState } from '../agent'
import type { ChatFrame, ChatMessage, ChatStreaming } from '../chat-data'
import { rejectNoteEdit } from '../notes'

export function useChat(openChat: () => void, applyNoteEditAndRefresh: (proposalId: string) => Promise<unknown>) {
  const [referencedIds, setReferencedIds] = useState<string[]>([])
  const [agentStatus, setAgentStatus] = useState<AgentStatus | null>(null)
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [pendingChoice, setPendingChoice] = useState<ChatState['pendingChoice']>(null)
  const [pendingLibrarySearch, setPendingLibrarySearch] = useState<ChatState['pendingLibrarySearch']>(null)
  const [pendingChatNoteEdit, setPendingChatNoteEdit] = useState<ChatState['pendingChatNoteEdit']>(null)
  const [pendingCommand, setPendingCommand] = useState<ChatState['pendingCommand']>(null)
  const [sessions, setSessions] = useState<ChatSessionSummary[]>([])
  const [activeSessionId, setActiveSessionId] = useState('')
  const [sessionError, setSessionError] = useState('')
  const [workingContext, setWorkingContext] = useState<ChatState['workingContext']>({ goal: '', completed: '', nextStep: '', updatedAt: '' })
  const [projectMemory, setProjectMemory] = useState<ChatState['projectMemory']>([])
  const [compactedSummary, setCompactedSummary] = useState('')
  const [compactedCount, setCompactedCount] = useState(0)
  const [sending, setSending] = useState(false)
  const [streaming, setStreaming] = useState<ChatStreaming | null>(null)
  const [suggestingMessageId, setSuggestingMessageId] = useState<string | null>(null)
  const [suggestionErrorId, setSuggestionErrorId] = useState<string | null>(null)
  const [agentError, setAgentError] = useState('')
  const sendingRef = useRef(false)

  const applyChatState = useCallback((state: ChatState) => {
    setActiveSessionId(state.activeId)
    setSessions(state.sessions)
    setMessages(state.messages)
    setPendingChoice(state.pendingChoice)
    setPendingLibrarySearch(state.pendingLibrarySearch)
    setPendingChatNoteEdit(state.pendingChatNoteEdit)
    setPendingCommand(state.pendingCommand)
    setReferencedIds(state.referencedIds)
    setWorkingContext(state.workingContext)
    setProjectMemory(state.projectMemory)
    setCompactedSummary(state.compactedSummary)
    setCompactedCount(state.compactedCount)
  }, [])

  const sendQuestion = async (question: string, frameId?: string, messageIds: string[] = [], choiceId?: string, deepMode = false, referenceOverride?: string[], continueMessageId?: string) => {
    if (sendingRef.current) return false
    sendingRef.current = true
    setSending(true)
    setStreaming({ question, answer: '', startedAt: Date.now(), activities: [] })
    setSuggestionErrorId(null)
    setAgentError('')
    try {
      const reply = await askAgent(question, referenceOverride ?? referencedIds, delta => setStreaming(current => current && { ...current, answer: current.answer + delta }), frameId,
        activity => setStreaming(current => current && { ...current, activity, activities: [...current.activities, activity] }), messageIds, choiceId, deepMode, continueMessageId, (requestId, reading) => setStreaming(current => current && { ...current, requestId, reading }))
      const saved = await getChatState()
      applyChatState(saved)
      setStreaming(null)
      if (!reply.frame && !reply.toolSteps?.some(step => (step.startsWith('深度阅读失败：') || step.startsWith('回答未完成：'))) && !(saved.pendingChoice?.status === 'open' && saved.pendingChoice.messageId === reply.id) && !(saved.pendingLibrarySearch?.status === 'open' && saved.pendingLibrarySearch.messageId === reply.id) && !(saved.pendingChatNoteEdit?.proposal.status === 'pending' && saved.pendingChatNoteEdit.messageId === reply.id) && !(saved.pendingCommand?.status === 'open' && saved.pendingCommand.messageId === reply.id)) {
        setSuggestingMessageId(reply.id)
        void suggestChatFollowups(saved.activeId, reply.id)
          .then(async () => applyChatState(await getChatState()))
          .catch(() => setSuggestionErrorId(reply.id))
          .finally(() => setSuggestingMessageId(current => current === reply.id ? null : current))
      }
      return true
    } catch (error) {
      try { applyChatState(await getChatState()) } catch { /* Keep the original send error visible. */ }
      setStreaming(null)
      setAgentError(String(error))
      return false
    } finally {
      sendingRef.current = false
      setSending(false)
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
    sendingRef.current = true
    setSending(true)
    setStreaming({ question, answer: '', startedAt: Date.now(), activities: [] })
    setAgentError('')
    try {
      const reply = await askAgent(question, [recordId], () => {})
      if (!reply.frame) throw new Error('取帧没有返回图片')
      applyChatState(await getChatState())
      return reply.frame
    } catch (error) {
      setAgentError(String(error))
      throw error
    } finally {
      setStreaming(null)
      setSending(false)
      sendingRef.current = false
    }
  }
  const decideLibrarySearch = async (proposalId: string, accept: boolean, question: string) => {
    try {
      const updated = await resolveLibrarySearch(proposalId, accept)
      applyChatState(updated)
      if (accept) await sendQuestion(question, undefined, [], undefined, false, updated.referencedIds)
    } catch (cause) { setAgentError(String(cause)) }
  }
  const decideChatNoteEdit = async (proposalId: string, accept: boolean) => {
    try {
      if (accept) await applyNoteEditAndRefresh(proposalId)
      else await rejectNoteEdit(proposalId)
      applyChatState(await getChatState())
    } catch (cause) { setAgentError(String(cause)) }
  }
  const decideCommand = async (proposalId: string, accept: boolean) => {
    try { applyChatState(await resolveCommandProposal(proposalId, accept)) }
    catch (cause) { setAgentError(String(cause)); try { applyChatState(await getChatState()) } catch { /* Preserve command error. */ } }
  }
  const manageSession = async (action: () => Promise<ChatState>) => {
    setSessionError('')
    try { applyChatState(await action()); openChat() }
    catch (error) { setSessionError(String(error)) }
  }

  useEffect(() => {
    if (!isTauri()) return
    let disposed = false
    const stops: UnlistenFn[] = []
    void Promise.all([getAgentStatus(), getChatState()]).then(([status, saved]) => {
      if (!disposed) { setAgentStatus(status); applyChatState(saved) }
    }).catch(() => { if (!disposed) setAgentError('对话或模型配置读取失败') })
    void listen('knowflow:chat-changed', () => {
      if (!sendingRef.current) void getChatState().then(saved => { if (!disposed) applyChatState(saved) })
    }).then(stop => { if (disposed) stop(); else stops.push(stop) })
    return () => { disposed = true; stops.forEach(stop => stop()) }
  }, [applyChatState])

  useEffect(() => {
    if (!isTauri() || !activeSessionId) return
    void setChatReferences(activeSessionId, referencedIds).catch(error => setSessionError(String(error)))
  }, [activeSessionId, referencedIds])

  return { referencedIds, setReferencedIds, agentStatus, setAgentStatus, messages, pendingChoice, pendingLibrarySearch,
    pendingChatNoteEdit, pendingCommand, sessions, activeSessionId, sessionError, workingContext, projectMemory,
    compactedSummary, compactedCount, sending, streaming, suggestingMessageId, suggestionErrorId, agentError,
    sendQuestion, captureStoryboardFrame, decideLibrarySearch, decideChatNoteEdit, decideCommand, manageSession }
}
