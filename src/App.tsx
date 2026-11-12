import { useCallback, useEffect, useRef, useState } from 'react'
import { isTauri } from '@tauri-apps/api/core'
import { askAgent, compactChatContext, suggestWorkingContext, applyWorkingSuggestion, undoWorkingSuggestion, rejectWorkingSuggestion, getPendingWorkingSuggestion, listWorkingWorkflowEvents, confirmProjectMemory, revokeProjectMemory, saveWorkingContext, createChatSession, deleteChatSession, getAgentStatus, getChatState, previewAgentContext, setChatReferences, switchChatSession, type AgentStatus, type ChatSessionSummary, type ChatState } from './agent'
import type { ChatMessage } from './chat-data'
import { AgentSetup } from './components/AgentSetup'
import { PersonaSetup } from './components/PersonaSetup'
import { ThemeSettings } from './components/ThemeSettings'
import { emitTo, listen, type UnlistenFn } from '@tauri-apps/api/event'
import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { HistoryPanel } from './components/HistoryPanel'
import { ChatPanel } from './components/ChatPanel'
import { NotesPanel } from './components/NotesPanel'
import { ExtensionSetup } from './components/ExtensionSetup'
import { SkillsPanel } from './components/SkillsPanel'
import { PanelHeading } from './components/PanelHeading'
import { HudButton } from './components/HudButton'
import { Sidebar, navigation, type SettingsSection, type View } from './components/Sidebar'
import { Topbar, type BrowserChoice } from './components/Topbar'
import { recordFixtures } from './data'
import { dismissCaptureJob, listCaptureJobs, loadBridgeStatus, loadSavedRecords, type BridgeStatus, type CaptureJob } from './records'
import type { KnowledgeRecord } from './data'
import { applyNoteEdit, createNoteFolder, deleteNoteFolder, draftFromMessage, exportNote, getPendingNoteEdit, listNoteFolders, listNoteWorkflowEvents, listNotes, proposeNoteEdit, rejectNoteEdit, renameNoteFolder, saveNote, undoNoteEdit, type Note, type NoteDraft, type NoteFolder } from './notes'
import {
  MINI_CONTEXT_EVENT, MINI_READY_EVENT, MINI_REMOVE_REFERENCE_EVENT, MINI_REVEAL_EVENT,
  openMiniWindow,
  type MiniContextPayload, type MiniRemoveReferencePayload, type MiniRevealPayload,
} from './mini-window'
import './app.css'

export default function App() {
  const [view, setView] = useState<View>('chat')
  const [settingsSection, setSettingsSection] = useState<SettingsSection>('overview')
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [referencedIds, setReferencedIds] = useState<string[]>([])
  const [rightOpen, setRightOpen] = useState(true)
  const [browser, setBrowser] = useState<BrowserChoice>('Chrome')
  const [focusRequest, setFocusRequest] = useState<{ id: string; atSeconds?: number } | null>(null)
  const [miniOpening, setMiniOpening] = useState(false)
  const [miniError, setMiniError] = useState('')
  const [savedRecords, setSavedRecords] = useState<KnowledgeRecord[]>([])
  const [captureJobs, setCaptureJobs] = useState<CaptureJob[]>([])
  const [bridgeStatus, setBridgeStatus] = useState<BridgeStatus | null>(null)
  const [agentStatus, setAgentStatus] = useState<AgentStatus | null>(null)
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [notes, setNotes] = useState<Note[]>([])
  const [noteFolders, setNoteFolders] = useState<NoteFolder[]>([])
  const [noteDraft, setNoteDraft] = useState<NoteDraft | null>(null)
  const [sessions, setSessions] = useState<ChatSessionSummary[]>([])
  const [activeSessionId, setActiveSessionId] = useState('')
  const [sessionError, setSessionError] = useState('')
  const [workingContext, setWorkingContext] = useState<ChatState['workingContext']>({ goal: '', completed: '', nextStep: '', updatedAt: '' })
  const [projectMemory, setProjectMemory] = useState<ChatState['projectMemory']>([])
  const [compactedSummary, setCompactedSummary] = useState('')
  const [compactedCount, setCompactedCount] = useState(0)
  const [sending, setSending] = useState(false)
  const [streaming, setStreaming] = useState<{ question: string; answer: string; activity?: string } | null>(null)
  const [agentError, setAgentError] = useState('')
  const searchRef = useRef<HTMLInputElement>(null)
  const sendingRef = useRef(false)
  const referencedIdsRef = useRef(referencedIds)
  referencedIdsRef.current = referencedIds
  const applyChatState = useCallback((state: ChatState) => {
    setActiveSessionId(state.activeId)
    setSessions(state.sessions)
    setMessages(state.messages)
    setReferencedIds(state.referencedIds)
    setWorkingContext(state.workingContext)
    setProjectMemory(state.projectMemory)
    setCompactedSummary(state.compactedSummary)
    setCompactedCount(state.compactedCount)
  }, [])
  const extensionConnected = bridgeStatus?.connected ?? false
  const agentAvailable = Boolean(agentStatus?.hasKey && agentStatus.model)
  const records = isTauri() ? savedRecords : recordFixtures.map(record => ({ ...record, demo: true }))
  const miniAvailable = isTauri() && !miniOpening
  const showHistory = rightOpen && view === 'chat'
  const pendingTitle = view === 'settings' && settingsSection === 'shortcuts' ? '快捷键设置' : navigation.find(item => item.id === view)?.label ?? ''

  const openSearch = useCallback(() => {
    setView('chat')
    setRightOpen(true)
    window.setTimeout(() => searchRef.current?.focus(), 0)
  }, [])

  const changeView = (nextView: View) => {
    setView(nextView)
    if (nextView === 'settings') setSettingsSection('overview')
  }

  const revealRecord = useCallback((id: string, atSeconds?: number) => {
    setView('chat')
    setSelectedId(id)
    setRightOpen(true)
    setFocusRequest({ id, atSeconds })
  }, [])

  useEffect(() => {
    if (!isTauri()) return
    void listNotes().then(setNotes).catch(() => setMiniError('本地笔记读取失败。'))
    void listNoteFolders().then(setNoteFolders).catch(() => setMiniError('笔记文件夹读取失败。'))
  }, [])

  const createNoteDraft = (message: ChatMessage) => {
    setNoteDraft(draftFromMessage(message, records))
    setView('library')
  }

  const persistNote = async (draft: NoteDraft, id?: string) => {
    const saved = await saveNote(draft, id)
    setNotes(await listNotes())
    return saved
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

  const sendQuestion = async (question: string, frameId?: string, messageIds: string[] = []) => {
    if (sendingRef.current) return false
    sendingRef.current = true
    setSending(true)
    setStreaming({ question, answer: '' })
    setAgentError('')
    try {
      await askAgent(question, referencedIds, delta => setStreaming(current => current && { ...current, answer: current.answer + delta }), frameId,
        activity => setStreaming(current => current && { ...current, activity }), messageIds)
      applyChatState(await getChatState())
      setStreaming(null)
      return true
    } catch (error) {
      try { applyChatState(await getChatState()) } catch { /* Keep the original send error visible. */ }
      setStreaming(null)
      setAgentError(String(error))
      return false
    } finally {
      sendingRef.current = false
      setSending(false)
    }
  }

  const manageSession = async (action: () => Promise<ChatState>) => {
    setSessionError('')
    try {
      applyChatState(await action())
      setView('chat')
    } catch (error) { setSessionError(String(error)) }
  }

  useEffect(() => {
    if (!isTauri()) return
    let disposed = false
    const stops: UnlistenFn[] = []
    void Promise.all([getAgentStatus(), getChatState()]).then(([status, saved]) => {
      if (!disposed) { setAgentStatus(status); applyChatState(saved) }
    }).catch(() => { if (!disposed) setAgentError('对话或模型配置读取失败') })
    void listen('knowflow:chat-changed', () => {
      if (!sendingRef.current) void getChatState().then(saved => { if (!disposed) applyChatState(saved) })
    }).then(stop => { if (disposed) stop(); else stops.push(stop) })
    return () => { disposed = true; stops.forEach(stop => stop()) }
  }, [applyChatState])

  useEffect(() => {
    if (!isTauri() || !activeSessionId) return
    void setChatReferences(activeSessionId, referencedIds).catch(error => setSessionError(String(error)))
  }, [activeSessionId, referencedIds])

  const handleOpenMini = async () => {
    if (!isTauri() || miniOpening) return
    setMiniOpening(true)
    setMiniError('')
    try {
      await openMiniWindow()
    } catch {
      setMiniError('小窗打开失败，请重试。')
    } finally {
      setMiniOpening(false)
    }
  }

  useEffect(() => {
    if (!isTauri()) return
    let disposed = false
    let stop: UnlistenFn | undefined
    const refresh = async () => {
      try {
        const saved = await loadSavedRecords()
        if (!disposed) setSavedRecords(saved)
      } catch {
        if (!disposed) setMiniError('本地资料读取失败，请检查应用数据。')
      }
    }
    void refresh()
    void listen<string>('knowflow:records-changed', event => {
      void refresh().then(() => {
        setView('chat')
        setRightOpen(true)
        setSelectedId(event.payload)
        setFocusRequest({ id: event.payload })
      })
    }).then(unlisten => {
      if (disposed) unlisten()
      else stop = unlisten
    }).catch(() => { if (!disposed) setMiniError('资料更新监听失败。') })
    return () => { disposed = true; stop?.() }
  }, [])

  useEffect(() => {
    if (!isTauri()) return
    let disposed = false
    let stop: UnlistenFn | undefined
    const refresh = async () => {
      try {
        const jobs = await listCaptureJobs()
        if (!disposed) setCaptureJobs(jobs)
      } catch { if (!disposed) setMiniError('采集任务读取失败。') }
    }
    void listen('gearnote:capture-jobs-changed', () => { void refresh() }).then(unlisten => {
      if (disposed) unlisten()
      else { stop = unlisten; void refresh() }
    }).catch(() => { if (!disposed) setMiniError('采集状态监听失败。') })
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

  useEffect(() => {
    if (!isTauri()) return
    let disposed = false
    const stops: UnlistenFn[] = []
    const subscribe = async () => {
      try {
        stops.push(await listen(MINI_READY_EVENT, () => {
          void emitTo<MiniContextPayload>('mini', MINI_CONTEXT_EVENT, { referencedIds: referencedIdsRef.current }).catch(() => setMiniError('小窗资料同步失败。'))
        }))
        stops.push(await listen<MiniRevealPayload>(MINI_REVEAL_EVENT, event => revealRecord(event.payload.id, event.payload.atSeconds)))
        stops.push(await listen<MiniRemoveReferencePayload>(MINI_REMOVE_REFERENCE_EVENT, event => {
          setReferencedIds(current => current.filter(id => id !== event.payload.id))
        }))
        if (disposed) stops.forEach(stop => stop())
      } catch {
        stops.forEach(stop => stop())
        if (!disposed) setMiniError('小窗通信初始化失败。')
      }
    }
    void subscribe()
    return () => { disposed = true; stops.forEach(stop => stop()) }
  }, [revealRecord])

  useEffect(() => {
    if (!isTauri()) return
    void WebviewWindow.getByLabel('mini')
      .then(mini => mini ? emitTo<MiniContextPayload>('mini', MINI_CONTEXT_EVENT, { referencedIds }) : undefined)
      .catch(() => setMiniError('小窗资料同步失败。'))
  }, [referencedIds])

  useEffect(() => {
    const handleShortcut = (event: KeyboardEvent) => {
      if (event.ctrlKey && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        openSearch()
      }
    }
    window.addEventListener('keydown', handleShortcut)
    return () => window.removeEventListener('keydown', handleShortcut)
  }, [openSearch])

  return <div className="app-shell">
    <Topbar sectionId="sec-01" browser={browser} onBrowserChange={setBrowser} extensionConnected={extensionConnected} onOpenExtensionSetup={() => setView('plugins')} />
    {miniError && <div className="app-error" role="alert"><span>{miniError}</span><button type="button" aria-label="关闭错误提示" onClick={() => setMiniError('')}>×</button></div>}

    <div className={`workspace ${showHistory ? '' : 'without-history'}`}>
      <Sidebar view={view} sessions={sessions} activeSessionId={activeSessionId} sessionBusy={sending} sessionError={sessionError} onViewChange={changeView} onCreateSession={() => { void manageSession(createChatSession) }} onSwitchSession={id => { void manageSession(() => switchChatSession(id)) }} onDeleteSession={id => { void manageSession(() => deleteChatSession(id)) }} />

      <main className="main-panel" id="sec-01">
        {view === 'chat' ? <ChatPanel records={records} sessionId={activeSessionId} referencedIds={referencedIds} historyOpen={showHistory} agentAvailable={agentAvailable} sending={sending} streaming={streaming} error={agentError} messages={messages} workingContext={workingContext} projectMemory={projectMemory} compactedSummary={compactedSummary} compactedCount={compactedCount} onCompactContext={async () => applyChatState(await compactChatContext())} onSuggestWorkingContext={suggestWorkingContext} onApplyWorkingSuggestion={async id => applyChatState(await applyWorkingSuggestion(id))} onUndoWorkingSuggestion={async id => applyChatState(await undoWorkingSuggestion(id))} onRejectWorkingSuggestion={rejectWorkingSuggestion} onGetPendingWorkingSuggestion={getPendingWorkingSuggestion} onListWorkingWorkflowEvents={listWorkingWorkflowEvents} onSaveWorkingContext={async (goal, completed, nextStep) => applyChatState(await saveWorkingContext(goal, completed, nextStep))} onConfirmMemory={async (content, source, expiresAt) => applyChatState(await confirmProjectMemory(content, source, expiresAt))} onRevokeMemory={async id => applyChatState(await revokeProjectMemory(id))} onSend={sendQuestion} onPreviewContext={(question, frameId, messageIds) => previewAgentContext(question, referencedIds, frameId, messageIds)} onOpenAgentSetup={() => { setView('settings'); setSettingsSection('overview') }} onCreateNoteDraft={createNoteDraft} onSummarize={() => { void sendQuestion('请总结本轮参考资料的核心内容，并标注来源。') }} onRemoveReference={id => setReferencedIds(current => current.filter(value => value !== id))} onRevealRecord={revealRecord} onToggleHistory={() => setRightOpen(current => !current)} onShowHistory={() => setRightOpen(true)}  /> : view === 'library' ? null : <div className="pending-view"><PanelHeading id={`sec-01-${view}${settingsSection === 'shortcuts' ? '-shortcuts' : ''}`} serial="SEC-01" overline={settingsSection === 'shortcuts' ? 'shortcuts' : view} title={pendingTitle} />{view === 'settings' && settingsSection === 'shortcuts' ? <div className="shortcut-preview"><p>当前已启用的应用内快捷键</p><div><span>查找记录</span><kbd>CTRL K</kbd></div><HudButton type="button" onClick={() => setSettingsSection('overview')}>返回设置</HudButton></div> : view === 'plugins' ? <ExtensionSetup status={bridgeStatus} /> : view === 'settings' ? <><div className="settings-actions"><HudButton type="button" onClick={() => setSettingsSection('shortcuts')}>快捷键设置</HudButton></div><ThemeSettings /><AgentSetup onStatusChange={setAgentStatus} /><PersonaSetup /></> : <SkillsPanel />}</div>}
        <div className="notes-host" hidden={view !== 'library'}><NotesPanel notes={notes} folders={noteFolders} onCreateFolder={addNoteFolder} onRenameFolder={changeNoteFolderName} onDeleteFolder={removeNoteFolder} records={records} incomingDraft={noteDraft} onDraftConsumed={() => setNoteDraft(null)} onSave={persistNote} onExport={exportNote} onRevealRecord={revealRecord} onProposeEdit={proposeNoteEdit} onApplyEdit={applyNoteEditAndRefresh} onUndoEdit={undoNoteEditAndRefresh} onRejectEdit={rejectNoteEdit} onListWorkflowEvents={listNoteWorkflowEvents} onGetPendingEdit={getPendingNoteEdit} /></div>
      </main>

      {showHistory && <HistoryPanel records={records} captureJobs={captureJobs} selectedId={selectedId} referencedIds={referencedIds} focusRequest={focusRequest} onSelect={id => setSelectedId(current => current === id ? null : id)} onReference={id => setReferencedIds(current => current.includes(id) ? current : [...current, id])} onRemoveReference={id => setReferencedIds(current => current.filter(value => value !== id))} onCollapse={() => setRightOpen(false)} miniAvailable={miniAvailable} onOpenMini={() => { void handleOpenMini() }} searchRef={searchRef} onRecordsChanged={async () => setSavedRecords(await loadSavedRecords())} onDismissCaptureJob={async id => { try { await dismissCaptureJob(id); setCaptureJobs(await listCaptureJobs()) } catch { setMiniError('关闭采集错误失败。') } }} />}
    </div>
  </div>
}


