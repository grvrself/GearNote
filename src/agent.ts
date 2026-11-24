import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { ChatMessage, ChatSuggestion } from './chat-data'
import type { NoteEditProposal } from './notes'

export type ReadingBudget = { toolCalls: number; explorationRounds: number; toolChars: number; deepSeconds: number; deepBatches: number; requestSeconds: number; answerChars: number; strategy: 'adaptive' | 'full'; effort: 'standard' | 'thorough' | 'custom' }
export const defaultReadingBudget: ReadingBudget = { toolCalls: 24, explorationRounds: 12, toolChars: 16000, deepSeconds: 180, deepBatches: 64, requestSeconds: 180, answerChars: 20000, strategy: 'adaptive', effort: 'standard' }
export const stopAgentReading = (requestId: string) => invoke<void>('stop_agent_reading', { requestId })
export function readingPreset(current: ReadingBudget, effort: ReadingBudget['effort']): ReadingBudget {
  if (effort === 'custom') return { ...current, effort }
  return { ...current, effort, ...(effort === 'standard'
    ? { toolCalls: 24, explorationRounds: 12, toolChars: 16000, deepSeconds: 180, deepBatches: 64 }
    : { toolCalls: 48, explorationRounds: 24, toolChars: 32000, deepSeconds: 600, deepBatches: 128 }) }
}
export const saveReadingBudget = (budget: ReadingBudget) => invoke<AgentStatus>('save_reading_budget', { budget })
export type AgentStatus = { baseUrl: string; model: string; exploreReferences: boolean; contextTier: 'small' | 'medium'; readingBudget: ReadingBudget;
  semanticRetrieval: boolean; embeddingBaseUrl: string; embeddingModel: string; embeddingHasKey: boolean;
  hasKey: boolean; keyPersistent: boolean; keyWarning: string | null }
export type ToolProbe = { supported: boolean; searched: boolean; read: boolean; elapsedMs: number; steps: string[]; reason: string }
export type SemanticProbe = { supported: boolean; dimensions: number; cases: number; topOne: number;
  recallThree: number; elapsedMs: number; reason: string }
export type ChatSessionSummary = { id: string; title: string; updatedAt: string; messageCount: number }
export type WorkingContext = { goal: string; completed: string; nextStep: string; updatedAt: string }
export type ProjectMemory = { id: string; content: string; source: string; createdAt: string; confirmedAt: string; expiresAt?: string | null; active: boolean }
export type ChoiceOption = { id: string; label: string; detail: string }
export type PendingChoice = { id: string; messageId: string; prompt: string; why: string; options: ChoiceOption[]; recommended: string | null; status: 'open' | 'answered' | 'dismissed'; selectedId: string | null; customText: string | null; answeredAt: string | null }
export type PendingLibrarySearch = { id: string; messageId: string; question: string; records: { id: string; title: string }[]; status: 'open' | 'accepted' | 'dismissed' }
export type PendingChatNoteEdit = { messageId: string; noteTitle: string; proposal: NoteEditProposal }
export type PendingCommand = { id: string; messageId: string; program: 'ffmpeg' | 'ffprobe' | 'yt-dlp'; args: string[]; effectiveArgs: string[]; status: 'open' | 'running' | 'finished' | 'failed' | 'rejected' }
export type ChatState = { activeId: string; sessions: ChatSessionSummary[]; messages: ChatMessage[]; referencedIds: string[]; workingContext: WorkingContext; projectMemory: ProjectMemory[]; compactedSummary: string; compactedCount: number; pendingChoice: PendingChoice | null; pendingLibrarySearch: PendingLibrarySearch | null; pendingChatNoteEdit: PendingChatNoteEdit | null; pendingCommand: PendingCommand | null }
export type PersonaInfo = { content: string; path: string; isDefault: boolean }
export type ContextPreview = {
  systemText: string
  question: string
  history: { role: string; text: string; createdAt: string }[]
  projectMemory: ProjectMemory[]
  workingContext: WorkingContext
  compactedSummary: string
  sources: { index: number; label: string; recordId: string; title: string; sourceUrl: string; capturedAt: string; atSeconds?: number; text: string; visual: boolean; selectionReason: 'search' | 'coverage' }[]
  explorationMode: boolean
  directoryText: string
  referencedRecordCount: number
  referencedMessageCount: number
  candidateChunkCount: number
  omittedHistoryCount: number
  shortenedHistoryCount: number
  shortenedSourceCount: number
  textChars: number
  limitChars: number
  reservedChars: number
  imageAttached: boolean
  semanticPending: boolean
}
type StreamDelta = { requestId: string; delta: string }
type ToolActivity = { requestId: string; message: string }

export const getAgentStatus = () => invoke<AgentStatus>('get_agent_status')
export const saveAgentConfig = (baseUrl: string, model: string, apiKey?: string) =>
  invoke<AgentStatus>('save_agent_config', { baseUrl, model, apiKey })
export const saveExploreReferences = (enabled: boolean) => invoke<AgentStatus>('save_explore_references', { enabled })
export const saveContextTier = (tier: 'small' | 'medium') => invoke<AgentStatus>('save_context_tier', { tier })
export const saveSemanticRetrieval = (enabled: boolean, baseUrl: string, model: string, apiKey?: string) =>
  invoke<AgentStatus>('save_semantic_retrieval', { enabled, baseUrl, model, apiKey })
export const testSemanticRetrieval = () => invoke<SemanticProbe>('test_semantic_retrieval')
export const testAgentTools = () => invoke<ToolProbe>('test_agent_tools')
export const listChatMessages = () => invoke<ChatMessage[]>('list_chat_messages')
export const getChatState = () => invoke<ChatState>('get_chat_state')
export const suggestChatFollowups = (sessionId: string, messageId: string) => invoke<ChatSuggestion[]>('suggest_chat_followups', { sessionId, messageId })
export const getChatFrameImage = (imageId: string) => invoke<string>('get_chat_frame_image', { imageId })
export const createChatSession = () => invoke<ChatState>('create_chat_session')
export const switchChatSession = (id: string) => invoke<ChatState>('switch_chat_session', { id })
export const renameChatSession = (id: string, title: string) => invoke<ChatState>('rename_chat_session', { id, title })
export const deleteChatSession = (id: string) => invoke<ChatState>('delete_chat_session', { id })
export const setChatReferences = (id: string, referencedIds: string[]) => invoke<ChatState>('set_chat_references', { id, referencedIds })
export const resolveLibrarySearch = (proposalId: string, accept: boolean) => invoke<ChatState>('resolve_library_search', { proposalId, accept })
export const resolveCommandProposal = (proposalId: string, accept: boolean) => invoke<ChatState>('resolve_command_proposal', { proposalId, accept })
export const previewAgentContext = (question: string, referencedIds: string[], frameId?: string, messageIds: string[] = [], deepMode = false) =>
  invoke<ContextPreview>('preview_agent_context', { question, referencedIds, frameId: frameId ?? null, messageIds, deepMode })
export async function askAgent(question: string, referencedIds: string[], onDelta: (delta: string) => void, frameId?: string, onActivity?: (message: string) => void, messageIds: string[] = [], choiceId?: string, deepMode = false, continueMessageId?: string, onReading?: (requestId: string, reading: boolean) => void, signal?: AbortSignal) {
  const requestId = crypto.randomUUID()
  const stops: (() => void)[] = []
  let disposed = false
  const cleanup = () => {
    if (disposed) return
    disposed = true
    stops.splice(0).forEach(stop => stop())
  }
  const subscribe = async <T>(eventName: string, handler: (payload: T) => void) => {
    const stop = await listen<T>(eventName, event => { if (!disposed) handler(event.payload) })
    if (disposed) stop()
    else stops.push(stop)
  }
  signal?.addEventListener('abort', cleanup, { once: true })
  try {
    if (signal?.aborted) throw new DOMException('The request was aborted', 'AbortError')
    await subscribe<StreamDelta>('knowflow:agent-stream', payload => {
      if (payload.requestId === requestId) onDelta(payload.delta)
    })
    await subscribe<ToolActivity>('knowflow:agent-tool', payload => {
      if (payload.requestId === requestId) onActivity?.(payload.message)
    })
    await subscribe<{ requestId: string; reading: boolean; message: string }>('knowflow:reading-state', payload => {
      if (payload.requestId === requestId) { onReading?.(requestId, payload.reading); onActivity?.(payload.message) }
    })
    if (signal?.aborted) throw new DOMException('The request was aborted', 'AbortError')
    return await invoke<ChatMessage>('ask_agent', { question, referencedIds, requestId, frameId: frameId ?? null, messageIds, choiceId: choiceId ?? null, deepMode, continueMessageId: continueMessageId ?? null })
  } finally {
    signal?.removeEventListener('abort', cleanup)
    cleanup()
  }
}

export const getPersona = () => invoke<PersonaInfo>('get_persona')
export const savePersona = (content: string) => invoke<PersonaInfo>('save_persona', { content })
export const getDefaultPersona = () => invoke<string>('get_default_persona')
export const reloadPersona = () => invoke<PersonaInfo>('reload_persona')
