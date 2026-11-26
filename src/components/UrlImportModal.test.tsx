import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import type { KnowledgeRecord } from '../data'
import type { CaptureResult } from '../records'
import { UrlImportModal } from './UrlImportModal'

vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => true, invoke: vi.fn() }))

const record: KnowledgeRecord = {
  id: 'rec-url-1', type: 'video', title: '直采视频', site: 'bilibili.com', savedAt: '2026-11-26T00:00:00Z',
  status: 'ready', sourceUrl: 'https://www.bilibili.com/video/BV1xx', contentText: '', excerpt: [],
}
const result: CaptureResult = { record, alreadySaved: false, updated: false }

beforeEach(() => { vi.clearAllMocks() })

describe('UrlImportModal', () => {
  it('粘贴分享文案提交后调用 capture_from_url，cookies 留空传 null，成功回调 onImported 并关闭', async () => {
    vi.mocked(invoke).mockResolvedValue(result)
    const onImported = vi.fn()
    const onClose = vi.fn()
    render(<UrlImportModal onClose={onClose} onImported={onImported} />)
    expect(screen.getByRole('dialog')).toHaveAttribute('aria-modal', 'true')
    expect(screen.getByText(/支持 B 站视频 \/ b23.tv 短链 \/ YouTube \/ 常见网页文章/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '抓取并导入' })).toBeDisabled()

    fireEvent.change(screen.getByLabelText('链接或分享文案'), { target: { value: '【标题】 https://b23.tv/abc123 ' } })
    fireEvent.click(screen.getByRole('button', { name: '抓取并导入' }))
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('capture_from_url', { url: '【标题】 https://b23.tv/abc123', cookiesText: null }))
    await waitFor(() => expect(onImported).toHaveBeenCalledWith(result))
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('抓取中显示加载态并禁用 Esc 与遮罩关闭，完成后才回调', async () => {
    let resolveCapture!: (value: CaptureResult) => void
    vi.mocked(invoke).mockImplementation(() => new Promise<CaptureResult>(resolve => { resolveCapture = resolve }))
    const onImported = vi.fn()
    const onClose = vi.fn()
    render(<UrlImportModal onClose={onClose} onImported={onImported} />)
    fireEvent.change(screen.getByLabelText('链接或分享文案'), { target: { value: 'https://example.com/post' } })
    fireEvent.click(screen.getByRole('button', { name: '抓取并导入' }))
    expect(screen.getByRole('button', { name: '正在抓取…' })).toBeDisabled()
    expect(screen.getByLabelText('关闭导入窗口')).toBeDisabled()

    fireEvent.keyDown(window, { key: 'Escape' })
    fireEvent.click(document.querySelector('.url-import-backdrop') as HTMLElement)
    expect(onClose).not.toHaveBeenCalled()

    await act(async () => { resolveCapture(result) })
    await waitFor(() => expect(onImported).toHaveBeenCalledWith(result))
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('后端报需要登录时显示真实错误并自动展开 cookies 输入框，粘贴后重试携带内容', async () => {
    vi.mocked(invoke).mockRejectedValueOnce('该视频需要登录后才能读取字幕，请粘贴仅含 B 站登录信息的 Netscape cookies.txt 后重试。')
    const onImported = vi.fn()
    render(<UrlImportModal onClose={vi.fn()} onImported={onImported} />)
    fireEvent.change(screen.getByLabelText('链接或分享文案'), { target: { value: 'https://www.bilibili.com/video/BV1xx' } })
    fireEvent.click(screen.getByRole('button', { name: '抓取并导入' }))

    expect(await screen.findByRole('alert')).toHaveTextContent('该视频需要登录后才能读取字幕')
    expect(screen.getByRole('button', { name: /可选：B 站 Netscape cookies.txt/ })).toHaveAttribute('aria-expanded', 'true')
    const cookiesInput = screen.getByLabelText(/粘贴 cookies.txt 内容/)
    expect(screen.getByText('仅在内存中临时用于本次取流，不写入磁盘。')).toBeInTheDocument()
    expect(onImported).not.toHaveBeenCalled()

    vi.mocked(invoke).mockResolvedValueOnce(result)
    fireEvent.change(cookiesInput, { target: { value: '# Netscape HTTP Cookie File\n.bilibili.com\tTRUE\t/\tTRUE\t0\tSESSDATA\tabc' } })
    fireEvent.click(screen.getByRole('button', { name: '抓取并导入' }))
    await waitFor(() => expect(invoke).toHaveBeenLastCalledWith('capture_from_url', {
      url: 'https://www.bilibili.com/video/BV1xx',
      cookiesText: '# Netscape HTTP Cookie File\n.bilibili.com\tTRUE\t/\tTRUE\t0\tSESSDATA\tabc',
    }))
    await waitFor(() => expect(onImported).toHaveBeenCalledWith(result))
  })

  it('与登录无关的错误不自动展开 cookies 区，可手动展开收起', async () => {
    vi.mocked(invoke).mockRejectedValueOnce('没有找到 http:// 或 https:// 链接')
    render(<UrlImportModal onClose={vi.fn()} onImported={vi.fn()} />)
    fireEvent.change(screen.getByLabelText('链接或分享文案'), { target: { value: '不是链接' } })
    fireEvent.click(screen.getByRole('button', { name: '抓取并导入' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('没有找到 http:// 或 https:// 链接')
    expect(screen.queryByLabelText(/粘贴 cookies.txt 内容/)).not.toBeInTheDocument()

    const toggle = screen.getByRole('button', { name: /可选：B 站 Netscape cookies.txt/ })
    fireEvent.click(toggle)
    expect(screen.getByLabelText(/粘贴 cookies.txt 内容/)).toBeInTheDocument()
    fireEvent.click(toggle)
    expect(screen.queryByLabelText(/粘贴 cookies.txt 内容/)).not.toBeInTheDocument()
  })

  it('空闲时 Esc、取消按钮与遮罩点击都会关闭弹窗', () => {
    const onClose = vi.fn()
    const view = render(<UrlImportModal onClose={onClose} onImported={vi.fn()} />)
    fireEvent.keyDown(window, { key: 'Escape' })
    expect(onClose).toHaveBeenCalledTimes(1)

    fireEvent.click(screen.getByRole('button', { name: '取消' }))
    expect(onClose).toHaveBeenCalledTimes(2)

    fireEvent.click(document.querySelector('.url-import-backdrop') as HTMLElement)
    expect(onClose).toHaveBeenCalledTimes(3)

    fireEvent.click(screen.getByRole('dialog'))
    expect(onClose).toHaveBeenCalledTimes(3)
    view.unmount()
  })
})