import { useEffect, useState } from 'react'
import { isTauri } from '@tauri-apps/api/core'
import { getDefaultPersona, getPersona, reloadPersona, savePersona } from '../agent'
import { ChamferCard } from './ChamferCard'
import { HudButton } from './HudButton'
import './persona-setup.css'

export function PersonaSetup() {
  const [content, setContent] = useState('')
  const [savedContent, setSavedContent] = useState('')
  const [path, setPath] = useState('')
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState('')
  const [error, setError] = useState('')
  const dirty = content !== savedContent

  useEffect(() => {
    if (!isTauri()) return
    void getPersona().then(info => {
      setContent(info.content)
      setSavedContent(info.content)
      setPath(info.path)
    }).catch(cause => setError(`人格文件读取失败：${String(cause)}`))
  }, [])

  const save = async () => {
    setBusy(true)
    setMessage('')
    setError('')
    try {
      const info = await savePersona(content)
      setContent(info.content)
      setSavedContent(info.content)
      setPath(info.path)
      setMessage('人格文件已保存，下一次提问开始生效')
    } catch (cause) {
      setError(String(cause))
    } finally { setBusy(false) }
  }

  const loadDefault = async () => {
    setBusy(true)
    setMessage('')
    setError('')
    try {
      setContent(await getDefaultPersona())
      setMessage('默认人格已载入草稿，点击保存后生效')
    } catch (cause) {
      setError(String(cause))
    } finally { setBusy(false) }
  }

  const reload = async () => {
    setBusy(true)
    setMessage('')
    setError('')
    try {
      const info = await reloadPersona()
      setContent(info.content)
      setSavedContent(info.content)
      setMessage('已从本地人格文件重新读取')
    } catch (cause) {
      setError(String(cause))
    } finally { setBusy(false) }
  }

  return <ChamferCard className="persona-setup-card"><section className="persona-setup" id="prs-01" aria-labelledby="prs-01-title">
    <div className="persona-setup-head"><span>PRS-01 / PERSONA FILE</span><h3 id="prs-01-title">人格文件</h3></div>
    <p>这里决定 Agent 的称呼、语气和表达方式。默认是一位严谨的知识管家，称呼你为「先生」。资料引用和真实性规则仍由问答功能控制。</p>
    <label htmlFor="persona-content">人格内容 <span>{content.length} / 12000 字</span></label>
    <textarea id="persona-content" value={content} onChange={event => { setContent(event.target.value); setMessage(''); setError('') }} maxLength={12000} disabled={!isTauri() || busy || !path} spellCheck={false} aria-describedby="persona-file-note" placeholder={isTauri() ? '正在读取人格文件…' : '请在桌面程序中编辑人格文件'} />
    <p id="persona-file-note" className="persona-file-note">文件：<code>{path || '请在桌面程序中查看'}</code><br />保存后对下一次提问生效。也可以用文本编辑器修改此文件，再点击「从文件重载」。</p>
    <div className="persona-setup-actions">
      <HudButton type="button" variant="primary" disabled={!isTauri() || busy || !path || !dirty || !content.trim()} onClick={() => { void save() }}>保存人格</HudButton>
      <HudButton type="button" disabled={!isTauri() || busy || !path} onClick={() => { void loadDefault() }}>载入默认草稿</HudButton>
      <HudButton type="button" disabled={!isTauri() || busy || !path || !dirty} onClick={() => { setContent(savedContent); setMessage('已撤销未保存的修改'); setError('') }}>撤销修改</HudButton>
      <HudButton type="button" disabled={!isTauri() || busy || !path || dirty} title={dirty ? '请先保存或撤销修改' : '读取本地 persona.md'} onClick={() => { void reload() }}>从文件重载</HudButton>
    </div>
    {error && <p className="persona-setup-error" role="alert">{error}</p>}
    {message && <p className="persona-setup-message" role="status">{message}</p>}
  </section></ChamferCard>
}
