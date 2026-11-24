import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { UnlistenFn } from '@tauri-apps/api/event'
import {
  askAgent,
  getAgentStatus,
  getChatState,
  setChatReferences,
  suggestChatFollowups,
  type AgentStatus,
  type ChatState,
} from '../agent'
import type { ChatMessage, ChatStreaming } from '../chat-data'
import { useChat } from './useChat'

const listenMock = vi.hoisted(() => vi.fn())

vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => true }))
vi.mock('@tauri-apps/api/event', () => ({ listen: listenMock }))
vi.mock('../agent', async importOriginal => {
  const actual = await importOriginal<typeof import('../agent')>()
  return {
    ...actual,
    askAgent: vi.fn(),
    getAgentStatus: vi.fn(),
    getChatState: vi.fn(),
    resolveCommandProposal: vi.fn(),
    resolveLibrarySearch: vi.fn(),
    setChatReferences: vi.fn(),
    suggestChatFollowups: vi.fn(),
  }
})
vi.mock('../notes', async importOriginal => ({ ...(await importOriginal<typeof import('../notes')>()), rejectNoteEdit: vi.fn() }))

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((res, rej) => { resolve = res; reject = rej })
  return { promise, resolve, reject }
}

function chatMessage(id: string, role: ChatMessage['role'] = 'agent', overrides: Partial<ChatMessage> = {}): ChatMessage {
  return { id, role, createdAt: '1', paragraphs: [role === 'agent' ? '已保存的回答' : '问题'], ...overrides }
}

const completedOutcome = { kind: 'answer', answer: 'completed' } as const

function chatState(overrides: Partial<ChatState> = {}): ChatState {
  return {
    activeId: 'session-a',
    sessions: [{ id: 'session-a', title: '会话 A', updatedAt: '1', messageCount: 0 }],
    messages: [],
    referencedIds: [],
    workingContext: { goal: '', completed: '', nextStep: '', updatedAt: '' },
    projectMemory: [],
    compactedSummary: '',
    compactedCount: 0,
    pendingChoice: null,
    pendingLibrarySearch: null,
    pendingChatNoteEdit: null,
    pendingCommand: null,
    ...overrides,
  }
}

const agentStatus: AgentStatus = {
  baseUrl: 'https://example.invalid', model: 'model', exploreReferences: true, contextTier: 'small',
  readingBudget: { toolCalls: 24, explorationRounds: 12, toolChars: 16000, deepSeconds: 180, deepBatches: 64, requestSeconds: 180, answerChars: 20000, strategy: 'adaptive', effort: 'standard' },
  semanticRetrieval: false, embeddingBaseUrl: '', embeddingModel: '', embeddingHasKey: false,
  hasKey: true, keyPersistent: true, keyWarning: null,
}

type ChatListener = () => void
let chatListener: ChatListener | undefined

async function renderChat(initial = chatState()) {
  vi.mocked(getChatState).mockResolvedValue(initial)
  const openChat = vi.fn()
  const applyNoteEditAndRefresh = vi.fn().mockResolvedValue({})
  const view = renderHook(() => useChat(openChat, applyNoteEditAndRefresh))
  await waitFor(() => expect(view.result.current.activeSessionId).toBe(initial.activeId))
  return { ...view, openChat, applyNoteEditAndRefresh }
}

beforeEach(() => {
  vi.clearAllMocks()
  chatListener = undefined
  vi.mocked(getAgentStatus).mockResolvedValue(agentStatus)
  vi.mocked(getChatState).mockResolvedValue(chatState())
  vi.mocked(setChatReferences).mockImplementation(async id => chatState({ activeId: id }))
  vi.mocked(suggestChatFollowups).mockResolvedValue([])
  listenMock.mockImplementation((event: string, listener: ChatListener) => {
    if (event === 'knowflow:chat-changed') chatListener = listener
    return Promise.resolve(vi.fn())
  })
})

describe('useChat 聊天控制', () => {
  it('流式正文逐步追加，结束后应用已保存会话', async () => {
    const view = await renderChat()
    const pending = deferred<ChatMessage>()
    let onDelta: ((delta: string) => void) | undefined
    let onActivity: ((message: string) => void) | undefined
    vi.mocked(askAgent).mockImplementation((...args) => {
      onDelta = args[2]
      onActivity = args[4]
      return pending.promise
    })
    const reply = chatMessage('reply-1', 'agent', { frame: { imageId: 'image-1', recordId: 'record-1', atSeconds: 0 } })

    let sending!: Promise<boolean>
    act(() => { sending = view.result.current.sendQuestion('总结') })
    act(() => { onDelta?.('第一段'); onActivity?.('读取资料') })
    expect(view.result.current.streaming).toMatchObject({ answer: '第一段', activity: '读取资料', activities: ['读取资料'] })
    act(() => onDelta?.('，第二段'))
    expect(view.result.current.streaming?.answer).toBe('第一段，第二段')

    vi.mocked(getChatState).mockResolvedValue(chatState({ messages: [reply] }))
    await act(async () => { pending.resolve(reply); expect(await sending).toBe(true) })
    expect(view.result.current.streaming).toBeNull()
    expect(view.result.current.messages).toEqual([reply])
    expect(view.result.current.sending).toBe(false)
  })

  it('连续发送只产生一次请求', async () => {
    const view = await renderChat()
    const pending = deferred<ChatMessage>()
    vi.mocked(askAgent).mockReturnValue(pending.promise)
    let first!: Promise<boolean>
    let second!: Promise<boolean>
    act(() => {
      first = view.result.current.sendQuestion('第一问')
      second = view.result.current.sendQuestion('第二问')
    })
    expect(askAgent).toHaveBeenCalledTimes(1)
    await expect(second).resolves.toBe(false)
    const reply = chatMessage('reply-frame', 'agent', { frame: { imageId: 'image-1', recordId: 'record-1', atSeconds: 0 } })
    vi.mocked(getChatState).mockResolvedValue(chatState({ messages: [reply] }))
    await act(async () => { pending.resolve(reply); await first })
  })

  it('失败时保留已有消息并显示错误', async () => {
    const existing = chatMessage('existing')
    const view = await renderChat(chatState({ messages: [existing] }))
    vi.mocked(askAgent).mockRejectedValue(new Error('模拟服务失败'))
    let sent!: boolean
    await act(async () => { sent = await view.result.current.sendQuestion('会失败的问题') })
    expect(sent).toBe(false)
    expect(view.result.current.messages).toEqual([existing])
    expect(view.result.current.agentError).toContain('模拟服务失败')
    expect(view.result.current.streaming).toBeNull()
  })

  it('切换会话后，旧请求完成不能覆盖新会话', async () => {
    const view = await renderChat()
    const pending = deferred<ChatMessage>()
    vi.mocked(askAgent).mockReturnValue(pending.promise)
    let sending!: Promise<boolean>
    act(() => { sending = view.result.current.sendQuestion('旧会话问题') })

    const next = chatState({ activeId: 'session-b', sessions: [{ id: 'session-b', title: '会话 B', updatedAt: '2', messageCount: 0 }] })
    await act(async () => { await view.result.current.manageSession(async () => next) })
    expect(view.result.current.activeSessionId).toBe('session-b')

    const oldReply = chatMessage('old-reply', 'agent', { frame: { imageId: 'image-1', recordId: 'record-1', atSeconds: 0 } })
    vi.mocked(getChatState).mockResolvedValue(chatState({ messages: [oldReply] }))
    await act(async () => { pending.resolve(oldReply); await sending })
    expect(view.result.current.activeSessionId).toBe('session-b')
    expect(view.result.current.messages).toEqual([])
  })

  it('旧建议完成后不能覆盖已切换的新会话', async () => {
    const view = await renderChat()
    const reply = chatMessage('reply-a', 'agent', { outcome: completedOutcome })
    const suggestion = deferred<never[]>()
    vi.mocked(askAgent).mockResolvedValue(reply)
    vi.mocked(suggestChatFollowups).mockReturnValue(suggestion.promise)
    vi.mocked(getChatState).mockResolvedValue(chatState({ messages: [reply] }))
    await act(async () => { await view.result.current.sendQuestion('问题 A') })
    await waitFor(() => expect(suggestChatFollowups).toHaveBeenCalledWith('session-a', 'reply-a'))

    const next = chatState({ activeId: 'session-b', sessions: [{ id: 'session-b', title: '会话 B', updatedAt: '2', messageCount: 0 }] })
    await act(async () => { await view.result.current.manageSession(async () => next) })
    vi.mocked(getChatState).mockResolvedValue(chatState({ messages: [{ ...reply, suggestions: [{ label: '旧建议', prompt: '旧建议' }] }] }))
    await act(async () => { suggestion.resolve([]); await suggestion.promise })
    await waitFor(() => expect(view.result.current.suggestingMessageId).toBeNull())
    expect(view.result.current.activeSessionId).toBe('session-b')
    expect(view.result.current.messages).toEqual([])
  })

  it('引用修改后，较早开始的状态回读不能把引用覆盖回去', async () => {
    const initial = chatState({ referencedIds: ['record-a'] })
    const view = await renderChat(initial)
    vi.mocked(setChatReferences).mockClear()
    const pending = deferred<ChatState>()
    vi.mocked(getChatState).mockReturnValue(pending.promise)
    act(() => chatListener?.())
    act(() => view.result.current.setReferencedIds([]))
    expect(view.result.current.referencedIds).toEqual([])
    await act(async () => { pending.resolve(initial); await pending.promise })
    await waitFor(() => expect(setChatReferences).toHaveBeenCalledWith('session-a', []))
    expect(view.result.current.referencedIds).toEqual([])
  })

  it('初始化只应用后端引用，不触发旧值回写', async () => {
    await renderChat(chatState({ referencedIds: ['record-a'] }))
    await act(async () => { await Promise.resolve() })
    expect(setChatReferences).not.toHaveBeenCalled()
  })

  it('停止阅读只改变阅读状态，保留已流式返回的正文和 requestId', async () => {
    const view = await renderChat()
    const pending = deferred<ChatMessage>()
    let onDelta: ((delta: string) => void) | undefined
    let onReading: ((requestId: string, reading: boolean) => void) | undefined
    vi.mocked(askAgent).mockImplementation((...args) => {
      onDelta = args[2]
      onReading = args[9]
      return pending.promise
    })
    act(() => { void view.result.current.sendQuestion('深读') })
    act(() => { onReading?.('request-7', true); onDelta?.('已经读到的正文') })
    expect(view.result.current.streaming).toMatchObject({ requestId: 'request-7', reading: true, answer: '已经读到的正文' })
    act(() => onReading?.('request-7', false))
    expect(view.result.current.streaming).toMatchObject<Partial<ChatStreaming>>({ requestId: 'request-7', reading: false, answer: '已经读到的正文' })
    const reply = chatMessage('reply-frame', 'agent', { frame: { imageId: 'image-1', recordId: 'record-1', atSeconds: 0 } })
    vi.mocked(getChatState).mockResolvedValue(chatState({ messages: [reply] }))
    await act(async () => { pending.resolve(reply); await pending.promise })
  })

  it('延迟注册的聊天监听器在卸载后仍会取消', async () => {
    const registration = deferred<UnlistenFn>()
    listenMock.mockReturnValue(registration.promise)
    const view = renderHook(() => useChat(vi.fn(), vi.fn()))
    view.unmount()
    const stop = vi.fn()
    await act(async () => { registration.resolve(stop); await registration.promise })
    expect(stop).toHaveBeenCalledTimes(1)
  })

  it('会话切换待决期间修改引用，最终采用新会话的引用', async () => {
    const view = await renderChat(chatState({ referencedIds: ['record-a'] }))
    const action = deferred<ChatState>()
    let managing!: Promise<void>
    act(() => { managing = view.result.current.manageSession(() => action.promise) })
    act(() => view.result.current.setReferencedIds(['record-old-session']))
    const next = chatState({ activeId: 'session-b', sessions: [{ id: 'session-b', title: '会话 B', updatedAt: '2', messageCount: 0 }], referencedIds: ['record-b'] })
    await act(async () => { action.resolve(next); await managing })
    expect(view.result.current.activeSessionId).toBe('session-b')
    expect(view.result.current.referencedIds).toEqual(['record-b'])
  })

  it('旧建议完成不会挤掉新请求的状态回读', async () => {
    const view = await renderChat()
    const replyA = chatMessage('reply-a', 'agent', { outcome: completedOutcome })
    const suggestionA = deferred<never[]>()
    vi.mocked(askAgent).mockResolvedValue(replyA)
    vi.mocked(suggestChatFollowups).mockReturnValue(suggestionA.promise)
    vi.mocked(getChatState).mockResolvedValue(chatState({ messages: [replyA] }))
    await act(async () => { await view.result.current.sendQuestion('问题 A') })
    await waitFor(() => expect(suggestChatFollowups).toHaveBeenCalledWith('session-a', 'reply-a'))

    // 第二次发送：askAgent 与状态回读都受控；新建议立即完成，避免干扰断言。
    vi.mocked(suggestChatFollowups).mockResolvedValue([])
    const replyB = chatMessage('reply-b', 'agent', { outcome: completedOutcome })
    const askB = deferred<ChatMessage>()
    vi.mocked(askAgent).mockReturnValue(askB.promise)
    const readB = deferred<ChatState>()
    vi.mocked(getChatState).mockReturnValue(readB.promise)
    let sendingB!: Promise<boolean>
    act(() => { sendingB = view.result.current.sendQuestion('问题 B') })
    await act(async () => { askB.resolve(replyB); await askB.promise })

    // 旧建议此时完成，不得让 readB 失效。
    await act(async () => { suggestionA.resolve([]); await suggestionA.promise.catch(() => undefined) })
    await act(async () => { readB.resolve(chatState({ messages: [replyA, replyB] })); expect(await sendingB).toBe(true) })
    await waitFor(() => expect(view.result.current.messages.map(message => message.id)).toEqual(['reply-a', 'reply-b']))
  })

  it('两个会话操作乱序完成时，只应用较新的结果', async () => {
    const view = await renderChat()
    const first = deferred<ChatState>()
    const second = deferred<ChatState>()
    let managingFirst!: Promise<void>
    let managingSecond!: Promise<void>
    act(() => { managingFirst = view.result.current.manageSession(() => first.promise) })
    act(() => { managingSecond = view.result.current.manageSession(() => second.promise) })
    const stateB = chatState({ activeId: 'session-b', sessions: [{ id: 'session-b', title: '会话 B', updatedAt: '2', messageCount: 0 }] })
    const stateC = chatState({ activeId: 'session-c', sessions: [{ id: 'session-c', title: '会话 C', updatedAt: '3', messageCount: 0 }] })
    await act(async () => { second.resolve(stateB); await managingSecond })
    expect(view.result.current.activeSessionId).toBe('session-b')
    await act(async () => { first.resolve(stateC); await managingFirst })
    expect(view.result.current.activeSessionId).toBe('session-b')
  })

  it('发送期间到达的会话事件，在发送结束后补读一次', async () => {
    const view = await renderChat()
    const pending = deferred<ChatMessage>()
    vi.mocked(askAgent).mockReturnValue(pending.promise)
    let sending!: Promise<boolean>
    act(() => { sending = view.result.current.sendQuestion('问题') })
    const reply = chatMessage('reply-frame', 'agent', { frame: { imageId: 'image-1', recordId: 'record-1', atSeconds: 0 } })
    const midState = chatState({ messages: [chatMessage('other-window')] })
    vi.mocked(getChatState).mockResolvedValue(midState)
    act(() => chatListener?.())
    await act(async () => { pending.resolve(reply); await sending })
    await waitFor(() => expect(view.result.current.messages.map(message => message.id)).toEqual(['other-window']))
  })

  it('失败回答不生成建议，且修改中文文案不影响判断', async () => {
    const view = await renderChat()
    const failed = chatMessage('reply-failed', 'agent', {
      toolSteps: ['文案已完全改写，不再是旧前缀'],
      outcome: { kind: 'answer', answer: 'failed', answerReason: 'reading_failed', readingReason: 'read_failed' },
    })
    vi.mocked(askAgent).mockResolvedValue(failed)
    vi.mocked(getChatState).mockResolvedValue(chatState({ messages: [failed] }))
    await act(async () => { await view.result.current.sendQuestion('深读失败的问题') })
    expect(suggestChatFollowups).not.toHaveBeenCalled()
    expect(view.result.current.suggestingMessageId).toBeNull()
  })

  it('缺少结构化状态的历史回复保守跳过建议，不自动补发模型请求', async () => {
    const view = await renderChat()
    const legacy = chatMessage('reply-legacy', 'agent', { toolSteps: ['深度阅读失败：旧文案'] })
    vi.mocked(askAgent).mockResolvedValue(legacy)
    vi.mocked(getChatState).mockResolvedValue(chatState({ messages: [legacy] }))
    await act(async () => { await view.result.current.sendQuestion('问题') })
    expect(suggestChatFollowups).not.toHaveBeenCalled()
  })

  it('用户停止阅读后完成的部分回答仍生成建议', async () => {
    const view = await renderChat()
    const partial = chatMessage('reply-partial', 'agent', {
      outcome: { kind: 'answer', answer: 'partial', answerReason: 'stream_interrupted', readingReason: 'user_stopped' },
    })
    vi.mocked(askAgent).mockResolvedValue(partial)
    vi.mocked(getChatState).mockResolvedValue(chatState({ messages: [partial] }))
    await act(async () => { await view.result.current.sendQuestion('停止阅读后回答') })
    await waitFor(() => expect(suggestChatFollowups).toHaveBeenCalledWith('session-a', 'reply-partial'))
  })

  it('选择题消息不误触发后续建议', async () => {
    const view = await renderChat()
    const choice = chatMessage('reply-choice', 'agent', { outcome: { kind: 'choice', answer: 'not_applicable' } })
    vi.mocked(askAgent).mockResolvedValue(choice)
    vi.mocked(getChatState).mockResolvedValue(chatState({ messages: [choice] }))
    await act(async () => { await view.result.current.sendQuestion('需要选择的问题') })
    expect(suggestChatFollowups).not.toHaveBeenCalled()
  })

  it('初始化未完成时到达的小窗引用同步，在会话确认后保留并补写', async () => {
    const init = deferred<ChatState>()
    vi.mocked(getChatState).mockReturnValue(init.promise)
    const view = renderHook(() => useChat(vi.fn(), vi.fn()))
    // 初始化待决、会话 ID 仍为空时注入小窗上下文同步。
    act(() => view.result.current.setReferencedIds(['from-main']))
    expect(view.result.current.referencedIds).toEqual(['from-main'])
    // 后端初始状态返回较旧的引用。
    await act(async () => { init.resolve(chatState({ referencedIds: ['stale-backend'] })); await init.promise })
    await waitFor(() => expect(view.result.current.activeSessionId).toBe('session-a'))
    // 保留较新的本地同步值，不被旧后端覆盖，并补写到后端。
    expect(view.result.current.referencedIds).toEqual(['from-main'])
    await waitFor(() => expect(setChatReferences).toHaveBeenCalledWith('session-a', ['from-main']))
  })

  it('聊天监听注册失败时显示错误且不泄漏', async () => {
    listenMock.mockRejectedValue(new Error('注册失败'))
    const view = await renderChat()
    await waitFor(() => expect(view.result.current.agentError).toContain('对话更新监听失败'))
    view.unmount()
  })
})
