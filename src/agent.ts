import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { ChatMessage } from './chat-data'

export type AgentStatus = { baseUrl: string; model: string; hasKey: boolean; keyPersistent: boolean; keyWarning: string | null }
export type ToolProbe = { supported: boolean; searched: boolean; read: boolean; elapsedMs: number; steps: string[]; reason: string }
export type ChatSessionSummary = { id: string; title: string; updatedAt: string; messageCount: number }
export type WorkingContext = { goal: string; completed: string; nextStep: string; updatedAt: string }
export type ProjectMemory = { id: string; content: string; source: string; createdAt: string; confirmedAt: string; expiresAt?: string | null; active: boolean }
export type ChatState = { activeId: string; sessions: ChatSessionSummary[]; messages: ChatMessage[]; referencedIds: string[]; workingContext: WorkingContext; projectMemory: ProjectMemory[]; compactedSummary: string; compactedCount: number }
export type WorkingSuggestion = { sessionId: string; baseUpdatedAt: string; goal: string; completed: string; nextStep: string }
export type PersonaInfo = { content: string; path: string; isDefault: boolean }
export type ContextPreview = {
  systemText: string
  question: string
  history: { role: string; text: string; createdAt: string }[]
  projectMemory: ProjectMemory[]
  workingContext: WorkingContext
  compactedSummary: string
  sources: { index: number; recordId: string; title: string; sourceUrl: string; capturedAt: string; atSeconds?: number; text: string; visual: boolean }[]
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
export const testAgentTools = () => invoke<ToolProbe>('test_agent_tools')
export const listChatMessages = () => invoke<ChatMessage[]>('list_chat_messages')
export const getChatState = () => invoke<ChatState>('get_chat_state')
export const getChatFrameImage = (imageId: string) => invoke<string>('get_chat_frame_image', { imageId })
export const createChatSession = () => invoke<ChatState>('create_chat_session')
export const switchChatSession = (id: string) => invoke<ChatState>('switch_chat_session', { id })
export const renameChatSession = (id: string, title: string) => invoke<ChatState>('rename_chat_session', { id, title })
export const deleteChatSession = (id: string) => invoke<ChatState>('delete_chat_session', { id })
export const setChatReferences = (id: string, referencedIds: string[]) => invoke<ChatState>('set_chat_references', { id, referencedIds })
export const saveWorkingContext = (goal: string, completed: string, nextStep: string) => invoke<ChatState>('save_working_context', { goal, completed, nextStep })
export const confirmProjectMemory = (content: string, source = '', expiresAt?: string) => invoke<ChatState>('confirm_project_memory', { content, source, expiresAt: expiresAt ?? null })
export const revokeProjectMemory = (id: string) => invoke<ChatState>('revoke_project_memory', { id })
export const compactChatContext = () => invoke<ChatState>('compact_chat_context')
export const suggestWorkingContext = () => invoke<WorkingSuggestion>('suggest_working_context')
export const applyWorkingSuggestion = (suggestion: WorkingSuggestion) => invoke<ChatState>('apply_working_suggestion', { suggestion })
export const previewAgentContext = (question: string, referencedIds: string[], frameId?: string, explore = false, messageIds: string[] = []) =>
  invoke<ContextPreview>('preview_agent_context', { question, referencedIds, frameId: frameId ?? null, explore, messageIds })
export async function askAgent(question: string, referencedIds: string[], onDelta: (delta: string) => void, frameId?: string, explore = false, onActivity?: (message: string) => void, messageIds: string[] = []) {
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
    return await invoke<ChatMessage>('ask_agent', { question, referencedIds, requestId, frameId: frameId ?? null, explore, messageIds })
  } finally {
    stop()
    stopActivity()
  }
}

export const getPersona = () => invoke<PersonaInfo>('get_persona')
export const savePersona = (content: string) => invoke<PersonaInfo>('save_persona', { content })
export const getDefaultPersona = () => invoke<string>('get_default_persona')
export const reloadPersona = () => invoke<PersonaInfo>('reload_persona')
