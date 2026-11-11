import { getCurrentWindow } from '@tauri-apps/api/window'
import { WebviewWindow } from '@tauri-apps/api/webviewWindow'

export const MINI_READY_EVENT = 'knowflow:mini-ready'
export const MINI_CONTEXT_EVENT = 'knowflow:mini-context'
export const MINI_REVEAL_EVENT = 'knowflow:mini-reveal'
export const MINI_REMOVE_REFERENCE_EVENT = 'knowflow:mini-remove-reference'

export type MiniContextPayload = { referencedIds: string[] }
export type MiniRevealPayload = { id: string; atSeconds?: number }
export type MiniRemoveReferencePayload = { id: string }

const MINI_LABEL = 'mini'
const MINI_SIZE = { width: 420, height: 560, minWidth: 360, minHeight: 420 }

export async function openMiniWindow() {
  const existing = await WebviewWindow.getByLabel(MINI_LABEL)
  if (existing) {
    await existing.show()
    await existing.setFocus()
    return
  }

  const mini = new WebviewWindow(MINI_LABEL, {
    url: '/?window=mini',
    title: 'KnowFlow · 提问小窗',
    ...MINI_SIZE,
    center: true,
    decorations: false,
    shadow: false,
    alwaysOnTop: true,
    resizable: true,
  })

  await new Promise<void>((resolve, reject) => {
    void mini.once('tauri://created', () => resolve()).catch(reject)
    void mini.once('tauri://error', event => reject(new Error(String(event.payload)))).catch(reject)
  })
  await mini.setFocus()
}

export async function returnToMainWindow(): Promise<boolean> {
  const main = await WebviewWindow.getByLabel('main')
  if (!main) return false
  await main.show()
  await getCurrentWindow().hide()
  await main.setFocus()
  return true
}
