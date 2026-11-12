import { useEffect, useState } from 'react'
import type { KnowledgeRecord, StoryboardResult } from '../data'
import { analyzeVideoStoryboard, getStoryboardImage, openVideoAt } from '../records'
import { HudButton } from './HudButton'
import './storyboard.css'

type Props = { record: KnowledgeRecord; onChanged: () => Promise<void> }

function stamp(seconds: number) {
  const total = Math.floor(seconds)
  return `${String(Math.floor(total / 60)).padStart(2, '0')}:${String(total % 60).padStart(2, '0')}`
}

export function StoryboardSection({ record, onChanged }: Props) {
  const [result, setResult] = useState<StoryboardResult | undefined>(record.storyboard)
  const [images, setImages] = useState<string[]>([])
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [cookiesFile, setCookiesFile] = useState<File | null>(null)
  const isBilibili = record.sourceUrl?.includes('bilibili.com') ?? false
  const needsLogin = isBilibili && (error.includes('HTTP 567') || cookiesFile !== null)

  useEffect(() => {
    setResult(record.storyboard)
  }, [record.storyboard])

  useEffect(() => {
    if (!result) { setImages([]); return }
    let active = true
    void Promise.all(Array.from({ length: result.imageCount }, (_, index) => getStoryboardImage(record.id, index)))
      .then(loaded => { if (active) setImages(loaded) })
      .catch(() => { if (active) setError('分镜图片读取失败。') })
    return () => { active = false }
  }, [record.id, result])

  const analyze = async (withCookies = false) => {
    setBusy(true)
    if (!withCookies) setError('')
    try {
      if (withCookies && !cookiesFile) throw new Error('请先选择 cookies.txt。')
      if (withCookies && cookiesFile!.size > 512_000) throw new Error('cookies.txt 不能超过 500 KB。')
      const cookiesText = withCookies ? await cookiesFile!.text() : undefined
      const next = await analyzeVideoStoryboard(record.id, cookiesText)
      setResult(next)
      setCookiesFile(null)
      await onChanged()
    } catch (failure) {
      setError(String(failure))
    } finally {
      setBusy(false)
    }
  }

  const openAt = async (seconds: number) => {
    setError('')
    try { await openVideoAt(record.id, Math.floor(seconds)) }
    catch (failure) { setError(String(failure)) }
  }

  return <section className="record-section storyboard-section" aria-label="视频画面分析">
    <h3>STORYBOARD / 画面分镜</h3>
    <p className="storyboard-note">按镜头提取代表画面。首次分析需要 Node.js、FFmpeg 和 yt-dlp；只处理可访问的视频。</p>
    {!result && <HudButton type="button" disabled={busy || record.demo} onClick={() => { void analyze() }}>{busy ? '正在分析画面…' : '分析画面'}</HudButton>}
    {error && <p className="storyboard-error" role="alert">{error}</p>}
    {!result && needsLogin && <div className="storyboard-auth">
      <p>此视频需要 B 站登录态。在浏览器登录 B 站后，导出仅含 B 站域名的 Netscape 格式 cookies.txt；它包含登录凭据，请勿分享。</p>
      <label>选择 cookies.txt <input type="file" accept=".txt,text/plain" disabled={busy} onChange={event => setCookiesFile(event.target.files?.[0] ?? null)} /></label>
      <HudButton type="button" disabled={busy || !cookiesFile} onClick={() => { void analyze(true) }}>用所选文件重试</HudButton>
      <p>文件内容只用于本次取流；GearNote 不长期保存，临时副本在分析结束后删除。</p>
    </div>}
    {result && <>
      <p className="storyboard-note">{result.imageCount} 张分镜图 · {result.cells.length} 个时间段</p>
      {images.map((src, index) => <div className="storyboard-image" key={index}>
        <span>FRAME SHEET / {String(index + 1).padStart(2, '0')}</span>
        <img src={src} alt={`第 ${index + 1} 张视频分镜图`} />
      </div>)}
      <div className="storyboard-times" aria-label="分镜时间点">
        {result.cells.map(cell => <button key={`${cell.sheet}-${cell.cell}`} type="button" onClick={() => { void openAt(cell.frameAt ?? cell.from) }} title="在浏览器中打开原视频的对应时间点">
          <span>{String(cell.cell).padStart(2, '0')}</span> {stamp(cell.from)}–{stamp(cell.to)} ↗
        </button>)}
      </div>
    </>}
  </section>
}
