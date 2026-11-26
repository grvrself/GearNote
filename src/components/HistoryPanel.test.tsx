import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { KnowledgeRecord } from '../data'
import { getRecord } from '../records'
import { HistoryPanel } from './HistoryPanel'

vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => false, invoke: vi.fn() }))
vi.mock('../records', async importOriginal => ({
  ...(await importOriginal<typeof import('../records')>()),
  getRecord: vi.fn(),
}))

const article: KnowledgeRecord = {
  id: 'art-1', type: 'article', title: '测试文章', site: 'example.com', savedAt: '2026-11-26T00:00:00Z',
  status: 'ready', sourceUrl: 'https://example.com/post', contentText: '正文', excerpt: ['正文'],
}

function panelProps(overrides: Partial<Parameters<typeof HistoryPanel>[0]> = {}) {
  return {
    records: [article], selectedId: article.id, referencedIds: [],
    onSelect: vi.fn(), onReference: vi.fn(), onRemoveReference: vi.fn(), onCollapse: vi.fn(),
    miniAvailable: false, onOpenMini: vi.fn(), onRecordsChanged: vi.fn(), onAskFrame: vi.fn(),
    onDeleteRecord: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  }
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(getRecord).mockResolvedValue(article)
})

describe('HistoryPanel 删除资料确认', () => {
  it('删除资料弹出应用内确认框，取消不删除，确认才执行', async () => {
    const props = panelProps()
    render(<HistoryPanel {...props} />)
    fireEvent.click(await screen.findByRole('button', { name: '删除资料' }))

    const dialog = await screen.findByRole('dialog')
    expect(dialog).toHaveAttribute('aria-modal', 'true')
    expect(within(dialog).getByText('确定删除《测试文章》吗？资料正文和字幕将从资料库移除，已有笔记或对话中的来源链接可能失效。')).toBeInTheDocument()
    fireEvent.click(within(dialog).getByRole('button', { name: '取消' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(props.onDeleteRecord).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: '删除资料' }))
    const again = await screen.findByRole('dialog')
    fireEvent.click(within(again).getByRole('button', { name: '删除' }))
    await waitFor(() => expect(props.onDeleteRecord).toHaveBeenCalledWith(article))
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  })

  it('onOpenUrlImport 传入时资料工具区渲染导入入口并可点击', () => {
    const onOpenUrlImport = vi.fn()
    const view = render(<HistoryPanel {...panelProps()} />)
    expect(screen.queryByRole('button', { name: '＋ 导入链接' })).not.toBeInTheDocument()
    view.rerender(<HistoryPanel {...panelProps({ onOpenUrlImport })} />)
    fireEvent.click(screen.getByRole('button', { name: '＋ 导入链接' }))
    expect(onOpenUrlImport).toHaveBeenCalledTimes(1)
  })
})