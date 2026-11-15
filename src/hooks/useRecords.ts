// 订阅本地资料、采集任务和扩展连接状态。
import { useEffect, useState } from 'react'
import { isTauri } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { listCaptureJobs, loadBridgeStatus, loadSavedRecords, type BridgeStatus, type CaptureJob } from '../records'
import type { KnowledgeRecord } from '../data'

export function useRecords(onError: (message: string) => void, onCaptured: (id: string) => void) {
  const [savedRecords, setSavedRecords] = useState<KnowledgeRecord[]>([])
  const [captureJobs, setCaptureJobs] = useState<CaptureJob[]>([])
  const [bridgeStatus, setBridgeStatus] = useState<BridgeStatus | null>(null)

  useEffect(() => {
    if (!isTauri()) return
    let disposed = false
    const stops: UnlistenFn[] = []
    const refresh = async () => {
      try {
        const saved = await loadSavedRecords()
        if (!disposed) setSavedRecords(saved)
      } catch {
        if (!disposed) onError('本地资料读取失败，请检查应用数据。')
      }
    }
    void refresh()
    void listen<string>('knowflow:records-changed', event => {
      void refresh().then(() => onCaptured(event.payload))
    }).then(unlisten => {
      if (disposed) unlisten()
      else stops.push(unlisten)
    }).catch(() => { if (!disposed) onError('资料更新监听失败。') })
    void listen('gearnote:record-deleted', () => { void refresh() }).then(unlisten => {
      if (disposed) unlisten()
      else stops.push(unlisten)
    }).catch(() => { if (!disposed) onError('资料删除监听失败。') })
    return () => { disposed = true; stops.forEach(stop => stop()) }
  }, [])

  useEffect(() => {
    if (!isTauri()) return
    let disposed = false
    let stop: UnlistenFn | undefined
    const refresh = async () => {
      try {
        const jobs = await listCaptureJobs()
        if (!disposed) setCaptureJobs(jobs)
      } catch { if (!disposed) onError('采集任务读取失败。') }
    }
    void listen('gearnote:capture-jobs-changed', () => { void refresh() }).then(unlisten => {
      if (disposed) unlisten()
      else { stop = unlisten; void refresh() }
    }).catch(() => { if (!disposed) onError('采集状态监听失败。') })
    return () => { disposed = true; stop?.() }
  }, [])

  useEffect(() => {
    if (!isTauri()) return
    let disposed = false
    const refresh = async () => {
      try {
        const status = await loadBridgeStatus()
        if (!disposed) setBridgeStatus(status)
      } catch {
        if (!disposed) setBridgeStatus(null)
      }
    }
    void refresh()
    const timer = window.setInterval(() => { void refresh() }, 5000)
    return () => { disposed = true; window.clearInterval(timer) }
  }, [])

  return { savedRecords, setSavedRecords, captureJobs, setCaptureJobs, bridgeStatus }
}
