// 管理笔记、文件夹和从聊天生成的草稿。
import { useEffect, useState } from 'react'
import { isTauri } from '@tauri-apps/api/core'
import type { ChatMessage } from '../chat-data'
import type { KnowledgeRecord } from '../data'
import { applyNoteEdit, createNoteFolder, deleteNote, deleteNoteFolder, draftFromMessage, listNoteFolders, listNotes, renameNoteFolder, saveNote, undoNoteEdit, type Note, type NoteDraft, type NoteFolder } from '../notes'

export function useNotes(records: KnowledgeRecord[], onError: (message: string) => void, openLibrary: () => void) {
  const [notes, setNotes] = useState<Note[]>([])
  const [noteFolders, setNoteFolders] = useState<NoteFolder[]>([])
  const [noteDraft, setNoteDraft] = useState<NoteDraft | null>(null)

  useEffect(() => {
    if (!isTauri()) return
    void listNotes().then(setNotes).catch(() => onError('本地笔记读取失败。'))
    void listNoteFolders().then(setNoteFolders).catch(() => onError('笔记文件夹读取失败。'))
  }, [])

  const createNoteDraft = (message: ChatMessage) => {
    setNoteDraft(draftFromMessage(message, records))
    openLibrary()
  }
  const persistNote = async (draft: NoteDraft, id?: string) => {
    const saved = await saveNote(draft, id)
    setNotes(await listNotes())
    return saved
  }
  const removeNote = async (id: string) => {
    await deleteNote(id)
    setNotes(await listNotes())
  }
  const addNoteFolder = async (name: string) => {
    const folder = await createNoteFolder(name)
    setNoteFolders(await listNoteFolders())
    return folder
  }
  const changeNoteFolderName = async (id: string, name: string) => {
    await renameNoteFolder(id, name)
    setNoteFolders(await listNoteFolders())
  }
  const removeNoteFolder = async (id: string) => {
    const moved = await deleteNoteFolder(id)
    setNoteFolders(await listNoteFolders())
    setNotes(await listNotes())
    return moved
  }
  const applyNoteEditAndRefresh = async (proposalId: string) => {
    const saved = await applyNoteEdit(proposalId)
    setNotes(await listNotes())
    return saved
  }
  const undoNoteEditAndRefresh = async (proposalId: string) => {
    const restored = await undoNoteEdit(proposalId)
    setNotes(await listNotes())
    return restored
  }

  return { notes, noteFolders, noteDraft, setNoteDraft, createNoteDraft, persistNote, removeNote, addNoteFolder,
    changeNoteFolderName, removeNoteFolder, applyNoteEditAndRefresh, undoNoteEditAndRefresh }
}
