import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import type { ComponentProps } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { stopAgentReading } from '../agent'
import type { ChatMessage } from '../chat-data'
import { ChatPanel } from './ChatPanel'

vi.mock('../agent', async importOriginal => ({
  ...(await importOriginal<typeof import('../agent')>()),
  stopAgentReading: vi.fn(),
  getChatFrameImage: vi.fn(),
}))

const reply: ChatMessage = { id: 'reply-1', role: 'agent', createdAt: '1', paragraphs: ['已保存回答'] }

function panelProps(overrides: Partial<ComponentProps<typeof ChatPanel>> = {}): ComponentProps<typeof ChatPanel> {
  return {
    records: [], referencedIds: [], historyOpen: true, agentAvailable: true, messages: [reply],
    onRemoveReference: vi.fn(), onRevealRecord: vi.fn(), onToggleHistory: vi.fn(), onShowHistory: vi.fn(),
    onSend: vi.fn().mockResolvedValue(true), ...overrides,
  }
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(stopAgentReading).mockResolvedValue(undefined)
})

describe('ChatPanel 关键交互', () => {
  it('发送失败后恢复当前会话的输入内容', async () => {
    const onSend = vi.fn().mockResolvedValue(false)
    render(<ChatPanel {...panelProps({ onSend })} />)
    const input = screen.getByLabelText('向 Agent 提问')
    fireEvent.change(input, { target: { value: '发送失败后保留这句话' } })
    fireEvent.click(screen.getByRole('button', { name: /发送/ }))
    await waitFor(() => expect(onSend).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(input).toHaveValue('发送失败后保留这句话'))
    expect(screen.getByText('已保存回答')).toBeInTheDocument()
  })

  it('停止阅读传递当前 requestId，进入回答阶段后保留已有正文', async () => {
    const props = panelProps({ streaming: { question: '深读', answer: '部分正文', startedAt: 1, activities: [], requestId: 'request-7', reading: true } })
    const view = render(<ChatPanel {...props} />)
    fireEvent.click(screen.getByRole('button', { name: '停止阅读并回答' }))
    await waitFor(() => expect(stopAgentReading).toHaveBeenCalledWith('request-7'))
    view.rerender(<ChatPanel {...props} streaming={{ ...props.streaming!, reading: false }} />)
    expect(screen.queryByRole('button', { name: '停止阅读并回答' })).not.toBeInTheDocument()
    expect(screen.getAllByText('部分正文').length).toBeGreaterThan(0)
  })

  it('选择题、资料搜索和写入提案仍显示在对应消息下，未确认前不发送', async () => {
    const onSend = vi.fn().mockResolvedValue(true)
    const onResolveLibrarySearch = vi.fn().mockResolvedValue(undefined)
    render(<ChatPanel {...panelProps({
      onSend, onResolveLibrarySearch,
      pendingChoice: { id: 'choice-1', messageId: reply.id, prompt: '选择下一步', why: '路径不同', options: [{ id: 'a', label: '方案 A', detail: '' }], recommended: null, status: 'open', selectedId: null, customText: null, answeredAt: null },
      pendingLibrarySearch: { id: 'search-1', messageId: reply.id, question: '原问题', records: [{ id: 'record-1', title: '资料一' }], status: 'open' },
      pendingChatNoteEdit: { messageId: reply.id, noteTitle: '笔记一', proposal: { id: 'note-1', noteId: 'saved-note', mode: 'append', beforeText: '', afterText: '追加', reason: '补充', edits: [{ mode: 'append', beforeText: '', afterText: '追加', reason: '补充' }], status: 'pending', createdAt: '1' } },
      pendingCommand: { id: 'command-1', messageId: reply.id, program: 'ffmpeg', args: ['-version'], effectiveArgs: ['-version'], status: 'open' },
    })} />)
    expect(within(screen.getByRole('group', { name: '选择下一步' })).getByText('方案 A')).toBeInTheDocument()
    expect(screen.getByRole('group', { name: '资料库搜索结果' })).toBeInTheDocument()
    expect(screen.getByRole('group', { name: '待确认的笔记修改' })).toBeInTheDocument()
    expect(screen.getByRole('group', { name: '待确认的命令' })).toBeInTheDocument()
    expect(onSend).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: '确认并继续提问' }))
    await waitFor(() => expect(onResolveLibrarySearch).toHaveBeenCalledWith('search-1', true, '原问题'))
    expect(onSend).not.toHaveBeenCalled()
  })

  it('续读按钮只依据后端续读状态显示，不再依赖中文前缀', async () => {
    const onContinueReading = vi.fn().mockResolvedValue(true)
    const message: ChatMessage = { ...reply, toolSteps: ['可继续读取：旧文案'] }
    const view = render(<ChatPanel {...panelProps({ messages: [message], onContinueReading })} />)
    expect(screen.queryByRole('button', { name: '继续读取' })).not.toBeInTheDocument()
    view.rerender(<ChatPanel {...panelProps({ messages: [message], onContinueReading, readContinuation: { messageId: reply.id, mode: 'adaptive', hasProgress: true } })} />)
    fireEvent.click(screen.getByRole('button', { name: '继续读取' }))
    await waitFor(() => expect(onContinueReading).toHaveBeenCalledWith(reply.id))
  })

  it('hasProgress 为 false 时即使消息匹配也隐藏续读按钮', () => {
    const message: ChatMessage = { ...reply }
    render(<ChatPanel {...panelProps({ messages: [message], onContinueReading: vi.fn(), readContinuation: { messageId: reply.id, mode: 'adaptive', hasProgress: false } })} />)
    expect(screen.queryByRole('button', { name: '继续读取' })).not.toBeInTheDocument()
  })

  it('结构化提醒按 code 渲染，修改展示文案不影响显示', () => {
    const message: ChatMessage = { ...reply, toolSteps: ['随便改写的步骤文案'], outcome: {
      kind: 'answer', answer: 'partial',
      notices: [{ code: 'citation_unverified', message: '引用提醒：新文案' }, { code: 'answer_warning', message: '部分回答已保留（新文案）' }],
    } }
    render(<ChatPanel {...panelProps({ messages: [message] })} />)
    expect(screen.getByText('引用提醒：新文案')).toBeInTheDocument()
    expect(screen.getByText('部分回答已保留（新文案）')).toBeInTheDocument()
    // toolSteps 仍作为执行记录展示，但不再被当作提醒（role=note）渲染。
    expect(screen.queryByRole('note', { name: '随便改写的步骤文案' })).not.toBeInTheDocument()
    expect(screen.getAllByRole('note')).toHaveLength(2)
  })
})
