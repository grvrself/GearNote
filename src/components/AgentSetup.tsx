import { useEffect, useState } from 'react'
import { isTauri } from '@tauri-apps/api/core'
import { getAgentStatus, saveReadingBudget, readingPreset, defaultReadingBudget, type ReadingBudget, saveAgentConfig, saveExploreReferences, saveContextTier, saveSemanticRetrieval, testSemanticRetrieval, testAgentTools, type AgentStatus, type ToolProbe, type SemanticProbe } from '../agent'
import { ChamferCard } from './ChamferCard'
import { HudButton } from './HudButton'
import './agent-setup.css'

type Props = { onStatusChange: (status: AgentStatus) => void; section?: 'full' | 'connection' | 'reading' }

export function AgentSetup({ onStatusChange, section = 'full' }: Props) {
  const [budget, setBudget] = useState<ReadingBudget>(defaultReadingBudget)
  const [savingBudget, setSavingBudget] = useState(false)
  const [budgetMessage, setBudgetMessage] = useState('')
  const [baseUrl, setBaseUrl] = useState('https://api.openai.com/v1')
  const [model, setModel] = useState('')
  const [apiKey, setApiKey] = useState('')
  const [hasKey, setHasKey] = useState(false)
  const [keyPersistent, setKeyPersistent] = useState(false)
  const [keyWarning, setKeyWarning] = useState<string | null>(null)
  const [exploreEnabled, setExploreEnabled] = useState(true)
  const [contextTier, setContextTier] = useState<'small' | 'medium'>('small')
  const [semanticEnabled, setSemanticEnabled] = useState(false)
  const [embeddingBaseUrl, setEmbeddingBaseUrl] = useState('')
  const [embeddingModel, setEmbeddingModel] = useState('')
  const [embeddingKey, setEmbeddingKey] = useState('')
  const [embeddingHasKey, setEmbeddingHasKey] = useState(false)
  const [savingSemantic, setSavingSemantic] = useState(false)
  const [semanticMessage, setSemanticMessage] = useState('')
  const [semanticProbeBusy, setSemanticProbeBusy] = useState(false)
  const [semanticProbe, setSemanticProbe] = useState<SemanticProbe | null>(null)
  const [savingTier, setSavingTier] = useState(false)
  const [tierMessage, setTierMessage] = useState('')
  const [savingExplore, setSavingExplore] = useState(false)
  const [exploreMessage, setExploreMessage] = useState('')
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
      setExploreEnabled(status.exploreReferences)
      setContextTier(status.contextTier)
      setBudget(status.readingBudget)
      setSemanticEnabled(status.semanticRetrieval)
      setEmbeddingBaseUrl(status.embeddingBaseUrl)
      setEmbeddingModel(status.embeddingModel)
      setEmbeddingHasKey(status.embeddingHasKey)
      setSemanticProbe(null)
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
      setExploreEnabled(status.exploreReferences)
      setContextTier(status.contextTier)
      setBudget(status.readingBudget)
      setSemanticEnabled(status.semanticRetrieval)
      setEmbeddingBaseUrl(status.embeddingBaseUrl)
      setEmbeddingModel(status.embeddingModel)
      setEmbeddingHasKey(status.embeddingHasKey)
      setSemanticProbe(null)
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

  const changeExplore = async (enabled: boolean) => {
    setSavingExplore(true)
    setExploreMessage('')
    try {
      const status = await saveExploreReferences(enabled)
      setExploreEnabled(status.exploreReferences)
      onStatusChange(status)
      setExploreMessage('已保存')
    } catch (error) { setExploreMessage(String(error)) }
    finally { setSavingExplore(false) }
  }

  const changeTier = async (tier: 'small' | 'medium') => {
    setSavingTier(true)
    setTierMessage('')
    try {
      const status = await saveContextTier(tier)
      setContextTier(status.contextTier)
      setBudget(status.readingBudget)
      onStatusChange(status)
      setTierMessage('已保存')
    } catch (error) { setTierMessage(String(error)) }
    finally { setSavingTier(false) }
  }

  const saveSemantic = async () => {
    setSavingSemantic(true)
    setSemanticMessage('')
    try {
      const status = await saveSemanticRetrieval(semanticEnabled, embeddingBaseUrl, embeddingModel, embeddingKey || undefined)
      setSemanticEnabled(status.semanticRetrieval)
      setEmbeddingBaseUrl(status.embeddingBaseUrl)
      setEmbeddingModel(status.embeddingModel)
      setEmbeddingHasKey(status.embeddingHasKey)
      setEmbeddingKey('')
      setSemanticProbe(null)
      onStatusChange(status)
      setSemanticMessage('已保存')
    } catch (error) { setSemanticMessage(String(error)) }
    finally { setSavingSemantic(false) }
  }

  const testSemantic = async () => {
    setSemanticProbeBusy(true)
    setSemanticMessage('')
    try { setSemanticProbe(await testSemanticRetrieval()) }
    catch (error) { setSemanticMessage(String(error)) }
    finally { setSemanticProbeBusy(false) }
  }

  return <ChamferCard className="agent-setup-card"><section className="agent-setup" id="agt-01" aria-labelledby="agt-01-title">
    {section !== 'reading' && <>
    <div className="agent-setup-head"><span>AGT-01 / MODEL SERVICE</span><h3 id="agt-01-title">Agent 服务</h3></div>
    <p>填写支持 Chat Completions 的 API 地址与模型名称。提问时，所选资料的相关片段会发送到该服务。</p>
    <label>API 地址<input value={baseUrl} onChange={event => setBaseUrl(event.target.value)} spellCheck={false} placeholder="https://api.openai.com/v1" /></label>
    <label>模型名称<input value={model} onChange={event => setModel(event.target.value)} spellCheck={false} placeholder={baseUrl.includes('api.deepseek.com') ? 'deepseek-flash' : '填写服务商提供的模型 ID'} /></label>
    {baseUrl.includes('api.deepseek.com') && <p className="agent-setup-note">DeepSeek V4.1 Flash 的 API 模型 ID 是 <code>deepseek-flash</code>，不要填写版本展示名。</p>}
    <label>API 密钥<input type="password" value={apiKey} onChange={event => setApiKey(event.target.value)} autoComplete="off" placeholder={hasKey ? '已保存；留空则保持原密钥' : '粘贴密钥'} /></label>
    <p className="agent-setup-note">{keyPersistent ? '密钥由当前 Windows 账户加密保存在本机，重启后自动读取；输入框不会显示原文。' : '当前平台的密钥只在本次运行期间保留。'}</p>
    {keyWarning && <p className="agent-setup-note" role="alert">{keyWarning}</p>}
    <div className="agent-setup-actions"><HudButton type="button" variant="primary" disabled={!isTauri() || saving || !baseUrl.trim() || !model.trim()} onClick={() => { void save() }}>{saving ? '保存中' : '保存配置'}</HudButton><span role="status">{message || (hasKey ? '密钥已设置' : '密钥未设置')}</span></div>
    </>}
    {section !== 'connection' && <>
    <div className="agent-setup-head"><span>READING / RETRIEVAL</span><h3>阅读与检索</h3></div>
    <p>控制资料探索、自动阅读、语义检索和发送给模型的上下文。数值预算仍可按需展开。</p>
    <div className="agent-explore-config">
      <label><input type="checkbox" checked={exploreEnabled} disabled={!isTauri() || saving || savingExplore} onChange={event => { void changeExplore(event.target.checked) }} /><span>默认开启资料探索</span></label>
      <p>有本轮参考资料的文字提问会允许模型搜索、读取其中的原文，调用次数和读取量由下面的阅读预算决定；可能增加等待时间和 API 用量。图片提问与指定时间截帧不使用资料探索。</p>
      {exploreMessage && <span role="status">{exploreMessage}</span>}
    </div>
    <details className="agent-explore-config reading-budget-config">
      <summary>自动阅读策略与任务预算</summary>
      <label>阅读策略<select value={budget.strategy} disabled={savingBudget} onChange={event => setBudget(current => ({ ...current, strategy: event.target.value as ReadingBudget['strategy'] }))}><option value="adaptive">自动按需阅读</option><option value="full">完整阅读</option></select></label>
      <p>按需模式在信息够用时回答；完整模式顺序处理选中的资料，达到任务预算时明确说明未读完。可以随时停止读取并回答。</p>
      <label>整项任务预算<select value={budget.effort} disabled={savingBudget} onChange={event => setBudget(current => readingPreset(current, event.target.value as ReadingBudget['effort']))}><option value="standard">标准</option><option value="thorough">充分</option><option value="custom">自定义</option></select></label>
      <p>标准：最多24次原文工具操作、12次阅读模型请求、180秒；充分：48次操作、24次请求、600秒。自动连续推进，每次提问计一份预算。更多阅读会增加等待和 API 用量；字符数不是 token 数。</p>
      {([
        ['toolCalls', '整项最多工具操作', 2, 48], ['explorationRounds', '整项最多阅读模型请求', 1, 32],
        ['toolChars', '每次工具输出字符', 4000, 32000], ['deepSeconds', '整项阅读时间秒数', 30, 900],
        ['deepBatches', '字幕最多批数（每批约 3000 字）', 4, 128], ['requestSeconds', '单次模型请求超时秒数', 30, 900],
        ['answerChars', '单次回答保留字符上限', 2000, 100000],
      ] as const).map(([key, label, min, max]) => <label key={key}>{label}<input type="number" min={min} max={max} step={1} value={budget[key]} disabled={savingBudget} onChange={event => setBudget(current => ({ ...current, effort: 'custom', [key]: Number(event.target.value) }))} /><small>{min}–{max}</small></label>)}
      <div className="agent-setup-actions"><HudButton type="button" disabled={!isTauri() || savingBudget} onClick={() => {
        setSavingBudget(true); setBudgetMessage('')
        void saveReadingBudget(budget).then(status => { setBudget(status.readingBudget); onStatusChange(status); setBudgetMessage('阅读预算已保存') })
          .catch(error => setBudgetMessage(String(error))).finally(() => setSavingBudget(false))
      }}>{savingBudget ? '保存中…' : '保存阅读预算'}</HudButton><HudButton type="button" disabled={savingBudget} onClick={() => { setBudget(defaultReadingBudget); setBudgetMessage('已填入默认值，点击保存后生效') }}>填入默认值</HudButton></div>
      <p role="status">{budgetMessage}</p>
    </details>
    <div className="agent-explore-config">
      <label>上下文档位<select value={contextTier} disabled={!isTauri() || saving || savingTier} onChange={event => { void changeTier(event.target.value as 'small' | 'medium') }}><option value="small">小 · 24000 字</option><option value="medium">中 · 48000 字</option></select></label>
      <p>这是发送文本的字符预算，不是模型 token 数。中档只扩展总预算，单段资料仍最多 1200 字、每轮最多 8 段。</p>
      {tierMessage && <span role="status">{tierMessage}</span>}
    </div>
    <div className="agent-explore-config">
      <label><input type="checkbox" checked={semanticEnabled} disabled={!isTauri() || savingSemantic} onChange={event => setSemanticEnabled(event.target.checked)} /><span>语义检索（默认关闭）</span></label>
      <p>开启后会把本轮问题及已选资料片段发送到向量模型，增加 API 调用和费用。用于文字提问中的已选资料；深度视频、附图和未选资料不参与。若上方问答服务不提供 /embeddings，请填写独立向量服务。服务不兼容或请求失败时自动使用原有词法检索。</p>
      {(semanticEnabled || !!embeddingModel) && <>
      <label className="semantic-text-field">向量模型 ID（开启时必填）<input value={embeddingModel} onChange={event => setEmbeddingModel(event.target.value)} spellCheck={false} placeholder="填写支持 /embeddings 的模型 ID" /></label>
      <label className="semantic-text-field">向量 API 地址（可选）<input value={embeddingBaseUrl} onChange={event => setEmbeddingBaseUrl(event.target.value)} spellCheck={false} placeholder="留空则沿用上方 API 地址" /></label>
      <label className="semantic-text-field">向量 API 密钥（独立地址时填写）<input type="password" value={embeddingKey} onChange={event => setEmbeddingKey(event.target.value)} autoComplete="off" placeholder={embeddingHasKey ? '已保存；留空则保持' : '留空则仅在共用地址时沿用上方密钥'} /></label>
      <div className="agent-setup-actions"><HudButton type="button" disabled={!isTauri() || savingSemantic || (semanticEnabled && !embeddingModel.trim())} onClick={() => { void saveSemantic() }}>{savingSemantic ? '保存中' : '保存语义检索'}</HudButton><HudButton type="button" disabled={!isTauri() || savingSemantic || semanticProbeBusy || !embeddingModel.trim()} onClick={() => { void testSemantic() }}>{semanticProbeBusy ? '测试中' : '测试已保存的向量服务'}</HudButton><span role="status">{semanticMessage}</span></div>
      {semanticProbe && <p role="status">{semanticProbe.supported ? `向量服务可用 · ${semanticProbe.dimensions} 维 · ${semanticProbe.cases} 题中首位命中 ${semanticProbe.topOne} 题、前三命中 ${semanticProbe.recallThree} 题 · ${(semanticProbe.elapsedMs / 1000).toFixed(1)} 秒` : '向量服务不可用'}。{semanticProbe.reason}</p>}
      </>}
    </div>
    </>}
    {section !== 'reading' && <div className="agent-tool-probe">
      <div><strong>资料探索兼容性</strong><HudButton type="button" disabled={!isTauri() || saving || testing || !hasKey || !savedConfig.model || baseUrl !== savedConfig.baseUrl || model !== savedConfig.model} onClick={() => { void testTools() }}>{testing ? '测试中' : '测试已保存配置'}</HudButton></div>
      <p>点击后会向已保存的模型服务发送一段虚构测试资料，可能产生少量 API 用量；不会读取真实资料或写入会话。修改配置后请先保存。</p>
      {probe && <div className="agent-tool-probe-result" role="status"><strong>{probe.searched && probe.read ? '搜索与读取均通过' : probe.supported ? '只确认了部分工具调用' : '未检测到工具调用'}</strong><span>耗时 {(probe.elapsedMs / 1000).toFixed(1)} 秒</span>{probe.reason && <p>{probe.reason}</p>}{probe.steps.length > 0 && <ol>{probe.steps.map((step, index) => <li key={index}>{step}</li>)}</ol>}</div>}
      {probeError && <p role="alert">{probeError}</p>}
    </div>}
  </section></ChamferCard>
}
