import { invoke } from '@tauri-apps/api/core'

export type NoteTheme = 'mono' | 'paper'
export type NoteImage = { origin: 'chat' | 'capture' | 'asset'; id: string; caption: string; position: 'before' | 'after' }
export type NoteFrameOption = { imageId: string; recordId: string; atSeconds: number; origin: 'chat' | 'capture' | 'asset' }
export type NoteDesign = {
  id: string
  noteId: string
  theme: NoteTheme
  createdAt: number
  stale: boolean
  html: string
  images: NoteImage[]
}

export const previewNoteDesign = (noteId: string, theme: NoteTheme, images: NoteImage[] = []) =>
  invoke<string>('preview_note_design', { noteId, theme, images })

export const saveNoteDesign = (noteId: string, theme: NoteTheme, expectedUpdatedAt: number, images: NoteImage[] = []) =>
  invoke<NoteDesign>('save_note_design', { noteId, theme, expectedUpdatedAt, images })

export const getNoteDesign = (noteId: string) =>
  invoke<NoteDesign | null>('get_note_design', { noteId })

export const listNoteFrames = (noteId: string) =>
  invoke<NoteFrameOption[]>('list_note_frames', { noteId })
export const exportNoteDesign = (noteId: string, format: 'html' | 'pdf') =>
  invoke<string>('export_note_design', { noteId, format })
export const deleteNoteDesign = (noteId: string) =>
  invoke<number>('delete_note_design', { noteId })
export const getNoteFrameImage = (noteId: string, imageId: string, origin: NoteFrameOption['origin']) =>
  invoke<string>('get_note_frame_image', { noteId, imageId, origin })
export const captureNoteFrame = (noteId: string, recordId: string, atSeconds: number) =>
  invoke<NoteFrameOption>('capture_note_frame', { noteId, recordId, atSeconds })
