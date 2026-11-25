import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { applyNoteEdit } from './notes'
import { useChat } from './hooks/useChat'
import MiniApp from './MiniApp'

let latestOnSend: ((question: string) => Promise<boolean>) | undefined

vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => false }))
vi.mock('./hooks/useChat', () => ({ useChat: vi.fn() }))
vi.mock('./notes', async importOriginal => ({ ...(await importOriginal<typeof import('./notes')>()), applyNoteEdit: vi.fn() }))
vi.mock('./components/ChatPanel', () => ({
  ChatPanel: ({ onSend }: { onSend: (question: string) => Promise<boolean> }) => {
    latestOnSend = onSend
    return <button type="button" onClick={() => { void onSend('来自小窗') }}>小窗发送</button>
  },
}))

beforeEach(() => {
  vi.clearAllMocks()
  latestOnSend = undefined
  const sendQuestion = vi.fn().mockResolvedValue(true)
  vi.mocked(useChat).mockReturnValue({
    referencedIds: [], setReferencedIds: vi.fn(), agentStatus: { hasKey: true, model: 'model' }, messages: [], activeSessionId: 'session-a',
    sending: false, streaming: null, suggestingMessageId: null, suggestionErrorId: null,
    pendingChoice: null, pendingLibrarySearch: null, pendingChatNoteEdit: null, pendingCommand: null,
    agentError: '', clearAgentError: vi.fn(), sendQuestion, decideLibrarySearch: vi.fn(), decideChatNoteEdit: vi.fn(), decideCommand: vi.fn(),
  } as unknown as ReturnType<typeof useChat>)
})

describe('MiniApp 聊天适配', () => {
  it('小窗发送转发到公共控制器的 sendQuestion', () => {
    render(<MiniApp />)
    const controller = vi.mocked(useChat).mock.results[0].value as ReturnType<typeof useChat>
    fireEvent.click(screen.getByRole('button', { name: '小窗发送' }))
    expect(latestOnSend).toBeDefined()
    expect(controller.sendQuestion).toHaveBeenCalledWith('来自小窗', undefined, undefined, undefined, undefined, undefined, undefined, undefined)
    expect(useChat).toHaveBeenCalledWith(expect.any(Function), applyNoteEdit)
  })
})
