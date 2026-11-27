import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { Note } from '../notes'
import { AdvancedNoteStudio } from './AdvancedNoteStudio'

const previewNoteDesign = vi.hoisted(() => vi.fn(() => Promise.resolve({})))

vi.mock('../note-design', () => ({ previewNoteDesign }))
vi.mock('./AdvancedNotePanel', () => ({ AdvancedNotePanel: () => <div>advanced-editor-stub</div> }))

const note = {
  id: 'n1', title: '测试笔记', body: '# 标题\n正文内容', sources: [],
  createdAt: 1764000000000, updatedAt: 1764000000000,
} as Note

function renderStudio() {
  return render(<AdvancedNoteStudio notes={[note]} records={[]} initialNoteId="n1"
    onSelectNote={vi.fn()} onClose={vi.fn()} onCreateFromSources={vi.fn()} onRevealRecord={vi.fn()}
    onApplyEdit={vi.fn()} onRejectEdit={vi.fn()} onUndoEdit={vi.fn()} onNoteChanged={vi.fn()} />)
}

describe('AdvancedNoteStudio 高级笔记工作台', () => {
  it('中央舞台包含 HeroDialStage 表盘与四角准星，悬停主按钮联动高光', () => {
    const { container } = renderStudio()
    expect(screen.getByText('READING EDITION / MONO')).toBeInTheDocument()
    const stage = container.querySelector('.hero-dial-stage')
    expect(stage).not.toBeNull()
    expect(stage).toHaveAttribute('data-highlighted', 'false')
    expect(container.querySelector('.hero-dial-stage svg')).not.toBeNull()
    expect(container.querySelectorAll('.hero-dial-gear').length).toBe(3)
    expect(container.querySelector('.gn-crosshair-frame')).not.toBeNull()

    const primaryBtn = screen.getByRole('button', { name: '生成高级版 →' })
    expect(primaryBtn.className).toContain('gn-btn-primary')
    expect(screen.getByRole('button', { name: '打开原笔记' }).className).toContain('gn-btn-secondary')
    expect(screen.getByRole('button', { name: '打开笔记' }).className).toContain('gn-btn-secondary')
    expect(screen.getByText('GENERATION DOES NOT MODIFY THE ORIGINAL NOTE')).toBeInTheDocument()

    fireEvent.mouseEnter(primaryBtn)
    expect(stage).toHaveAttribute('data-highlighted', 'true')
    fireEvent.mouseLeave(primaryBtn)
    expect(stage).toHaveAttribute('data-highlighted', 'false')
  })

  it('同时挂载 HUD 方案 B 游标刻度轨与单体黄齿轮，仅保留单组文案按钮并联动悬停微转', () => {
    const { container } = renderStudio()
    const hudStage = container.querySelector('.studio-hud-stage')
    const rail = container.querySelector('.studio-hud-vernier-rail')
    expect(hudStage).not.toBeNull()
    expect(rail).not.toBeNull()
    expect(rail).toHaveAttribute('data-highlighted', 'false')
    expect(container.querySelector('.studio-hud-vernier-cog path')).not.toBeNull()
    expect(container.querySelectorAll('.studio-hud-cog-axis')).toHaveLength(2)
    expect(container.querySelectorAll('.studio-hud-corner')).toHaveLength(4)
    expect(screen.getByText('AX.00 // TYPESET')).toBeInTheDocument()
    expect(screen.getAllByRole('button', { name: '生成高级版 →' })).toHaveLength(1)
    expect(screen.getAllByRole('button', { name: '打开原笔记' })).toHaveLength(1)

    fireEvent.mouseEnter(screen.getByRole('button', { name: '生成高级版 →' }))
    expect(hudStage).toHaveAttribute('data-highlighted', 'true')
    expect(rail).toHaveAttribute('data-highlighted', 'true')
    fireEvent.mouseLeave(screen.getByRole('button', { name: '生成高级版 →' }))
    expect(hudStage).toHaveAttribute('data-highlighted', 'false')
    expect(rail).toHaveAttribute('data-highlighted', 'false')
  })

  it('生成高级版调用预览接口并进入编辑态，原笔记不被修改', async () => {
    renderStudio()
    fireEvent.click(screen.getByRole('button', { name: '生成高级版 →' }))
    expect(await screen.findByText('advanced-editor-stub', {}, { timeout: 2500 })).toBeInTheDocument()
    expect(previewNoteDesign).toHaveBeenCalledWith('n1', 'mono', [])
  })
})