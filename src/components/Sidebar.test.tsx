import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { ChatSessionSummary } from '../agent'
import { Sidebar } from './Sidebar'

const sessions: ChatSessionSummary[] = [
  { id: 's-1', title: '默认会话', updatedAt: '2026-11-26T00:00:00Z', messageCount: 2 },
]

describe('Sidebar 删除会话确认', () => {
  it('删除会话弹出应用内确认框，取消不删除，确认才执行', async () => {
    const onDeleteSession = vi.fn()
    render(<Sidebar view="chat" sessions={sessions} activeSessionId="s-1" sessionBusy={false} sessionError=""
      onViewChange={vi.fn()} onCreateSession={vi.fn()} onSwitchSession={vi.fn()} onDeleteSession={onDeleteSession} />)
    fireEvent.click(screen.getByRole('button', { name: '删除' }))

    const dialog = await screen.findByRole('dialog')
    expect(dialog).toHaveAttribute('aria-modal', 'true')
    expect(within(dialog).getByText('删除会话“默认会话”？此操作会删除该会话的对话记录。')).toBeInTheDocument()
    fireEvent.click(within(dialog).getByRole('button', { name: '取消' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(onDeleteSession).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: '删除' }))
    const again = await screen.findByRole('dialog')
    fireEvent.click(within(again).getByRole('button', { name: '删除' }))
    await waitFor(() => expect(onDeleteSession).toHaveBeenCalledWith('s-1'))
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  })
})