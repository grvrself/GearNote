import { fireEvent, render, screen } from '@testing-library/react'
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import type { KnowledgeRecord } from '../data'
import { applyMotion, applyTheme } from '../theme'
import { SkillsPanel } from './SkillsPanel'

beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: vi.fn(() => ({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn(), addListener: vi.fn(), removeListener: vi.fn() })),
  })
})

beforeEach(() => {
  applyTheme('mono')
  applyMotion('full')
})

vi.mock('../extensions', () => ({
  getSkillPrompt: vi.fn(),
  installSkillFromGithub: vi.fn(),
  listInstalledSkills: vi.fn(() => Promise.resolve([])),
  uninstallSkill: vi.fn(),
}))

vi.mock('../records', () => ({ analyzeVideoStoryboard: vi.fn() }))

const record = {
  id: 'r1', type: 'article', title: '测试资料', site: 'example.com',
  savedAt: '2026-11-26T00:00:00Z', status: 'ready',
} as KnowledgeRecord

function renderSkills() {
  return render(<SkillsPanel records={[record]} notes={[]} agentAvailable
    onRunQuestion={vi.fn()} onCompose={vi.fn()} onOpenStudio={vi.fn()}
    onRecordsChanged={vi.fn(async () => {})} onRevealRecord={vi.fn()} onOpenAgentSetup={vi.fn()} />)
}

describe('SkillsPanel 技能工作台', () => {
  it('技能卡片使用等宽序号而不是重复齿轮，统计栏无占宽 flex 刻度干扰，详情头部保留单一齿轮', () => {
    const { container } = renderSkills()
    expect(screen.getAllByText('01').length).toBeGreaterThan(0)
    expect(screen.getAllByText('05').length).toBeGreaterThan(0)
    expect(container.querySelectorAll('.skill-card .gear-icon')).toHaveLength(0)
    expect(container.querySelectorAll('.detail-gear .gear-icon')).toHaveLength(1)
    expect(container.querySelectorAll('.extension-stats > .gn-ruler-divider')).toHaveLength(0)
    expect(container.querySelectorAll('.skill-index')).toHaveLength(5)
    expect(container.querySelector('.gn-watermark svg')).not.toBeNull()
  })

  it('选中资料后计数更新，使用技能按钮进入可执行高亮态', () => {
    const { container } = renderSkills()
    const run = container.querySelector('.gn-skill-run')
    expect(run).not.toBeNull()
    expect(run!.className).not.toContain('gn-skill-run--ready')
    expect(screen.getByText('0 / 8')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('checkbox', { name: '测试资料' }))
    expect(screen.getByText('1 / 8')).toBeInTheDocument()
    expect(container.querySelector('.gn-skill-run')!.className).toContain('gn-skill-run--ready')
  })

  it('HUD 主题下挂载状态指示 LED、PCB 走线背景与右侧双角齿轮及物理开关帽', () => {
    applyTheme('hud')
    const { container } = renderSkills()

    expect(container.querySelector('.hud-stats-led-cell .hud-stats-led')).not.toBeNull()
    expect(container.querySelector('.extension-page-heading .circuit-board-backdrop svg')).not.toBeNull()
    expect(container.querySelectorAll('.circuit-trace-pulse').length).toBe(2)
    expect(container.querySelector('.extruded-gear-3d svg')).not.toBeNull()
    expect(container.querySelectorAll('.extruded-gear-edge').length).toBe(64)
    expect(container.querySelector('.gn-skill-run .hud-switch-cap .gear-icon')).not.toBeNull()
    expect(screen.getByRole('button', { name: '使用技能' })).toBeInTheDocument()
  })
})