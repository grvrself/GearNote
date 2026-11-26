import { useEffect, useId } from 'react'
import { createPortal } from 'react-dom'
import { HudButton } from './HudButton'
import './confirm-dialog.css'

export type ConfirmOptions = {
  title?: string
  message: string
  confirmLabel?: string
  cancelLabel?: string
  danger?: boolean
}

type Props = ConfirmOptions & { onConfirm: () => void; onCancel: () => void }

// WebView 可能屏蔽原生 window.confirm，应用内确认一律走这个弹窗。
export function ConfirmDialog({ title = '请确认', message, confirmLabel = '确认', cancelLabel = '取消', danger = false, onConfirm, onCancel }: Props) {
  const titleId = useId()
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { event.preventDefault(); onCancel() }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [onCancel])

  return createPortal(<div className="confirm-dialog-backdrop" role="presentation" onClick={event => { if (event.target === event.currentTarget) onCancel() }}>
    <section className="confirm-dialog" role="dialog" aria-modal="true" aria-labelledby={titleId} data-danger={danger || undefined}>
      <span className="confirm-dialog-eyebrow">CONFIRM / ACTION</span>
      <h2 id={titleId}>{title}</h2>
      <p className="confirm-dialog-message">{message}</p>
      <div className="confirm-dialog-actions">
        <HudButton type="button" autoFocus onClick={onCancel}>{cancelLabel}</HudButton>
        <HudButton type="button" variant="primary" onClick={onConfirm}>{confirmLabel}</HudButton>
      </div>
    </section>
  </div>, document.body)
}