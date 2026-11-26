import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import type { ComponentProps } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { getChatFrameImage, stopAgentReading } from '../agent'
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

  it('显式操作入口随发送传递 intent，可取消，发送成功后清除', async () => {
    const onSend = vi.fn().mockResolvedValue(true)
    render(<ChatPanel {...panelProps({ onSend })} />)
    fireEvent.click(screen.getByRole('button', { name: '改笔记' }))
    expect(screen.getByRole('button', { name: '改笔记' })).toHaveAttribute('aria-pressed', 'true')
    expect(screen.getByText('生成笔记修改建议；确认后才写入。请在问题中写明笔记标题')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '取消操作入口' }))
    expect(screen.getByRole('button', { name: '改笔记' })).toHaveAttribute('aria-pressed', 'false')
    fireEvent.click(screen.getByRole('button', { name: '搜资料库' }))
    fireEvent.change(screen.getByLabelText('向 Agent 提问'), { target: { value: 'Rust 异步教程' } })
    fireEvent.click(screen.getByRole('button', { name: /发送/ }))
    await waitFor(() => expect(onSend).toHaveBeenCalledWith('Rust 异步教程', undefined, [], undefined, false, 'library_search'))
    await waitFor(() => expect(screen.getByRole('button', { name: '搜资料库' })).toHaveAttribute('aria-pressed', 'false'))
  })

  it('消息内截图加入提问或勾选深度模式时自动清除已选 intent', async () => {
    vi.mocked(getChatFrameImage).mockResolvedValue('data:image/jpeg;base64,abc')
    const videoRecord = { id: 'vid-1', type: 'video' as const, title: '测试视频', site: 'example.com', savedAt: '2026-11-25T00:00:00Z', status: 'ready' as const, sourceUrl: 'https://example.com/v', contentText: '', excerpt: [] }
    const frameReply: ChatMessage = { ...reply, id: 'reply-frame', frame: { imageId: 'img-1', recordId: 'vid-1', atSeconds: 80 } }
    render(<ChatPanel {...panelProps({ records: [videoRecord], referencedIds: ['vid-1'], messages: [frameReply] })} />)

    fireEvent.click(screen.getByRole('button', { name: '搜资料库' }))
    expect(screen.getByRole('button', { name: '搜资料库' })).toHaveAttribute('aria-pressed', 'true')
    await waitFor(() => expect(screen.getByRole('button', { name: '加入提问' })).toBeInTheDocument())
    fireEvent.click(screen.getByRole('button', { name: '加入提问' }))
    expect(screen.getByRole('button', { name: '搜资料库' })).toHaveAttribute('aria-pressed', 'false')
    expect(screen.queryByRole('button', { name: '取消操作入口' })).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '移除已选截图' }))
    fireEvent.click(screen.getByRole('button', { name: '改笔记' }))
    expect(screen.getByRole('button', { name: '改笔记' })).toHaveAttribute('aria-pressed', 'true')
    fireEvent.click(screen.getByLabelText(/深度模式/))
    expect(screen.getByRole('button', { name: '改笔记' })).toHaveAttribute('aria-pressed', 'false')
    expect(screen.queryByRole('button', { name: '取消操作入口' })).not.toBeInTheDocument()
  })

  it('选中 intent 后输入截帧指令仍按普通请求发送与展示状态', async () => {
    const onSend = vi.fn().mockResolvedValue(true)
    const view = render(<ChatPanel {...panelProps({ onSend, sending: true, streaming: { question: '截取 01:20 的画面', answer: '', startedAt: 1, activities: [], capture: false } })} />)
    expect(screen.getAllByText('等待回复').length).toBeGreaterThan(0)
    expect(screen.queryByText('正在截取')).not.toBeInTheDocument()

    view.rerender(<ChatPanel {...panelProps({ onSend, sending: false })} />)
    fireEvent.click(screen.getByRole('button', { name: '运行命令' }))
    fireEvent.change(screen.getByLabelText('向 Agent 提问'), { target: { value: '截取 01:20 的画面' } })
    view.rerender(<ChatPanel {...panelProps({ onSend, sending: true })} />)
    expect(screen.getByText('正在回答')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /生成中/ })).toBeInTheDocument()
    expect(screen.queryByText('正在截取')).not.toBeInTheDocument()
    expect(screen.queryByText('截取中')).not.toBeInTheDocument()

    view.rerender(<ChatPanel {...panelProps({ onSend, sending: false })} />)
    fireEvent.click(screen.getByRole('button', { name: /发送/ }))
    await waitFor(() => expect(onSend).toHaveBeenCalledWith('截取 01:20 的画面', undefined, [], undefined, false, 'command'))
  })

  it('截帧发送期间输入框已清空，顶栏与发送按钮仍按流式 capture 显示截取状态', () => {
    render(<ChatPanel {...panelProps({ sending: true, streaming: { question: '截取 01:20 的画面', answer: '', startedAt: 1, activities: [], capture: true } })} />)
    expect(screen.getAllByText('正在截取').length).toBeGreaterThanOrEqual(2)
    expect(screen.getByRole('button', { name: '截取中' })).toBeInTheDocument()
    expect(screen.queryByText('正在回答')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /生成中/ })).not.toBeInTheDocument()
  })

  it('输入 / 唤起斜杠命令菜单，支持键盘选择、过滤、Esc 关闭与互斥隐藏', async () => {
    const onSend = vi.fn().mockResolvedValue(true)
    const videoRecord = { id: 'vid-1', type: 'video' as const, title: '测试视频', site: 'example.com', savedAt: '2026-11-25T00:00:00Z', status: 'ready' as const, sourceUrl: 'https://example.com/v', contentText: '', excerpt: [] }
    render(<ChatPanel {...panelProps({ onSend, records: [videoRecord], referencedIds: ['vid-1'] })} />)

    const input = screen.getByLabelText('向 Agent 提问')
    fireEvent.change(input, { target: { value: '/' } })
    const listbox = screen.getByRole('listbox', { name: '斜杠命令菜单' })
    expect(within(listbox).getAllByRole('option')).toHaveLength(3)

    fireEvent.keyDown(input, { key: 'ArrowDown' })
    fireEvent.keyDown(input, { key: 'Enter' })
    expect(input).toHaveValue('')
    expect(screen.queryByRole('listbox', { name: '斜杠命令菜单' })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '改笔记' })).toHaveAttribute('aria-pressed', 'true')

    fireEvent.change(input, { target: { value: '/cmd' } })
    expect(within(screen.getByRole('listbox', { name: '斜杠命令菜单' })).getAllByRole('option')).toHaveLength(1)
    fireEvent.click(screen.getByRole('option', { name: /运行命令/ }))
    expect(input).toHaveValue('')
    expect(screen.getByRole('button', { name: '运行命令' })).toHaveAttribute('aria-pressed', 'true')

    fireEvent.change(input, { target: { value: '/etc' } })
    expect(screen.queryByRole('listbox', { name: '斜杠命令菜单' })).not.toBeInTheDocument()
    fireEvent.change(input, { target: { value: '/s' } })
    expect(screen.getByRole('listbox', { name: '斜杠命令菜单' })).toBeInTheDocument()
    fireEvent.keyDown(input, { key: 'Escape' })
    expect(screen.queryByRole('listbox', { name: '斜杠命令菜单' })).not.toBeInTheDocument()
    expect(input).toHaveValue('/s')

    fireEvent.change(input, { target: { value: '' } })
    fireEvent.click(screen.getByLabelText(/深度模式/))
    fireEvent.change(input, { target: { value: '/' } })
    expect(screen.queryByRole('listbox', { name: '斜杠命令菜单' })).not.toBeInTheDocument()
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

  it('有草稿时采用建议先弹应用内确认，取消保留原草稿，确认才替换', async () => {
    const message: ChatMessage = { ...reply, suggestions: [{ label: '继续深入', prompt: '请继续深入分析' }] }
    render(<ChatPanel {...panelProps({ messages: [message] })} />)
    const input = screen.getByLabelText('向 Agent 提问')
    fireEvent.change(input, { target: { value: '我的原有草稿' } })
    fireEvent.click(screen.getByRole('button', { name: '继续深入 ↗' }))

    const dialog = await screen.findByRole('dialog')
    expect(dialog).toHaveAttribute('aria-modal', 'true')
    expect(within(dialog).getByText('用这条建议替换当前输入内容？')).toBeInTheDocument()
    fireEvent.click(within(dialog).getByRole('button', { name: '取消' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(input).toHaveValue('我的原有草稿')

    fireEvent.click(screen.getByRole('button', { name: '继续深入 ↗' }))
    const again = await screen.findByRole('dialog')
    fireEvent.click(within(again).getByRole('button', { name: '替换' }))
    await waitFor(() => expect(input).toHaveValue('请继续深入分析'))
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  })

  it('草稿为空或与建议相同时采用建议不弹确认', async () => {
    const message: ChatMessage = { ...reply, suggestions: [{ label: '继续深入', prompt: '请继续深入分析' }] }
    render(<ChatPanel {...panelProps({ messages: [message] })} />)
    const input = screen.getByLabelText('向 Agent 提问')
    fireEvent.click(screen.getByRole('button', { name: '继续深入 ↗' }))
    await waitFor(() => expect(input).toHaveValue('请继续深入分析'))
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '继续深入 ↗' }))
    await waitFor(() => expect(input).toHaveValue('请继续深入分析'))
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  })
})
