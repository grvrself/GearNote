import { useState, type ButtonHTMLAttributes, type ReactNode } from 'react'
import type { ChatSessionSummary } from '../agent'
import { useConfirm } from '../hooks/useConfirm'
import { GearIcon } from './GearIcon'
import { SidebarFloralBackdrop } from './mechanical'
import './sidebar.css'

export type View = 'chat' | 'library' | 'studio' | 'skills' | 'plugins' | 'settings'
export type SettingsSection = 'overview' | 'agent' | 'reading' | 'persona' | 'appearance' | 'data' | 'shortcuts' | 'about'

export const navigation: { id: View; label: string; icon: IconName; index: string }[] = [
  { id: 'chat', label: 'Agent 对话', icon: 'chat', index: '01' },
  { id: 'library', label: '资料库', icon: 'library', index: '02' },
  { id: 'studio', label: '高级笔记', icon: 'studio', index: '03' },
  { id: 'skills', label: '技能', icon: 'skills', index: '04' },
  { id: 'plugins', label: '插件', icon: 'plugins', index: '05' },
  { id: 'settings', label: '设置', icon: 'settings', index: '06' },
]

type IconName = 'chat' | 'library' | 'studio' | 'skills' | 'plugins' | 'settings' | 'search' | 'capture' | 'video' | 'summary' | 'mini' | 'shortcut'

function SidebarIcon({ name }: { name: IconName }) {
  const paths: Record<IconName, ReactNode> = {
    chat: <path d="M3 4.5h10v7H7l-3.5 2v-2H3z" />,
    library: <><path d="M3 3h4v4H3zM9 3h4v4H9zM3 9h4v4H3zM9 9h4v4H9z" /></>,
    studio: <><path d="M3 2.5h7v11H3zM5 6h3M5 9h3" /><circle cx="12" cy="12" r="3.2" /><path d="M12 7.5v1.2M12 15.3v1.2M7.5 12h1.2M15.3 12h1.2" /></>,
    skills: <><path d="M8 1.5v3m0 7v3M1.5 8h3m7 0h3M3.4 3.4l2.1 2.1m5 5 2.1 2.1m0-9.2-2.1 2.1m-5 5-2.1 2.1" /><circle cx="8" cy="8" r="2" /></>,
    plugins: <path d="M3 3h3v-.5a1.5 1.5 0 0 1 3 0V3h3v3h.5a1.5 1.5 0 0 1 0 3H12v3H9v.5a1.5 1.5 0 0 1-3 0V12H3V9h-.5a1.5 1.5 0 0 1 0-3H3z" />,
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
  index?: string
  kind: 'nav' | 'command'
}

function SideButton({ icon, label, active = false, hint, index, kind, disabled, title, ...props }: SideButtonProps) {
  return <div className="side-button-wrapper" data-kind={kind} data-active={active} data-disabled={disabled || undefined} title={title ?? label}>
    <div className="side-button-border">
      <button className="side-button-content" type="button" disabled={disabled} aria-label={disabled ? `${label}，暂不可用` : label} {...props}>
        {active && <span className="side-active-corner" aria-hidden="true">⌜</span>}
        {index && <span className="side-button-index" aria-hidden="true">{index}</span>}
        <SidebarIcon name={icon} />
        <span className="side-button-label">{label}</span>
        {active && kind === 'nav' && <span className="mono-active-gear"><GearIcon kind="sidebar" /></span>}
        {hint && <kbd className="side-button-hint">{hint}</kbd>}
      </button>
    </div>
  </div>
}

type Props = {
  view: View
  sessions: ChatSessionSummary[]
  activeSessionId: string
  sessionBusy: boolean
  sessionError: string
  onViewChange: (view: View) => void
  onCreateSession: () => void
  onSwitchSession: (id: string) => void
  onDeleteSession: (id: string) => void
}

export function Sidebar({ view, sessions, activeSessionId, sessionBusy, sessionError, onViewChange, onCreateSession, onSwitchSession, onDeleteSession }: Props) {
  const { confirm, confirmDialog } = useConfirm()
  const [query, setQuery] = useState('')
  const [sessionListOpen, setSessionListOpen] = useState(false)
  const visibleSessions = sessions.filter(session => session.title.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()))

  const requestDelete = (session: ChatSessionSummary) => {
    void (async () => {
      if (await confirm({ title: '删除会话', message: `删除会话“${session.title}”？此操作会删除该会话的对话记录。`, confirmLabel: '删除', danger: true })) onDeleteSession(session.id)
    })()
  }

  return <aside className="left-sidebar" id="nav-01" aria-label="工作区导航与会话列表">
    <div className="sidebar-section-heading"><span>NAV-01 / WORKSPACE</span></div>
    <div className="mono-workspace-gear" aria-hidden="true"><GearIcon kind="sidebar" size={20} /></div>
    <nav className="sidebar-nav" aria-label="主导航">
      {navigation.map(item => <SideButton key={item.id} kind="nav" icon={item.icon} index={item.index} label={item.label} active={view === item.id} aria-current={view === item.id ? 'page' : undefined} onClick={() => onViewChange(item.id)} />)}
    </nav>

    <section className="sidebar-sessions" id="ses-01" aria-labelledby="ses-01-label" data-open={sessionListOpen}>
      <button className="session-rail-toggle" type="button" aria-label="会话列表" aria-expanded={sessionListOpen} onClick={() => setSessionListOpen(open => !open)}>▤</button>
      <div className="session-manager">
        <div className="sidebar-section-heading session-heading"><span id="ses-01-label">会话</span><button type="button" disabled={sessionBusy || !activeSessionId} onClick={onCreateSession} aria-label="新建会话" title="新建会话">＋</button></div>
        <label className="session-search"><input aria-label="查找会话" value={query} onChange={event => setQuery(event.target.value)} placeholder="查找会话" /></label>
        <ul className="session-list" aria-label="会话列表">
          {visibleSessions.map(session => <li className="session-row" key={session.id} data-active={session.id === activeSessionId}>
            <button className="session-select" type="button" disabled={sessionBusy} aria-current={session.id === activeSessionId ? 'page' : undefined} title={session.title} onClick={() => { onSwitchSession(session.id); setSessionListOpen(false) }}>
              <span>{session.title}</span>
              {session.id === activeSessionId && <span className="mono-active-gear"><GearIcon kind="sidebar" /></span>}
            </button>
            <button className="session-delete" type="button" disabled={sessionBusy} aria-label={`删除会话 ${session.title}`} title="删除会话" onClick={() => requestDelete(session)}>
              <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2.5 4.5h11M6.5 2.5h3M4 4.5l.7 9h6.6l.7-9M6.5 7v4M9.5 7v4" /></svg>
            </button>
          </li>)}
          {visibleSessions.length === 0 && <li className="session-empty">{sessions.length ? '没有匹配会话' : '暂无会话'}</li>}
        </ul>
        {sessionError && <p className="session-error" role="alert">{sessionError}</p>}
      </div>
    </section>
    <SidebarFloralBackdrop />
    {confirmDialog}
  </aside>
}
