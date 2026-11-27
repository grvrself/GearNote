import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import type { ChatSessionSummary } from '../agent'
import { applyTheme } from '../theme'
import { Sidebar } from './Sidebar'

beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: vi.fn(() => ({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn(), addListener: vi.fn(), removeListener: vi.fn() })),
  })
})

beforeEach(() => {
  applyTheme('mono')
})

const sessions: ChatSessionSummary[] = [
  { id: 's-1', title: '默认会话', updatedAt: '2026-11-26T00:00:00Z', messageCount: 2 },
]

describe('Sidebar 删除会话确认', () => {
  it('会话项内删除按钮弹出应用内确认框，取消不删除，确认才执行', async () => {
    const onDeleteSession = vi.fn()
    render(<Sidebar view="chat" sessions={sessions} activeSessionId="s-1" sessionBusy={false} sessionError=""
      onViewChange={vi.fn()} onCreateSession={vi.fn()} onSwitchSession={vi.fn()} onDeleteSession={onDeleteSession} />)
    expect(screen.queryByRole('button', { name: '删除' })).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '删除会话 默认会话' }))

    const dialog = await screen.findByRole('dialog')
    expect(dialog).toHaveAttribute('aria-modal', 'true')
    expect(within(dialog).getByText('删除会话“默认会话”？此操作会删除该会话的对话记录。')).toBeInTheDocument()
    fireEvent.click(within(dialog).getByRole('button', { name: '取消' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(onDeleteSession).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: '删除会话 默认会话' }))
    const again = await screen.findByRole('dialog')
    fireEvent.click(within(again).getByRole('button', { name: '删除' }))
    await waitFor(() => expect(onDeleteSession).toHaveBeenCalledWith('s-1'))
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  })

  it('mono 主题下在侧边栏底部渲染暗黑繁花暗纹，切换到 hud 主题时立即卸载', () => {
    render(<Sidebar view="chat" sessions={sessions} activeSessionId="s-1" sessionBusy={false} sessionError=""
      onViewChange={vi.fn()} onCreateSession={vi.fn()} onSwitchSession={vi.fn()} onDeleteSession={vi.fn()} />)

    const floral = document.querySelector('.sidebar-floral-backdrop')
    expect(floral).toBeInTheDocument()
    expect(floral).toHaveAttribute('aria-hidden', 'true')
    const img = floral?.querySelector('img')
    expect(img).toBeInTheDocument()
    expect(img?.getAttribute('src')).toBeTruthy()

    act(() => { applyTheme('hud') })
    expect(document.querySelector('.sidebar-floral-backdrop')).not.toBeInTheDocument()
  })
})