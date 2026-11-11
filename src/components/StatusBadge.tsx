import type { ProcessStatus } from '../data'
import './primitives.css'

const labels: Record<ProcessStatus, string> = {
  extracting: 'EXTRACTING', summarizing: 'SUMMARIZING', ready: 'READY', fault: 'FAULT',
}

type Props = { status: ProcessStatus; progress?: number; reason?: string }

export function StatusBadge({ status, progress, reason }: Props) {
  const safeProgress = progress == null ? undefined : Math.max(0, Math.min(100, progress))
  const activeTicks = safeProgress == null ? 0 : Math.round(safeProgress / 100 * 12)
  return <span className="status-unit" data-status={status} role="status" aria-label={`${labels[status]}${safeProgress == null ? '' : ` ${safeProgress}%`}${reason ? `：${reason}` : ''}`}>
    <span className="status-badge">
      {status === 'fault' && <span className="status-hazard" aria-hidden="true" />}
      <span className="status-label">{labels[status]}</span>
      {(status === 'extracting' || status === 'summarizing') && <span className="status-ticks" aria-hidden="true">
        {Array.from({ length: 12 }, (_, index) => <i key={index} className={index < activeTicks ? 'filled' : ''} />)}
      </span>}
    </span>
    {status === 'fault' && reason && <span className="status-reason">{reason}</span>}
  </span>
}
