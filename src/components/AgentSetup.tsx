import { useEffect, useState } from 'react'
import { isTauri } from '@tauri-apps/api/core'
import { getAgentStatus, saveAgentConfig, testAgentTools, type AgentStatus, type ToolProbe } from '../agent'
import { ChamferCard } from './ChamferCard'
import { HudButton } from './HudButton'
import './agent-setup.css'

type Props = { onStatusChange: (status: AgentStatus) => void }

export function AgentSetup({ onStatusChange }: Props) {
  const [baseUrl, setBaseUrl] = useState('https://api.openai.com/v1')
  const [model, setModel] = useState('')
  const [apiKey, setApiKey] = useState('')
  const [hasKey, setHasKey] = useState(false)
  const [keyPersistent, setKeyPersistent] = useState(false)
  const [keyWarning, setKeyWarning] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const [message, setMessage] = useState('')
  const [savedConfig, setSavedConfig] = useState({ baseUrl: '', model: '' })
  const [testing, setTesting] = useState(false)
  const [probe, setProbe] = useState<ToolProbe | null>(null)
  const [probeError, setProbeError] = useState('')

  useEffect(() => {
    if (!isTauri()) return
    void getAgentStatus().then(status => {
      setBaseUrl(status.baseUrl)
      setModel(status.model)
      setSavedConfig({ baseUrl: status.baseUrl, model: status.model })
      setHasKey(status.hasKey)
      setKeyPersistent(status.keyPersistent)
      setKeyWarning(status.keyWarning)
      onStatusChange(status)
    }).catch(() => setMessage('模型配置读取失败'))
  }, [onStatusChange])

  const save = async () => {
    setSaving(true)
    setMessage('')
    try {
      const status = await saveAgentConfig(baseUrl, model, apiKey || undefined)
      setHasKey(status.hasKey)
      setKeyPersistent(status.keyPersistent)
      setKeyWarning(status.keyWarning)
      setBaseUrl(status.baseUrl)
      setModel(status.model)
      setSavedConfig({ baseUrl: status.baseUrl, model: status.model })
      setProbe(null)
      setProbeError('')
      setApiKey('')
      onStatusChange(status)
      setMessage('配置已保存，可以返回对话区提问')
    } catch (error) {
      setMessage(String(error))
    } finally {
      setSaving(false)
    }
  }

  const testTools = async () => {
    setTesting(true)
    setProbe(null)
    setProbeError('')
    try { setProbe(await testAgentTools()) }
    catch (error) { setProbeError(String(error)) }
    finally { setTesting(false) }
  }

  return <ChamferCard className="agent-setup-card"><section className="agent-setup" id="agt-01" aria-labelledby="agt-01-title">
    <div className="agent-setup-head"><span>AGT-01 / MODEL SERVICE</span><h3 id="agt-01-title">Agent 服务</h3></div>
    <p>填写支持 Chat Completions 的 API 地址与模型名称。提问时，所选资料的相关片段会发送到该服务。</p>
    <label>API 地址<input value={baseUrl} onChange={event => setBaseUrl(event.target.value)} spellCheck={false} placeholder="https://api.openai.com/v1" /></label>
    <label>模型名称<input value={model} onChange={event => setModel(event.target.value)} spellCheck={false} placeholder={baseUrl.includes('api.deepseek.com') ? 'deepseek-flash' : '填写服务商提供的模型 ID'} /></label>
    {baseUrl.includes('api.deepseek.com') && <p className="agent-setup-note">DeepSeek V4.1 Flash 的 API 模型 ID 是 <code>deepseek-flash</code>，不要填写版本展示名。</p>}
    <label>API 密钥<input type="password" value={apiKey} onChange={event => setApiKey(event.target.value)} autoComplete="off" placeholder={hasKey ? '已保存；留空则保持原密钥' : '粘贴密钥'} /></label>
    <p className="agent-setup-note">{keyPersistent ? '密钥由当前 Windows 账户加密保存在本机，重启后自动读取；输入框不会显示原文。' : '当前平台的密钥只在本次运行期间保留。'}</p>
    {keyWarning && <p className="agent-setup-note" role="alert">{keyWarning}</p>}
    <div className="agent-setup-actions"><HudButton type="button" variant="primary" disabled={!isTauri() || saving || !baseUrl.trim() || !model.trim()} onClick={() => { void save() }}>{saving ? '保存中' : '保存配置'}</HudButton><span role="status">{message || (hasKey ? '密钥已设置' : '密钥未设置')}</span></div>
    <div className="agent-tool-probe">
      <div><strong>资料探索兼容性</strong><HudButton type="button" disabled={!isTauri() || saving || testing || !hasKey || !savedConfig.model || baseUrl !== savedConfig.baseUrl || model !== savedConfig.model} onClick={() => { void testTools() }}>{testing ? '测试中' : '测试已保存配置'}</HudButton></div>
      <p>点击后会向已保存的模型服务发送一段虚构测试资料，可能产生少量 API 用量；不会读取真实资料或写入会话。修改配置后请先保存。</p>
      {probe && <div className="agent-tool-probe-result" role="status"><strong>{probe.searched && probe.read ? '搜索与读取均通过' : probe.supported ? '只确认了部分工具调用' : '未检测到工具调用'}</strong><span>耗时 {(probe.elapsedMs / 1000).toFixed(1)} 秒</span>{probe.reason && <p>{probe.reason}</p>}{probe.steps.length > 0 && <ol>{probe.steps.map((step, index) => <li key={index}>{step}</li>)}</ol>}</div>}
      {probeError && <p role="alert">{probeError}</p>}
    </div>
  </section></ChamferCard>
}
