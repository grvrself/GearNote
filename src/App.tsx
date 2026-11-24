import { useCallback, useEffect, useRef, useState } from 'react'
import { isTauri } from '@tauri-apps/api/core'
import { createChatSession, deleteChatSession, previewAgentContext, switchChatSession } from './agent'
import { HistoryPanel } from './components/HistoryPanel'
import { ChatPanel } from './components/ChatPanel'
import { NotesPanel } from './components/NotesPanel'
import { PluginsPanel } from './components/PluginsPanel'
import { SkillsPanel } from './components/SkillsPanel'
import { SettingsPanel } from './components/SettingsPanel'
import { Sidebar, type SettingsSection, type View } from './components/Sidebar'
import { Topbar, type BrowserChoice } from './components/Topbar'
import { recordFixtures } from './data'
import type { ChatFrame } from './chat-data'
import { deleteRecord, dismissCaptureJob, listCaptureJobs, loadSavedRecords } from './records'
import { useRecords } from './hooks/useRecords'
import { exportNote, rejectNoteEdit } from './notes'
import { useNotes } from './hooks/useNotes'
import { useChat } from './hooks/useChat'
import { useMiniWindow } from './hooks/useMiniWindow'
import './app.css'

export default function App() {
  const [view, setView] = useState<View>('chat')
  const [settingsSection, setSettingsSection] = useState<SettingsSection>('overview')
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [rightOpen, setRightOpen] = useState(true)
  const [browser, setBrowser] = useState<BrowserChoice>('Chrome')
  const [focusRequest, setFocusRequest] = useState<{ id: string; atSeconds?: number } | null>(null)
  const [frameToAttach, setFrameToAttach] = useState<{ frame: ChatFrame; sessionId: string } | null>(null)
  const [composeRequest, setComposeRequest] = useState<{ ids: string[]; nonce: number } | null>(null)
  const [studioNoteId, setStudioNoteId] = useState<string | null>(null)
  const [miniError, setMiniError] = useState('')
  const searchRef = useRef<HTMLInputElement>(null)
  const { savedRecords, setSavedRecords, captureJobs, setCaptureJobs, bridgeStatus } = useRecords(setMiniError, id => {
    setView('chat')
    setRightOpen(true)
    setSelectedId(id)
    setFocusRequest({ id })
  })
  const extensionConnected = bridgeStatus?.connected ?? false
  const records = isTauri() ? savedRecords : recordFixtures.map(record => ({ ...record, demo: true }))
  const { notes, noteFolders, noteDraft, setNoteDraft, createNoteDraft, persistNote, removeNote, addNoteFolder,
    changeNoteFolderName, removeNoteFolder, applyNoteEditAndRefresh, undoNoteEditAndRefresh } = useNotes(records, setMiniError, () => setView('library'))
  const { referencedIds, setReferencedIds, agentStatus, setAgentStatus, messages, pendingChoice, pendingLibrarySearch,
    pendingChatNoteEdit, pendingCommand, readContinuation, sessions, activeSessionId, sessionError, workingContext, projectMemory,
    compactedSummary, compactedCount, sending, streaming, suggestingMessageId, suggestionErrorId, agentError,
    sendQuestion, captureStoryboardFrame, decideLibrarySearch, decideChatNoteEdit, decideCommand, manageSession } = useChat(() => setView('chat'), applyNoteEditAndRefresh)
  const agentAvailable = Boolean(agentStatus?.hasKey && agentStatus.model)
  const showHistory = rightOpen && view === 'chat'

  const runSkillQuestion = (question: string, recordIds: string[], deepMode: boolean) => {
    setReferencedIds(recordIds)
    setView('chat')
    void sendQuestion(question, undefined, [], undefined, deepMode, recordIds)
  }
  const composeSkillNote = (recordIds: string[]) => {
    setComposeRequest({ ids: recordIds, nonce: Date.now() })
    setView('library')
  }
  const openSkillStudio = (noteId: string) => {
    setStudioNoteId(noteId)
    setView('studio')
  }

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
  const { miniOpening, handleOpenMini } = useMiniWindow(referencedIds, setReferencedIds, revealRecord, setMiniError)
  const miniAvailable = isTauri() && !miniOpening

  const askStoryboardFrame = async (recordId: string, atSeconds: number) => {
    if (!activeSessionId) throw new Error('会话尚未加载，请稍后重试')
    if (pendingChoice?.status === 'open') throw new Error('请先回答当前选择题，再加入画面')
    const frame = await captureStoryboardFrame(recordId, atSeconds)
    setFrameToAttach({ frame, sessionId: activeSessionId })
    setView('chat')
  }

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
        {view === 'chat' ? <ChatPanel records={records} frameToAttach={frameToAttach} onFrameAttached={() => setFrameToAttach(null)} pendingChoice={pendingChoice} pendingLibrarySearch={pendingLibrarySearch} onResolveLibrarySearch={decideLibrarySearch} pendingChatNoteEdit={pendingChatNoteEdit} onResolveChatNoteEdit={decideChatNoteEdit} pendingCommand={pendingCommand} onResolveCommand={decideCommand} sessionId={activeSessionId} referencedIds={referencedIds} historyOpen={showHistory} agentAvailable={agentAvailable} sending={sending} streaming={streaming} suggestingMessageId={suggestingMessageId} suggestionErrorId={suggestionErrorId} error={agentError} messages={messages} workingContext={workingContext} projectMemory={projectMemory} compactedSummary={compactedSummary} compactedCount={compactedCount} onSend={sendQuestion} onContinueReading={id => sendQuestion('继续读取', undefined, [], undefined, false, undefined, id)} onPreviewContext={(question, frameId, messageIds, deepMode) => previewAgentContext(question, referencedIds, frameId, messageIds, deepMode)} onOpenAgentSetup={() => { setView('settings'); setSettingsSection('agent') }} onCreateNoteDraft={createNoteDraft} onSummarize={deepMode => { void sendQuestion('请总结本轮参考资料的核心内容，并标注来源。', undefined, [], undefined, deepMode) }} onRemoveReference={id => setReferencedIds(current => current.filter(value => value !== id))} onRevealRecord={revealRecord} onToggleHistory={() => setRightOpen(current => !current)} onShowHistory={() => setRightOpen(true)} readContinuation={readContinuation ?? null}  /> : view === 'library' || view === 'studio' ? null : <div className="pending-view extension-host">{view === 'settings' && settingsSection === 'shortcuts' ? <SettingsPanel section={settingsSection} onSectionChange={setSettingsSection} agentStatus={agentStatus} onAgentStatusChange={setAgentStatus} bridgeStatus={bridgeStatus} /> : view === 'plugins' ? <PluginsPanel status={bridgeStatus} captureJobs={captureJobs} onGoSkills={() => setView('skills')} /> : view === 'settings' ? <SettingsPanel section={settingsSection} onSectionChange={setSettingsSection} agentStatus={agentStatus} onAgentStatusChange={setAgentStatus} bridgeStatus={bridgeStatus} /> : <SkillsPanel records={records} notes={notes} agentAvailable={agentAvailable} onRunQuestion={runSkillQuestion} onCompose={composeSkillNote} onOpenStudio={openSkillStudio} onRecordsChanged={async () => setSavedRecords(await loadSavedRecords())} onRevealRecord={revealRecord} onOpenAgentSetup={() => { setView('settings'); setSettingsSection('agent') }} />}</div>}
        <div className="notes-host" hidden={view !== 'library' && view !== 'studio'}><NotesPanel designOpen={view === 'studio'} onDesignOpenChange={open => setView(open ? 'studio' : 'library')} composeRequest={composeRequest} onComposeRequestConsumed={() => setComposeRequest(null)} requestedNoteId={studioNoteId} notes={notes} folders={noteFolders} onCreateFolder={addNoteFolder} onRenameFolder={changeNoteFolderName} onDeleteFolder={removeNoteFolder} records={records} incomingDraft={noteDraft} onDraftConsumed={() => setNoteDraft(null)} onSave={persistNote} onDelete={removeNote} onExport={exportNote} onRevealRecord={revealRecord} onApplyEdit={applyNoteEditAndRefresh} onUndoEdit={undoNoteEditAndRefresh} onRejectEdit={rejectNoteEdit} /></div>
      </main>

      {showHistory && <HistoryPanel records={records} onAskFrame={askStoryboardFrame} captureJobs={captureJobs} selectedId={selectedId} referencedIds={referencedIds} focusRequest={focusRequest} onSelect={id => setSelectedId(current => current === id ? null : id)} onReference={id => setReferencedIds(current => current.includes(id) ? current : [...current, id])} onRemoveReference={id => setReferencedIds(current => current.filter(value => value !== id))} onCollapse={() => setRightOpen(false)} miniAvailable={miniAvailable} onOpenMini={() => { void handleOpenMini() }} searchRef={searchRef} onRecordsChanged={async () => setSavedRecords(await loadSavedRecords())} onDeleteRecord={async record => { await deleteRecord(record.id); setSavedRecords(current => current.filter(item => item.id !== record.id)); setSelectedId(current => current === record.id ? null : current); setReferencedIds(current => current.filter(id => id !== record.id)) }} onDismissCaptureJob={async id => { try { await dismissCaptureJob(id); setCaptureJobs(await listCaptureJobs()) } catch { setMiniError('关闭采集错误失败。') } }} />}
    </div>
  </div>
}


