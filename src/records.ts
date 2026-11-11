import { isTauri } from '@tauri-apps/api/core'
import { invoke } from '@tauri-apps/api/core'
import type { KnowledgeRecord, StoryboardResult } from './data'

export type BridgeStatus = { pairCode: string; connected: boolean; error: string | null }

export async function loadSavedRecords(): Promise<KnowledgeRecord[]> {
  if (!isTauri()) return []
  return invoke<KnowledgeRecord[]>('list_records')
}

export async function loadBridgeStatus(): Promise<BridgeStatus | null> {
  if (!isTauri()) return null
  return invoke<BridgeStatus>('get_bridge_status')
}

export const analyzeVideoStoryboard = (recordId: string, cookiesText?: string) =>
  invoke<StoryboardResult>('analyze_video_storyboard', { recordId, cookiesText: cookiesText ?? null })

export const getStoryboardImage = (recordId: string, index: number) =>
  invoke<string>('get_storyboard_image', { recordId, index })

export const openVideoAt = (recordId: string, atSeconds: number) =>
  invoke<void>('open_video_at', { recordId, atSeconds })
