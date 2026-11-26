import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import type { ComponentProps } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { Note } from '../notes'
import { NotesPanel } from './NotesPanel'

const note: Note = { id: 'note-1', title: '测试笔记', body: '正文内容', sources: [], folderId: null, createdAt: 1, updatedAt: 1 }

function panelProps(overrides: Partial<ComponentProps<typeof NotesPanel>> = {}): ComponentProps<typeof NotesPanel> {
  return {
    notes: [note], folders: [], records: [], incomingDraft: null,
    onDraftConsumed: vi.fn(), onSave: vi.fn(), onDelete: vi.fn().mockResolvedValue(undefined), onExport: vi.fn(),
    onRevealRecord: vi.fn(), onApplyEdit: vi.fn(), onUndoEdit: vi.fn(), onRejectEdit: vi.fn(),
    onCreateFolder: vi.fn(), onRenameFolder: vi.fn(), onDeleteFolder: vi.fn(), ...overrides,
  }
}

async function editFirstNote() {
  fireEvent.click(await screen.findByRole('button', { name: '编辑笔记' }))
  fireEvent.change(screen.getByLabelText('正文'), { target: { value: '修改后的正文' } })
}

describe('NotesPanel 应用内确认', () => {
  it('未保存修改时新建笔记先弹确认，取消保留草稿，确认才清空', async () => {
    render(<NotesPanel {...panelProps()} />)
    await editFirstNote()
    fireEvent.click(screen.getByRole('button', { name: '＋ 新建笔记' }))

    const dialog = await screen.findByRole('dialog')
    expect(within(dialog).getByText('当前笔记有未保存的内容，确定放弃并切换吗？')).toBeInTheDocument()
    expect(screen.getByLabelText('正文')).toHaveValue('修改后的正文')
    fireEvent.click(within(dialog).getByRole('button', { name: '取消' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(screen.getByLabelText('正文')).toHaveValue('修改后的正文')

    fireEvent.click(screen.getByRole('button', { name: '＋ 新建笔记' }))
    const again = await screen.findByRole('dialog')
    fireEvent.click(within(again).getByRole('button', { name: '放弃并切换' }))
    await waitFor(() => expect(screen.getByLabelText('正文')).toHaveValue(''))
    expect(screen.getByLabelText('标题')).toHaveValue('')
  })

  it('删除笔记走应用内确认，取消不删除，确认才执行', async () => {
    const props = panelProps()
    render(<NotesPanel {...props} />)
    fireEvent.click(await screen.findByRole('button', { name: '删除笔记' }))
    const dialog = await screen.findByRole('dialog')
    expect(within(dialog).getByText('确定删除笔记《测试笔记》？这会同时删除它的高级版和笔记截图，且无法撤销。')).toBeInTheDocument()
    fireEvent.click(within(dialog).getByRole('button', { name: '取消' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(props.onDelete).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: '删除笔记' }))
    const again = await screen.findByRole('dialog')
    fireEvent.click(within(again).getByRole('button', { name: '删除' }))
    await waitFor(() => expect(props.onDelete).toHaveBeenCalledWith('note-1'))
  })

  it('外部打开高级工作台先弹放弃确认，取消回退且不丢草稿，确认后才进入', async () => {
    const onDesignOpenChange = vi.fn()
    const props = panelProps({ designOpen: false, onDesignOpenChange })
    const view = render(<NotesPanel {...props} />)
    await editFirstNote()

    view.rerender(<NotesPanel {...props} designOpen />)
    const dialog = await screen.findByRole('dialog')
    expect(screen.queryByRole('button', { name: '返回资料库' })).not.toBeInTheDocument()
    expect(screen.getByLabelText('正文')).toHaveValue('修改后的正文')
    fireEvent.click(within(dialog).getByRole('button', { name: '取消' }))
    await waitFor(() => expect(onDesignOpenChange).toHaveBeenCalledWith(false))
    expect(screen.getByLabelText('正文')).toHaveValue('修改后的正文')

    view.rerender(<NotesPanel {...props} designOpen={false} />)
    view.rerender(<NotesPanel {...props} designOpen />)
    const again = await screen.findByRole('dialog')
    fireEvent.click(within(again).getByRole('button', { name: '放弃并切换' }))
    expect(await screen.findByRole('button', { name: '返回资料库' })).toBeInTheDocument()
  })

  it('外部 composeRequest 先弹放弃确认，取消保留草稿，确认后才打开生成面板', async () => {
    const onComposeRequestConsumed = vi.fn()
    const props = panelProps({ composeRequest: null, onComposeRequestConsumed })
    const view = render(<NotesPanel {...props} />)
    await editFirstNote()

    view.rerender(<NotesPanel {...props} composeRequest={{ ids: ['rec-1'], nonce: 1 }} />)
    const dialog = await screen.findByRole('dialog')
    fireEvent.click(within(dialog).getByRole('button', { name: '取消' }))
    await waitFor(() => expect(onComposeRequestConsumed).toHaveBeenCalledTimes(1))
    expect(screen.getByLabelText('正文')).toHaveValue('修改后的正文')
    expect(screen.queryByText('选择资料生成草稿')).not.toBeInTheDocument()

    view.rerender(<NotesPanel {...props} composeRequest={null} />)
    view.rerender(<NotesPanel {...props} composeRequest={{ ids: ['rec-1'], nonce: 2 }} />)
    const again = await screen.findByRole('dialog')
    fireEvent.click(within(again).getByRole('button', { name: '放弃并切换' }))
    expect(await screen.findByText('选择资料生成草稿')).toBeInTheDocument()
    expect(onComposeRequestConsumed).toHaveBeenCalledTimes(2)
  })
})