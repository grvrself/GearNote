import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { invoke } from '@tauri-apps/api/core'
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { applyMotion, applyTheme } from '../theme'
import { PluginsPanel } from './PluginsPanel'

beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: vi.fn(() => ({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn(), addListener: vi.fn(), removeListener: vi.fn() })),
  })
})

vi.mock('@tauri-apps/api/core', async importOriginal => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isTauri: () => true,
  invoke: vi.fn(() => Promise.resolve()),
}))

vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

vi.mock('../extensions', () => ({
  getToolchainStatus: vi.fn(() => Promise.resolve([])),
  openWorkspacePath: vi.fn(() => Promise.resolve()),
}))

beforeEach(() => {
  vi.clearAllMocks()
  applyTheme('hud')
  applyMotion('full')
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: { writeText: vi.fn(() => Promise.resolve()) },
  })
})

describe('PluginsPanel HUD 战术界面与交互', () => {
  it('挂载 HUD 金色页眉蓝图、LIVE BUS 四齿轮节点与三张战术状态卡', async () => {
    render(
      <PluginsPanel
        status={{ connected: true, pairCode: 'abcd1234efgh5678', error: '' }}
        captureJobs={[]}
        onGoSkills={vi.fn()}
      />,
    )

    expect(document.querySelector('.plugins-page')).toBeInTheDocument()
    expect(document.querySelector('.plugin-hud-backdrop')).toHaveAttribute('data-active', 'true')
    expect(document.querySelectorAll('.plugin-hud-gear')).toHaveLength(3)

    const bus = document.querySelector('.connection-bus')
    expect(bus).toHaveAttribute('data-live', 'true')
    expect(bus?.querySelectorAll('.mech-gear')).toHaveLength(4)
    expect(bus?.querySelectorAll('.bus-cyan-pulse')).toHaveLength(3)
    expect(screen.getByText('LIVE BUS')).toBeInTheDocument()

    expect(screen.getByText('浏览器桥接正常')).toBeInTheDocument()
    expect(screen.getByText('视频工具 0 / 4 可用')).toBeInTheDocument()
    expect(document.querySelector('.hud-heartbeat-bars')).toHaveClass('is-live')
    expect(document.querySelector('.hud-warn-triangle')).toBeInTheDocument()
    expect(document.querySelector('.hud-warn-bar')).toBeInTheDocument()
    expect(document.querySelector('.hud-detail-scan-tag')).toHaveTextContent('ACTIVE')
    await waitFor(() => expect(screen.getByText('127.0.0.1:45147')).toBeInTheDocument())
  })

  it('桥接断开时总线处于静止 data-live=false，保留配对码复制与目录打开功能', async () => {
    render(
      <PluginsPanel
        status={{ connected: false, pairCode: '5416a2c7ffff', error: '' }}
        captureJobs={[]}
        onGoSkills={vi.fn()}
      />,
    )

    expect(document.querySelector('.connection-bus')).toHaveAttribute('data-live', 'false')
    expect(document.querySelector('.hud-detail-scan-tag')).toHaveTextContent('STANDBY')

    fireEvent.click(screen.getByRole('button', { name: '复制' }))
    await waitFor(() => expect(navigator.clipboard.writeText).toHaveBeenCalledWith('5416a2c7ffff'))
    expect(screen.getByText('配对码已复制；请勿分享给其他人。')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '打开扩展文件夹' }))
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('open_extension_folder'))
  })

  it('连接器分组切换与 perm-allow / perm-deny 权限标识正常渲染', async () => {
    render(<PluginsPanel status={{ connected: true, pairCode: '5416a2c7ffff', error: '' }} captureJobs={[]} onGoSkills={vi.fn()} />)

    expect(document.querySelectorAll('.permission-box li.perm-allow')).toHaveLength(2)
    expect(document.querySelectorAll('.permission-box li.perm-deny')).toHaveLength(2)

    fireEvent.click(screen.getByRole('button', { name: /视频工具箱/ }))
    await waitFor(() => expect(screen.getByText('OFFICIAL CONNECTOR / VIDEO')).toBeInTheDocument())

    fireEvent.click(screen.getByRole('button', { name: '社区' }))
    expect(screen.getByText('连接器协议尚未开放执行')).toBeInTheDocument()
  })
})

describe('PluginsPanel 简约黑白怀表组件与主题隔离', () => {
  it('mono 主题下将页眉齿轮暗纹替换为铜版画怀表 SVG，包含表壳刻度、罗马数字、小秒盘与三枚宝玑指针', () => {
    applyTheme('mono')
    render(
      <PluginsPanel
        status={{ connected: true, pairCode: '5416a2c7ffff', error: '' }}
        captureJobs={[]}
        onGoSkills={vi.fn()}
      />,
    )

    expect(document.querySelector('.extension-page-heading .gn-watermark')).toBeNull()
    const watch = document.querySelector('.pocket-watch-backdrop')
    expect(watch).toBeInTheDocument()
    expect(watch).toHaveAttribute('data-connected', 'true')
    expect(watch).toHaveAttribute('data-motion', 'full')
    expect(watch).toHaveAttribute('aria-hidden', 'true')

    const svg = watch?.querySelector('svg')
    expect(svg).toHaveAttribute('viewBox', '0 0 340 220')
    expect(svg).toHaveAttribute('fill', 'none')

    expect(svg?.querySelector('.pocket-watch-case circle[cx="130"][cy="110"][r="96"]')).toBeInTheDocument()
    expect(svg?.querySelector('.pocket-watch-case circle[cx="130"][cy="110"][r="91"]')).toBeInTheDocument()
    expect(svg?.querySelector('.pocket-watch-case circle[cx="130"][cy="110"][r="84"]')).toBeInTheDocument()
    expect(svg?.querySelectorAll('.pocket-watch-railroad line')).toHaveLength(60)
    expect(svg?.querySelectorAll('.pocket-watch-numerals text')).toHaveLength(12)
    expect(screen.getByText('XII')).toBeInTheDocument()
    expect(screen.getByText('VI')).toBeInTheDocument()

    expect(svg?.querySelector('.pocket-watch-subdial circle[cx="94"][cy="128"][r="21"]')).toBeInTheDocument()
    expect(svg?.querySelectorAll('.pocket-watch-subdial line')).toHaveLength(13)
    expect(svg?.querySelector('.pocket-watch-hand--hour')).toBeInTheDocument()
    expect(svg?.querySelector('.pocket-watch-hand--minute')).toBeInTheDocument()
    expect(svg?.querySelector('.pocket-watch-hand--second')).toBeInTheDocument()
    expect(svg?.querySelector('.pocket-watch-hand--sub')).toBeInTheDocument()
    expect(svg?.querySelector('.pocket-watch-hands circle[cx="130"][cy="110"][r="3.5"]')).toBeInTheDocument()
  })

  it('响应浏览器桥接状态、三档动效开关与主题切换隔离', () => {
    applyTheme('mono')
    const view = render(
      <PluginsPanel
        status={{ connected: false, pairCode: '5416a2c7ffff', error: '' }}
        captureJobs={[]}
        onGoSkills={vi.fn()}
      />,
    )

    const watch = document.querySelector('.pocket-watch-backdrop')
    expect(watch).toHaveAttribute('data-connected', 'false')

    view.rerender(
      <PluginsPanel
        status={{ connected: true, pairCode: '5416a2c7ffff', error: '' }}
        captureJobs={[]}
        onGoSkills={vi.fn()}
      />,
    )
    expect(document.querySelector('.pocket-watch-backdrop')).toHaveAttribute('data-connected', 'true')

    act(() => { applyMotion('reduced') })
    expect(document.querySelector('.pocket-watch-backdrop')).toHaveAttribute('data-motion', 'reduced')

    act(() => { applyMotion('off') })
    expect(document.querySelector('.pocket-watch-backdrop')).toHaveAttribute('data-motion', 'off')

    act(() => { applyTheme('hud') })
    expect(document.querySelector('.pocket-watch-backdrop')).not.toBeInTheDocument()
    expect(document.querySelector('.plugin-hud-backdrop')).toHaveAttribute('data-active', 'true')
  })
})
