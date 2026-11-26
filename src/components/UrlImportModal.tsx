import { useEffect, useId, useState } from 'react'
import { createPortal } from 'react-dom'
import { captureFromUrl, type CaptureResult } from '../records'
import { HudButton } from './HudButton'
import './url-import.css'

type Props = {
  onClose: () => void
  onImported: (result: CaptureResult) => void | Promise<void>
}

export function UrlImportModal({ onClose, onImported }: Props) {
  const titleId = useId()
  const [url, setUrl] = useState('')
  const [cookiesText, setCookiesText] = useState('')
  const [cookiesOpen, setCookiesOpen] = useState(false)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState('')

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !submitting) { event.preventDefault(); onClose() }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [submitting, onClose])

  const submit = async () => {
    if (!url.trim() || submitting) return
    setSubmitting(true); setError('')
    try {
      const result = await captureFromUrl(url.trim(), cookiesText)
      await onImported(result)
      onClose()
    } catch (cause) {
      const message = String(cause)
      setError(message)
      if (/需要登录|cookies/i.test(message)) setCookiesOpen(true)
    } finally { setSubmitting(false) }
  }

  return createPortal(<div className="url-import-backdrop" role="presentation" onClick={event => { if (event.target === event.currentTarget && !submitting) onClose() }}>
    <section className="url-import-modal" role="dialog" aria-modal="true" aria-labelledby={titleId}>
      <div className="url-import-head">
        <div><span>URL / DIRECT CAPTURE</span><h2 id={titleId}>导入链接</h2></div>
        <button type="button" disabled={submitting} aria-label="关闭导入窗口" onClick={onClose}>×</button>
      </div>
      <form onSubmit={event => { event.preventDefault(); void submit() }}>
        <label htmlFor="url-import-input">链接或分享文案</label>
        <textarea id="url-import-input" autoFocus rows={2} value={url} disabled={submitting} spellCheck={false}
          onChange={event => setUrl(event.target.value)}
          placeholder="粘贴链接，或整段分享文案，如 【标题】 https://b23.tv/xxxxxx" />
        <p className="url-import-hint">支持 B 站视频 / b23.tv 短链 / YouTube / 常见网页文章；分享文案会自动提取其中的链接。</p>
        <button className="url-import-cookies-toggle" type="button" aria-expanded={cookiesOpen} onClick={() => setCookiesOpen(open => !open)}>
          {cookiesOpen ? '▾' : '▸'} 可选：B 站 Netscape cookies.txt
        </button>
        {cookiesOpen && <div className="url-import-cookies">
          <label htmlFor="url-import-cookies-input">粘贴 cookies.txt 内容（仅 B 站视频需要）</label>
          <textarea id="url-import-cookies-input" rows={4} value={cookiesText} disabled={submitting} spellCheck={false}
            onChange={event => setCookiesText(event.target.value)} placeholder="# Netscape HTTP Cookie File" />
          <p className="url-import-hint">仅在内存中临时用于本次取流，不写入磁盘。</p>
        </div>}
        {error && <p className="url-import-error" role="alert">{error}</p>}
        <div className="url-import-actions">
          <HudButton type="button" disabled={submitting} onClick={onClose}>取消</HudButton>
          <HudButton type="submit" variant="primary" disabled={submitting || !url.trim()}>{submitting ? '正在抓取…' : '抓取并导入'}</HudButton>
        </div>
      </form>
    </section>
  </div>, document.body)
}