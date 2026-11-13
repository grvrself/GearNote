import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { ChatMessage, ChatSuggestion } from './chat-data'

export type AgentStatus = { baseUrl: string; model: string; exploreReferences: boolean; contextTier: 'small' | 'medium'; hasKey: boolean; keyPersistent: boolean; keyWarning: string | null }
export type ToolProbe = { supported: boolean; searched: boolean; read: boolean; elapsedMs: number; steps: string[]; reason: string }
export type ChatSessionSummary = { id: string; title: string; updatedAt: string; messageCount: number }
export type WorkingContext = { goal: string; completed: string; nextStep: string; updatedAt: string }
export type ProjectMemory = { id: string; content: string; source: string; createdAt: string; confirmedAt: string; expiresAt?: string | null; active: boolean }
export type ChoiceOption = { id: string; label: string; detail: string }
export type PendingChoice = { id: string; messageId: string; prompt: string; why: string; options: ChoiceOption[]; recommended: string | null; status: 'open' | 'answered' | 'dismissed'; selectedId: string | null; customText: string | null; answeredAt: string | null }
export type PendingLibrarySearch = { id: string; messageId: string; question: string; records: { id: string; title: string }[]; status: 'open' | 'accepted' | 'dismissed' }
export type ChatState = { activeId: string; sessions: ChatSessionSummary[]; messages: ChatMessage[]; referencedIds: string[]; workingContext: WorkingContext; projectMemory: ProjectMemory[]; compactedSummary: string; compactedCount: number; pendingChoice: PendingChoice | null; pendingLibrarySearch: PendingLibrarySearch | null }
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
}
type StreamDelta = { requestId: string; delta: string }
type ToolActivity = { requestId: string; message: string }

export const getAgentStatus = () => invoke<AgentStatus>('get_agent_status')
export const saveAgentConfig = (baseUrl: string, model: string, apiKey?: string) =>
  invoke<AgentStatus>('save_agent_config', { baseUrl, model, apiKey })
export const saveExploreReferences = (enabled: boolean) => invoke<AgentStatus>('save_explore_references', { enabled })
export const saveContextTier = (tier: 'small' | 'medium') => invoke<AgentStatus>('save_context_tier', { tier })
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
export const previewAgentContext = (question: string, referencedIds: string[], frameId?: string, messageIds: string[] = [], deepMode = false) =>
  invoke<ContextPreview>('preview_agent_context', { question, referencedIds, frameId: frameId ?? null, messageIds, deepMode })
export async function askAgent(question: string, referencedIds: string[], onDelta: (delta: string) => void, frameId?: string, onActivity?: (message: string) => void, messageIds: string[] = [], choiceId?: string, deepMode = false) {
  const requestId = crypto.randomUUID()
  const stop = await listen<StreamDelta>('knowflow:agent-stream', event => {
    if (event.payload.requestId === requestId) onDelta(event.payload.delta)
  })
  let stopActivity: () => void
  try {
    stopActivity = await listen<ToolActivity>('knowflow:agent-tool', event => {
      if (event.payload.requestId === requestId) onActivity?.(event.payload.message)
    })
  } catch (error) {
    stop()
    throw error
  }
  try {
    return await invoke<ChatMessage>('ask_agent', { question, referencedIds, requestId, frameId: frameId ?? null, messageIds, choiceId: choiceId ?? null, deepMode })
  } finally {
    stop()
    stopActivity()
  }
}

export const getPersona = () => invoke<PersonaInfo>('get_persona')
export const savePersona = (content: string) => invoke<PersonaInfo>('save_persona', { content })
export const getDefaultPersona = () => invoke<string>('get_default_persona')
export const reloadPersona = () => invoke<PersonaInfo>('reload_persona')
