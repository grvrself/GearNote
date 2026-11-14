// 管理置顶小窗的打开、同步和消息监听。
import { useEffect, useRef, useState, type Dispatch, type SetStateAction } from 'react'
import { isTauri } from '@tauri-apps/api/core'
import { emitTo, listen, type UnlistenFn } from '@tauri-apps/api/event'
import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { MINI_CONTEXT_EVENT, MINI_READY_EVENT, MINI_REMOVE_REFERENCE_EVENT, MINI_REVEAL_EVENT,
  openMiniWindow, type MiniContextPayload, type MiniRemoveReferencePayload, type MiniRevealPayload } from '../mini-window'

export function useMiniWindow(referencedIds: string[], setReferencedIds: Dispatch<SetStateAction<string[]>>,
  revealRecord: (id: string, atSeconds?: number) => void, onError: (message: string) => void) {
  const [miniOpening, setMiniOpening] = useState(false)
  const referencedIdsRef = useRef(referencedIds)
  referencedIdsRef.current = referencedIds

  const handleOpenMini = async () => {
    if (!isTauri() || miniOpening) return
    setMiniOpening(true)
    onError('')
    try { await openMiniWindow() }
    catch { onError('小窗打开失败，请重试。') }
    finally { setMiniOpening(false) }
  }

  useEffect(() => {
    if (!isTauri()) return
    let disposed = false
    const stops: UnlistenFn[] = []
    const subscribe = async () => {
      try {
        stops.push(await listen(MINI_READY_EVENT, () => {
          void emitTo<MiniContextPayload>('mini', MINI_CONTEXT_EVENT, { referencedIds: referencedIdsRef.current }).catch(() => onError('小窗资料同步失败。'))
        }))
        stops.push(await listen<MiniRevealPayload>(MINI_REVEAL_EVENT, event => revealRecord(event.payload.id, event.payload.atSeconds)))
        stops.push(await listen<MiniRemoveReferencePayload>(MINI_REMOVE_REFERENCE_EVENT, event => {
          setReferencedIds(current => current.filter(id => id !== event.payload.id))
        }))
        if (disposed) stops.forEach(stop => stop())
      } catch {
        stops.forEach(stop => stop())
        if (!disposed) onError('小窗通信初始化失败。')
      }
    }
    void subscribe()
    return () => { disposed = true; stops.forEach(stop => stop()) }
  }, [revealRecord])

  useEffect(() => {
    if (!isTauri()) return
    void WebviewWindow.getByLabel('mini')
      .then(mini => mini ? emitTo<MiniContextPayload>('mini', MINI_CONTEXT_EVENT, { referencedIds }) : undefined)
      .catch(() => onError('小窗资料同步失败。'))
  }, [referencedIds])

  return { miniOpening, handleOpenMini }
}
