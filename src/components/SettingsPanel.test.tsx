import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { applyMotion, applyTheme } from '../theme'
import { defaultReadingBudget } from '../agent'
import { SettingsPanel } from './SettingsPanel'

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

const fixtures = vi.hoisted(() => ({
  tools: [
    { name: 'ffmpeg', found: true, path: 'C:/ffmpeg.exe', version: 'ffmpeg version 8.1.1-full_build-www.gyan.dev Copyright (c) 2000-2026 the FFmpeg developers', error: '' },
    { name: 'ffprobe', found: true, path: 'C:/ffprobe.exe', version: 'ffprobe version 8.1.1-full_build-www.gyan.dev', error: '' },
    { name: 'node', found: true, path: 'C:/node.exe', version: 'v24.15.0', error: '' },
    { name: 'yt-dlp', found: true, path: 'C:/yt-dlp.exe', version: '2026.10.26', error: '' },
  ],
  workspace: { dataDir: 'C:/data', cacheDir: 'C:/cache', databaseBytes: 2048, cacheBytes: 73728000 },
}))

vi.mock('@tauri-apps/api/core', async importOriginal => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isTauri: () => true,
}))

vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

vi.mock('../extensions', () => ({
  getToolchainStatus: vi.fn(() => Promise.resolve(fixtures.tools)),
  getWorkspaceInfo: vi.fn(() => Promise.resolve(fixtures.workspace)),
  openWorkspacePath: vi.fn(() => Promise.resolve()),
}))

function renderSettings(section: 'overview' | 'about') {
  return render(<SettingsPanel section={section} onSectionChange={vi.fn()} agentStatus={null}
    onAgentStatusChange={vi.fn()} bridgeStatus={null} />)
}

describe('SettingsPanel 系统诊断', () => {
  it('概览诊断行显示清洗后的版本号，完整版权串只保留在 title 中', async () => {
    renderSettings('overview')
    const versions = await screen.findAllByText('v8.1.1')
    expect(versions).toHaveLength(2)
    expect(versions[0]).toHaveAttribute('title', fixtures.tools[0].version)
    expect(versions[1]).toHaveAttribute('title', fixtures.tools[1].version)
    expect(screen.queryByText(/Copyright/)).not.toBeInTheDocument()
    expect(screen.getByText('v24.15.0')).toBeInTheDocument()
    expect(screen.getByText('2026.10.26')).toBeInTheDocument()
  })

  it('关于页完整诊断同样使用清洗版本与 title 提示', async () => {
    renderSettings('about')
    const versions = await screen.findAllByText('v8.1.1')
    expect(versions).toHaveLength(2)
    expect(versions[0]).toHaveAttribute('title', fixtures.tools[0].version)
    expect(screen.queryByText(/Copyright/)).not.toBeInTheDocument()
  })

  it('应用缓存使用游标刻度进度条并带完整 aria 数值', async () => {
    renderSettings('overview')
    const gauge = await screen.findByRole('progressbar', { name: '应用缓存' })
    await waitFor(() => expect(gauge).toHaveAttribute('aria-valuetext', '70.3 MB / 512 MB'))
    expect(gauge).toHaveAttribute('aria-valuemax', String(512 * 1024 * 1024))
    expect(gauge).toHaveAttribute('aria-valuenow', '73728000')
  })

  it('右上角 SettingsGearBackdrop 随分组索引棘轮步进并与三档动效实时联动', async () => {
    const view = renderSettings('overview')
    await screen.findAllByText('v8.1.1')

    const backdrop = document.querySelector('.settings-gear-backdrop')
    expect(backdrop).toBeInTheDocument()
    expect(backdrop).toHaveAttribute('data-section-index', '0')
    expect(backdrop).toHaveAttribute('data-motion', 'full')
    expect(document.querySelectorAll('.settings-gear-ratchet')).toHaveLength(3)

    view.rerender(<SettingsPanel section="appearance" onSectionChange={vi.fn()} agentStatus={null} onAgentStatusChange={vi.fn()} bridgeStatus={null} />)
    expect(document.querySelector('.settings-gear-backdrop')).toHaveAttribute('data-section-index', '4')
    const gearA = document.querySelector('.settings-gear-ratchet--a') as HTMLElement
    const gearB = document.querySelector('.settings-gear-ratchet--b') as HTMLElement
    const gearC = document.querySelector('.settings-gear-ratchet--c') as HTMLElement
    expect(gearA.style.getPropertyValue('--ratchet-angle')).toBe('120deg')
    expect(gearB.style.getPropertyValue('--ratchet-angle')).toBe('-210deg')
    expect(gearC.style.getPropertyValue('--ratchet-angle')).toBe('-168deg')

    fireEvent.click(screen.getByRole('button', { name: '减弱' }))
    expect(document.querySelector('.settings-gear-backdrop')).toHaveAttribute('data-motion', 'reduced')

    fireEvent.click(screen.getByRole('button', { name: '关闭' }))
    expect(document.querySelector('.settings-gear-backdrop')).toHaveAttribute('data-motion', 'off')
  })

  it('左侧栏下方 SettingsBlueprintPlate 渲染点阵矩阵与 CAD 剖面图，并同步分组序号与内环旋转角', async () => {
    const view = renderSettings('overview')
    await screen.findAllByText('v8.1.1')

    expect(screen.getByText('GN.CTRL // 01')).toBeInTheDocument()
    expect(document.querySelectorAll('.blueprint-corner')).toHaveLength(4)
    expect(document.querySelectorAll('.blueprint-matrix-cell')).toHaveLength(180)
    expect(document.querySelector('.blueprint-cad-svg')).toBeInTheDocument()
    expect(screen.getByText('AX-01')).toBeInTheDocument()
    expect(screen.getByText('R.24')).toBeInTheDocument()
    expect(screen.getByText('SYNC')).toBeInTheDocument()
    expect(screen.getByText('0.02')).toBeInTheDocument()

    view.rerender(<SettingsPanel section="appearance" onSectionChange={vi.fn()} agentStatus={null} onAgentStatusChange={vi.fn()} bridgeStatus={null} />)
    expect(screen.getByText('GN.CTRL // 05')).toBeInTheDocument()
    const inner = document.querySelector('.blueprint-cad-inner') as HTMLElement
    expect(inner.style.getPropertyValue('--cad-angle')).toBe('180deg')
  })

  it('工作区概览渲染三联扁平航电遥测、4通道拨码与 KPI 切角刻度条，并响应服务实时状态', async () => {
    const view = render(
      <SettingsPanel
        section="overview"
        onSectionChange={vi.fn()}
        agentStatus={{
          baseUrl: 'https://api.deepseek.com',
          model: 'deepseek-chat',
          exploreReferences: true,
          contextTier: 'medium',
          readingBudget: defaultReadingBudget,
          semanticRetrieval: false,
          embeddingBaseUrl: '',
          embeddingModel: '',
          embeddingHasKey: false,
          hasKey: true,
          keyPersistent: true,
          keyWarning: null,
        }}
        onAgentStatusChange={vi.fn()}
        bridgeStatus={{ pairCode: 'GN-001', connected: false, error: '未连接' }}
      />,
    )

    const agtCol = document.querySelector('.hud-dip-col[data-channel="AGT"]')
    const brgCol = document.querySelector('.hud-dip-col[data-channel="BRG"]')
    const vidCol = document.querySelector('.hud-dip-col[data-channel="VID"]')
    expect(agtCol).toHaveAttribute('data-state', 'up')
    expect(brgCol).toHaveAttribute('data-state', 'down')
    expect(vidCol).toHaveAttribute('data-state', 'mid')

    await screen.findAllByText('v8.1.1')
    expect(document.querySelector('.hud-dip-col[data-channel="VID"]')).toHaveAttribute('data-state', 'up')
    expect(document.querySelector('.hud-dip-col[data-channel="MEM"]')).toHaveAttribute('data-state', 'up')

    expect(screen.getByText('GN-001 // LOCAL')).toBeInTheDocument()
    expect(document.querySelector('.hud-scope-svg .hud-scope-wave')).toBeInTheDocument()
    expect(document.querySelectorAll('.hud-barcode-svg rect').length).toBeGreaterThan(10)

    expect(Array.from(document.querySelectorAll('.hud-kpi-index')).map((el) => el.textContent)).toEqual(['01', '02', '03', '04'])
    expect(document.querySelectorAll('.hud-kpi-ruler')).toHaveLength(4)
    expect(document.querySelectorAll('.quick-settings .hud-box-corners > i')).toHaveLength(4)
    expect(document.querySelectorAll('.diagnostics-card .hud-box-corners > i')).toHaveLength(4)

    view.rerender(
      <SettingsPanel
        section="appearance"
        onSectionChange={vi.fn()}
        agentStatus={null}
        onAgentStatusChange={vi.fn()}
        bridgeStatus={null}
      />,
    )
    expect(document.querySelector('.settings-overview-telemetry')).not.toBeInTheDocument()
  })

  it('简约黑白设置页渲染细线荆棘玫瑰暗纹与可转动 6 孔 CAD 左轮弹仓组件，并严格隔离 hud 主题', async () => {
    applyTheme('mono')
    const view = renderSettings('overview')
    await screen.findAllByText('v8.1.1')

    const rose = document.querySelector('.gn-thorn-rose-backdrop') as HTMLImageElement
    expect(rose).toBeInTheDocument()
    expect(rose.tagName).toBe('IMG')
    expect(rose.getAttribute('src')).toBeTruthy()
    expect(rose).toHaveAttribute('aria-hidden', 'true')

    const revolver = document.querySelector('.revolver-cylinder-widget') as HTMLElement
    expect(revolver).toBeInTheDocument()
    expect(screen.getByText('CYL-06 // CHAMBER')).toBeInTheDocument()
    expect(document.querySelectorAll('.revolver-corner')).toHaveLength(4)
    expect(document.querySelectorAll('.revolver-chamber')).toHaveLength(6)
    expect(document.querySelectorAll('.revolver-chamber[data-loaded="true"]')).toHaveLength(1)
    expect(revolver).toHaveAttribute('data-steps', '0')
    expect(revolver).toHaveAttribute('data-active-chamber', '0')
    expect(revolver).toHaveAttribute('data-bullet-pos', '0')

    const rotor = document.querySelector('.revolver-cylinder-rotor') as SVGGElement
    expect(rotor.style.getPropertyValue('--cylinder-angle')).toBe('0deg')

    // 切换左侧设置分组：单发子弹固定在 0 号弹仓随转轮顶部从左向右（+60deg）顺时针转动
    view.rerender(<SettingsPanel section="agent" onSectionChange={vi.fn()} agentStatus={null} onAgentStatusChange={vi.fn()} bridgeStatus={null} />)
    expect(revolver).toHaveAttribute('data-steps', '1')
    expect(revolver).toHaveAttribute('data-active-chamber', '0')
    expect(revolver).toHaveAttribute('data-bullet-pos', '1')
    expect(rotor.style.getPropertyValue('--cylinder-angle')).toBe('60deg')

    // 再次切换分组：累计 +120deg 持续从左往右顺时针旋转
    view.rerender(<SettingsPanel section="appearance" onSectionChange={vi.fn()} agentStatus={null} onAgentStatusChange={vi.fn()} bridgeStatus={null} />)
    expect(revolver).toHaveAttribute('data-steps', '2')
    expect(revolver).toHaveAttribute('data-bullet-pos', '2')
    expect(rotor.style.getPropertyValue('--cylinder-angle')).toBe('120deg')

    // 点击弹仓交互：从左往右顺时针转至下一孔 +180deg，子弹随物理弹仓旋转而非总是停在 12 点位
    fireEvent.click(revolver)
    expect(revolver).toHaveAttribute('data-steps', '3')
    expect(revolver).toHaveAttribute('data-bullet-pos', '3')
    expect(rotor.style.getPropertyValue('--cylinder-angle')).toBe('180deg')

    // 动效档位联动
    fireEvent.click(screen.getByRole('button', { name: '关闭' }))
    expect(revolver).toHaveAttribute('data-motion', 'off')
    expect(rose).toHaveAttribute('data-motion', 'off')

    // 切换为黑黄机械传动主题时卸载荆棘玫瑰节点，不影响 hud 主题
    fireEvent.click(screen.getByRole('button', { name: /机械传动/ }))
    expect(document.querySelector('.gn-thorn-rose-backdrop')).not.toBeInTheDocument()
    applyTheme('mono')
  })

  it('HUD 主题设置页渲染几何鼠标追踪义眼 MinimalCyberEye，支持阻尼追踪、点击/切页眨眼与主题动效隔离', async () => {
    applyTheme('mono')
    const view = renderSettings('overview')
    await screen.findAllByText('v8.1.1')
    expect(document.querySelector('.minimal-cyber-eye')).not.toBeInTheDocument()

    act(() => { applyTheme('hud') })
    const eye = document.querySelector('.minimal-cyber-eye') as HTMLButtonElement
    expect(eye).toBeInTheDocument()
    expect(eye).toHaveAttribute('data-tracking', 'true')
    expect(eye).toHaveAttribute('data-motion', 'full')
    expect(eye.nextElementSibling).toHaveClass('save-state')

    const svg = eye.querySelector('.minimal-cyber-eye-svg') as SVGSVGElement
    expect(svg).toHaveAttribute('viewBox', '0 0 64 52')
    expect(svg.querySelector('clipPath')).toBeInTheDocument()
    expect(svg.querySelectorAll('.cyber-eye-dots circle')).toHaveLength(6)
    expect(svg.querySelector('.cyber-eye-lid')).toBeInTheDocument()
    expect(svg.querySelector('.cyber-eye-iris circle[r="10.5"]')).toBeInTheDocument()
    expect(svg.querySelector('.cyber-eye-iris circle[r="7.8"]')).toBeInTheDocument()
    expect(svg.querySelector('.cyber-eye-iris rect[width="3"][height="8.5"]')).toBeInTheDocument()

    vi.spyOn(svg, 'getBoundingClientRect').mockReturnValue({
      left: 900,
      top: 60,
      width: 52,
      height: 44,
      right: 952,
      bottom: 104,
      x: 900,
      y: 60,
      toJSON: () => ({}),
    })

    const iris = svg.querySelector('.cyber-eye-iris') as SVGGElement
    expect(iris.getAttribute('transform')).toBe('translate(0, 0)')

    vi.useFakeTimers()
    try {
      fireEvent.mouseMove(window, { clientX: 1200, clientY: 82 })
      act(() => { vi.advanceTimersByTime(120) })

      const movedTransform = iris.getAttribute('transform') ?? ''
      const match = movedTransform.match(/translate\(([-\d.]+),\s*([-\d.]+)\)/)
      expect(match).not.toBeNull()
      const dx = Number(match![1])
      const dy = Number(match![2])
      expect(dx).toBeGreaterThan(2)
      expect(dx).toBeLessThanOrEqual(7)
      expect(Math.abs(dy)).toBeLessThanOrEqual(3.5)

      // 鼠标离开窗口时平滑归中
      fireEvent.mouseOut(window, { relatedTarget: null })
      act(() => { vi.advanceTimersByTime(1200) })
      expect(iris.getAttribute('transform')).toBe('translate(0.00, 0.00)')

      // 点击义眼触发 180ms 眨眼动画
      fireEvent.click(eye)
      expect(eye).toHaveAttribute('data-blinking', 'true')
      act(() => { vi.advanceTimersByTime(200) })
      expect(eye).toHaveAttribute('data-blinking', 'false')

      // 切换设置分组时触发眨眼动画
      view.rerender(<SettingsPanel section="agent" onSectionChange={vi.fn()} agentStatus={null} onAgentStatusChange={vi.fn()} bridgeStatus={null} />)
      expect(eye).toHaveAttribute('data-blinking', 'true')
      act(() => { vi.advanceTimersByTime(200) })
      expect(eye).toHaveAttribute('data-blinking', 'false')

      // 减弱/关闭动效时停止鼠标追踪并居中
      act(() => { applyMotion('reduced') })
      expect(eye).toHaveAttribute('data-tracking', 'false')
      expect(iris.getAttribute('transform')).toBe('translate(0, 0)')

      fireEvent.mouseMove(window, { clientX: 1200, clientY: 82 })
      act(() => { vi.advanceTimersByTime(120) })
      expect(iris.getAttribute('transform')).toBe('translate(0, 0)')

      // 切回 mono 主题卸载节点
      act(() => { applyTheme('mono') })
      expect(document.querySelector('.minimal-cyber-eye')).not.toBeInTheDocument()
    } finally {
      vi.useRealTimers()
    }
  })
})