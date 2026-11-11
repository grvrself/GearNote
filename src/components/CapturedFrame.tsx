import { useEffect, useState } from 'react'
import { getChatFrameImage } from '../agent'
import type { ChatFrame } from '../chat-data'
import { HudButton } from './HudButton'

type Props = { frame: ChatFrame; title: string; selected?: boolean; onAttach?: () => void }

export function CapturedFrame({ frame, title, selected = false, onAttach }: Props) {
  const [source, setSource] = useState('')
  const [error, setError] = useState('')

  useEffect(() => {
    let active = true
    setSource('')
    setError('')
    void getChatFrameImage(frame.imageId)
      .then(image => { if (active) setSource(image) })
      .catch(() => { if (active) setError('截图文件无法读取') })
    return () => { active = false }
  }, [frame.imageId])

  return <div className="captured-frame">
    <span>FRAME / {String(Math.floor(frame.atSeconds / 60)).padStart(2, '0')}:{String(frame.atSeconds % 60).padStart(2, '0')}</span>
    {source ? <img src={source} alt={`《${title}》在该时间点截取的视频画面`} /> : <p role={error ? 'alert' : 'status'}>{error || '正在读取截图…'}</p>}
    {source && onAttach && <HudButton type="button" disabled={selected} onClick={onAttach}>{selected ? '已加入提问' : '加入提问'}</HudButton>}
  </div>
}
