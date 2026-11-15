import { useCallback, useEffect, useRef, useState } from 'react'
import { isTauri } from '@tauri-apps/api/core'
import { createChatSession, deleteChatSession, previewAgentContext, switchChatSession } from './agent'
import { AgentSetup } from './components/AgentSetup'
import { PersonaSetup } from './components/PersonaSetup'
import { ThemeSettings } from './components/ThemeSettings'
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
    pendingChatNoteEdit, pendingCommand, sessions, activeSessionId, sessionError, workingContext, projectMemory,
    compactedSummary, compactedCount, sending, streaming, suggestingMessageId, suggestionErrorId, agentError,
    sendQuestion, captureStoryboardFrame, decideLibrarySearch, decideChatNoteEdit, decideCommand, manageSession } = useChat(() => setView('chat'), applyNoteEditAndRefresh)
  const agentAvailable = Boolean(agentStatus?.hasKey && agentStatus.model)
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
        {view === 'chat' ? <ChatPanel records={records} frameToAttach={frameToAttach} onFrameAttached={() => setFrameToAttach(null)} pendingChoice={pendingChoice} pendingLibrarySearch={pendingLibrarySearch} onResolveLibrarySearch={decideLibrarySearch} pendingChatNoteEdit={pendingChatNoteEdit} onResolveChatNoteEdit={decideChatNoteEdit} pendingCommand={pendingCommand} onResolveCommand={decideCommand} sessionId={activeSessionId} referencedIds={referencedIds} historyOpen={showHistory} agentAvailable={agentAvailable} sending={sending} streaming={streaming} suggestingMessageId={suggestingMessageId} suggestionErrorId={suggestionErrorId} error={agentError} messages={messages} workingContext={workingContext} projectMemory={projectMemory} compactedSummary={compactedSummary} compactedCount={compactedCount} onSend={sendQuestion} onPreviewContext={(question, frameId, messageIds, deepMode) => previewAgentContext(question, referencedIds, frameId, messageIds, deepMode)} onOpenAgentSetup={() => { setView('settings'); setSettingsSection('overview') }} onCreateNoteDraft={createNoteDraft} onSummarize={deepMode => { void sendQuestion('请总结本轮参考资料的核心内容，并标注来源。', undefined, [], undefined, deepMode) }} onRemoveReference={id => setReferencedIds(current => current.filter(value => value !== id))} onRevealRecord={revealRecord} onToggleHistory={() => setRightOpen(current => !current)} onShowHistory={() => setRightOpen(true)}  /> : view === 'library' ? null : <div className="pending-view"><PanelHeading id={`sec-01-${view}${settingsSection === 'shortcuts' ? '-shortcuts' : ''}`} serial="SEC-01" overline={settingsSection === 'shortcuts' ? 'shortcuts' : view} title={pendingTitle} />{view === 'settings' && settingsSection === 'shortcuts' ? <div className="shortcut-preview"><p>当前已启用的应用内快捷键</p><div><span>查找记录</span><kbd>CTRL K</kbd></div><HudButton type="button" onClick={() => setSettingsSection('overview')}>返回设置</HudButton></div> : view === 'plugins' ? <ExtensionSetup status={bridgeStatus} /> : view === 'settings' ? <><div className="settings-actions"><HudButton type="button" onClick={() => setSettingsSection('shortcuts')}>快捷键设置</HudButton></div><ThemeSettings /><AgentSetup onStatusChange={setAgentStatus} /><PersonaSetup /></> : <SkillsPanel />}</div>}
        <div className="notes-host" hidden={view !== 'library'}><NotesPanel notes={notes} folders={noteFolders} onCreateFolder={addNoteFolder} onRenameFolder={changeNoteFolderName} onDeleteFolder={removeNoteFolder} records={records} incomingDraft={noteDraft} onDraftConsumed={() => setNoteDraft(null)} onSave={persistNote} onDelete={removeNote} onExport={exportNote} onRevealRecord={revealRecord} onApplyEdit={applyNoteEditAndRefresh} onUndoEdit={undoNoteEditAndRefresh} onRejectEdit={rejectNoteEdit} /></div>
      </main>

      {showHistory && <HistoryPanel records={records} onAskFrame={askStoryboardFrame} captureJobs={captureJobs} selectedId={selectedId} referencedIds={referencedIds} focusRequest={focusRequest} onSelect={id => setSelectedId(current => current === id ? null : id)} onReference={id => setReferencedIds(current => current.includes(id) ? current : [...current, id])} onRemoveReference={id => setReferencedIds(current => current.filter(value => value !== id))} onCollapse={() => setRightOpen(false)} miniAvailable={miniAvailable} onOpenMini={() => { void handleOpenMini() }} searchRef={searchRef} onRecordsChanged={async () => setSavedRecords(await loadSavedRecords())} onDeleteRecord={async record => { await deleteRecord(record.id); setSavedRecords(current => current.filter(item => item.id !== record.id)); setSelectedId(current => current === record.id ? null : current); setReferencedIds(current => current.filter(id => id !== record.id)) }} onDismissCaptureJob={async id => { try { await dismissCaptureJob(id); setCaptureJobs(await listCaptureJobs()) } catch { setMiniError('关闭采集错误失败。') } }} />}
    </div>
  </div>
}


