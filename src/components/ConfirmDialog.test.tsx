import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { useConfirm } from '../hooks/useConfirm'
import type { ConfirmOptions } from './ConfirmDialog'

function Harness({ onResult, options }: { onResult: (value: boolean) => void; options?: ConfirmOptions }) {
  const { confirm, confirmDialog } = useConfirm()
  return <div>
    <button type="button" onClick={() => { void confirm(options ?? { title: '删除会话', message: '确定删除吗？', confirmLabel: '删除', danger: true }).then(onResult) }}>发起确认</button>
    {confirmDialog}
  </div>
}

describe('useConfirm + ConfirmDialog', () => {
  it('打开带 aria-modal 的弹窗，确认按钮 resolve true 并关闭', async () => {
    const onResult = vi.fn()
    render(<Harness onResult={onResult} />)
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '发起确认' }))
    const dialog = await screen.findByRole('dialog')
    expect(dialog).toHaveAttribute('aria-modal', 'true')
    expect(screen.getByText('删除会话')).toBeInTheDocument()
    expect(screen.getByText('确定删除吗？')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '删除' }))
    await waitFor(() => expect(onResult).toHaveBeenCalledWith(true))
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  })

  it('取消按钮与 Esc 都 resolve false，点击弹窗本体不关闭', async () => {
    const onResult = vi.fn()
    render(<Harness onResult={onResult} />)
    fireEvent.click(screen.getByRole('button', { name: '发起确认' }))
    const dialog = await screen.findByRole('dialog')
    fireEvent.click(dialog)
    expect(onResult).not.toHaveBeenCalled()
    expect(screen.getByRole('dialog')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '取消' }))
    await waitFor(() => expect(onResult).toHaveBeenCalledWith(false))
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '发起确认' }))
    await screen.findByRole('dialog')
    fireEvent.keyDown(window, { key: 'Escape' })
    await waitFor(() => expect(onResult).toHaveBeenCalledTimes(2))
    expect(onResult).toHaveBeenLastCalledWith(false)
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  })

  it('点击遮罩 resolve false', async () => {
    const onResult = vi.fn()
    render(<Harness onResult={onResult} />)
    fireEvent.click(screen.getByRole('button', { name: '发起确认' }))
    await screen.findByRole('dialog')
    fireEvent.click(document.querySelector('.confirm-dialog-backdrop') as HTMLElement)
    await waitFor(() => expect(onResult).toHaveBeenCalledWith(false))
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  })

  it('卸载时把未决 promise resolve 为 false，不泄漏', async () => {
    const onResult = vi.fn()
    const view = render(<Harness onResult={onResult} />)
    fireEvent.click(screen.getByRole('button', { name: '发起确认' }))
    await screen.findByRole('dialog')
    view.unmount()
    await waitFor(() => expect(onResult).toHaveBeenCalledWith(false))
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  })

  it('默认文案与自定义标签生效，新的确认会把旧未决确认按取消处理', async () => {
    const onResult = vi.fn()
    render(<Harness onResult={onResult} options={{ message: '第二次确认' }} />)
    fireEvent.click(screen.getByRole('button', { name: '发起确认' }))
    await screen.findByRole('dialog')
    expect(screen.getByRole('button', { name: '确认' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '取消' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '发起确认' }))
    await waitFor(() => expect(onResult).toHaveBeenCalledWith(false))
    expect(screen.getByText('第二次确认')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '确认' }))
    await waitFor(() => expect(onResult).toHaveBeenLastCalledWith(true))
  })
})