import { beforeEach, describe, expect, it, vi } from 'vitest'
import { askAgent } from './agent'

const { listeners, invokeState } = vi.hoisted(() => ({
  listeners: new Map<string, (event: { payload: unknown }) => void>(),
  invokeState: { resolve: null as null | ((value: unknown) => void) },
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: (name: string, handler: (event: { payload: unknown }) => void) => {
    listeners.set(name, handler)
    return Promise.resolve(() => {})
  },
}))
vi.mock('@tauri-apps/api/core', () => ({
  invoke: () => new Promise(resolve => { invokeState.resolve = resolve }),
  isTauri: () => true,
}))

beforeEach(() => {
  listeners.clear()
  invokeState.resolve = null
  vi.stubGlobal('crypto', { randomUUID: () => 'req-fixed' })
})

describe('askAgent 真实事件层', () => {
  it('只处理匹配 requestId 的正文、工具活动与阅读状态事件', async () => {
    const deltas: string[] = []
    const activities: string[] = []
    const reading: boolean[] = []
    const promise = askAgent('问题', [], delta => deltas.push(delta), undefined,
      message => activities.push(message), [], undefined, false, undefined,
      (_requestId, value) => reading.push(value))
    await vi.waitFor(() => expect(listeners.size).toBe(3))

    listeners.get('knowflow:agent-stream')!({ payload: { requestId: 'req-fixed', delta: '匹配正文' } })
    listeners.get('knowflow:agent-stream')!({ payload: { requestId: 'req-other', delta: '旧请求正文' } })
    listeners.get('knowflow:agent-tool')!({ payload: { requestId: 'req-fixed', message: '匹配工具' } })
    listeners.get('knowflow:agent-tool')!({ payload: { requestId: 'req-other', message: '旧工具' } })
    listeners.get('knowflow:reading-state')!({ payload: { requestId: 'req-fixed', reading: true, message: '匹配阅读' } })
    listeners.get('knowflow:reading-state')!({ payload: { requestId: 'req-other', reading: false, message: '旧阅读' } })

    expect(deltas).toEqual(['匹配正文'])
    expect(activities).toEqual(['匹配工具', '匹配阅读'])
    expect(reading).toEqual([true])

    invokeState.resolve!({ id: 'reply', role: 'agent', createdAt: '1', paragraphs: ['完成'] })
    await promise
  })
})
