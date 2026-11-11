import { invoke } from '@tauri-apps/api/core'
import type { ChatMessage } from './chat-data'
import type { KnowledgeRecord } from './data'

export type NoteSourceInput = { recordId: string; atSeconds?: number }
export type NoteSource = NoteSourceInput & { title: string; url: string }
export type Note = { id: string; title: string; body: string; sources: NoteSource[]; createdAt: number; updatedAt: number }
export type NoteDraft = { title: string; body: string; sources: NoteSourceInput[] }

export const listNotes = () => invoke<Note[]>('list_notes')
export const saveNote = (draft: NoteDraft, id?: string) => invoke<Note>('save_note', { id: id ?? null, ...draft })
export const exportNote = (id: string) => invoke<string>('export_note', { id })

export function draftFromMessage(message: ChatMessage, records: KnowledgeRecord[]): NoteDraft {
  const sources: NoteSourceInput[] = []
  for (const citation of message.citations ?? []) {
    if (!records.some(record => record.id === citation.recordId)) continue
    if (!sources.some(source => source.recordId === citation.recordId && source.atSeconds === citation.atSeconds)) {
      sources.push({ recordId: citation.recordId, atSeconds: citation.atSeconds })
    }
  }
  const first = records.find(record => record.id === sources[0]?.recordId)
  const paragraphs = message.paragraphs.join('\n\n').trim()
  const points = message.points?.map(point => `- ${point}`).join('\n') ?? ''
  return {
    title: first ? `${first.title} · 笔记` : '新笔记',
    body: [paragraphs, points].filter(Boolean).join('\n\n'),
    sources,
  }
}
