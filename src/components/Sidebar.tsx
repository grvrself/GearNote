import { useState, type ButtonHTMLAttributes, type ReactNode } from 'react'
import type { ChatSessionSummary } from '../agent'
import './sidebar.css'

export type View = 'chat' | 'library' | 'tags' | 'archive' | 'settings'
export type SettingsSection = 'overview' | 'shortcuts'

export const navigation: { id: View; label: string; icon: IconName }[] = [
  { id: 'chat', label: 'Agent 对话', icon: 'chat' },
  { id: 'library', label: '资料库', icon: 'library' },
  { id: 'tags', label: '标签 / 合集', icon: 'tags' },
  { id: 'archive', label: '归档', icon: 'archive' },
  { id: 'settings', label: '设置', icon: 'settings' },
]

type IconName = 'chat' | 'library' | 'tags' | 'archive' | 'settings' | 'search' | 'capture' | 'video' | 'summary' | 'mini' | 'shortcut'

function SidebarIcon({ name }: { name: IconName }) {
  const paths: Record<IconName, ReactNode> = {
    chat: <path d="M3 4.5h10v7H7l-3.5 2v-2H3z" />,
    library: <><path d="M3 3h4v4H3zM9 3h4v4H9zM3 9h4v4H3zM9 9h4v4H9z" /></>,
    tags: <><path d="M3 3h6l4 5-5 5-5-5z" /><path d="M6 6h1" /></>,
    archive: <><path d="M2.5 4h11v2.5h-11zM4 6.5v6h8v-6M6 9h4" /></>,
    settings: <><path d="M3 4h10M3 8h10M3 12h10" /><path d="M6 2.5v3M10 6.5v3M7 10.5v3" /></>,
    search: <><circle cx="7" cy="7" r="4" /><path d="m10 10 3.5 3.5" /></>,
    capture: <><path d="M3 3h10v10H3zM8 5v5m-2-2 2 2 2-2" /></>,
    video: <><path d="M2.5 3.5h11v9h-11zM6.5 6l3 2-3 2zM5 11h6" /></>,
    summary: <><path d="M3 4h7M3 8h10M3 12h8M12 3v3m-1.5-1.5h3" /></>,
    mini: <><path d="M2.5 4h11v8h-11zM8 7h5.5v5H8z" /></>,
    shortcut: <><path d="M2.5 4h11v8h-11zM4.5 6h1M7.5 6h1M10.5 6h1M5 9.5h6" /></>,
  }
  return <svg className="sidebar-icon" viewBox="0 0 16 16" aria-hidden="true">{paths[name]}</svg>
}

type SideButtonProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children'> & {
  icon: IconName
  label: string
  active?: boolean
  hint?: string
  kind: 'nav' | 'command'
}

function SideButton({ icon, label, active = false, hint, kind, disabled, title, ...props }: SideButtonProps) {
  return <div className="side-button-wrapper" data-kind={kind} data-active={active} data-disabled={disabled || undefined} title={title ?? label}>
    <div className="side-button-border">
      <button className="side-button-content" type="button" disabled={disabled} aria-label={disabled ? `${label}，暂不可用` : label} {...props}>
        {active && <span className="side-active-corner" aria-hidden="true">⌜</span>}
        <SidebarIcon name={icon} />
        <span className="side-button-label">{label}</span>
        {hint && <kbd className="side-button-hint">{hint}</kbd>}
      </button>
    </div>
  </div>
}

type Props = {
  view: View
  recordCount: number
  sessions: ChatSessionSummary[]
  activeSessionId: string
  sessionBusy: boolean
  sessionError: string
  onViewChange: (view: View) => void
  onCreateSession: () => void
  onSwitchSession: (id: string) => void
  onRenameSession: (id: string, title: string) => void
  onDeleteSession: (id: string) => void
}

export function Sidebar({ view, recordCount, sessions, activeSessionId, sessionBusy, sessionError, onViewChange, onCreateSession, onSwitchSession, onRenameSession, onDeleteSession }: Props) {
  const [query, setQuery] = useState('')
  const [renamingId, setRenamingId] = useState<string | null>(null)
  const [draftTitle, setDraftTitle] = useState('')
  const [sessionListOpen, setSessionListOpen] = useState(false)
  const visibleSessions = sessions.filter(session => session.title.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()))
  const activeSession = sessions.find(session => session.id === activeSessionId)

  return <aside className="left-sidebar" id="nav-01" aria-label="工作区导航与会话管理">
    <div className="sidebar-section-heading"><span>NAV-01 / WORKSPACE</span></div>
    <nav className="sidebar-nav" aria-label="主导航">
      {navigation.map(item => <SideButton key={item.id} kind="nav" icon={item.icon} label={item.label} active={view === item.id} aria-current={view === item.id ? 'page' : undefined} onClick={() => onViewChange(item.id)} />)}
    </nav>

    <section className="sidebar-sessions" id="ses-01" aria-labelledby="ses-01-label" data-open={sessionListOpen}>
      <button className="session-rail-toggle" type="button" aria-label="会话管理" aria-expanded={sessionListOpen} onClick={() => setSessionListOpen(open => !open)}>▤</button>
      <div className="session-manager">
        <div className="sidebar-section-heading session-heading"><span id="ses-01-label">会话</span><button type="button" disabled={sessionBusy || !activeSessionId} onClick={onCreateSession} aria-label="新建会话" title="新建会话">＋</button></div>
        <label className="session-search"><input aria-label="查找会话" value={query} onChange={event => setQuery(event.target.value)} placeholder="查找会话" /></label>
        <ul className="session-list" aria-label="会话列表">
          {visibleSessions.map(session => <li className="session-row" key={session.id} data-active={session.id === activeSessionId}>
            {renamingId === session.id ? <form onSubmit={event => { event.preventDefault(); if (draftTitle.trim()) { onRenameSession(session.id, draftTitle); setRenamingId(null) } }}>
              <input autoFocus aria-label="会话名称" value={draftTitle} maxLength={80} onChange={event => setDraftTitle(event.target.value)} onKeyDown={event => { if (event.key === 'Escape') setRenamingId(null) }} />
              <button type="submit" disabled={!draftTitle.trim()}>保存</button><button type="button" onClick={() => setRenamingId(null)}>取消</button>
            </form> : <button className="session-select" type="button" disabled={sessionBusy} aria-current={session.id === activeSessionId ? 'page' : undefined} title={session.title} onClick={() => { onSwitchSession(session.id); setSessionListOpen(false) }}>
              <span>{session.title}</span><small>{session.messageCount} 条</small>
            </button>}
          </li>)}
          {visibleSessions.length === 0 && <li className="session-empty">{sessions.length ? '没有匹配会话' : '暂无会话'}</li>}
        </ul>
        {activeSession && <div className="session-actions"><button type="button" disabled={sessionBusy} onClick={() => { setRenamingId(activeSession.id); setDraftTitle(activeSession.title) }}>重命名</button><button type="button" disabled={sessionBusy} onClick={() => { if (window.confirm(`删除会话“${activeSession.title}”？此操作会删除该会话的对话记录。`)) onDeleteSession(activeSession.id) }}>删除</button></div>}
        {sessionError && <p className="session-error" role="alert">{sessionError}</p>}
      </div>
    </section>

    <footer className="sidebar-footer"><span>本地记录</span><strong>{String(recordCount).padStart(2, '0')}</strong></footer>
  </aside>
}
