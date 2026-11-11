import { isTauri } from '@tauri-apps/api/core'
import { invoke } from '@tauri-apps/api/core'
import type { KnowledgeRecord, StoryboardResult } from './data'

export type BridgeStatus = { pairCode: string; connected: boolean; error: string | null }
export type RecordPage = { items: KnowledgeRecord[]; total: number; offset: number; limit: number }

export function queryRecordPage(recordType: 'all' | 'video' | 'article', search: string, offset: number, limit = 20): Promise<RecordPage> {
  return invoke<RecordPage>('query_record_page', { recordType, search, offset, limit })
}

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
