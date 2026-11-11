import { useState } from 'react'
import type { BridgeStatus } from '../records'
import { ChamferCard } from './ChamferCard'
import { HudButton } from './HudButton'
import './extension-setup.css'

type Props = { status: BridgeStatus | null }

export function ExtensionSetup({ status }: Props) {
  const [copyMessage, setCopyMessage] = useState('')
  const copyCode = async () => {
    if (!status?.pairCode) return
    try {
      await navigator.clipboard.writeText(status.pairCode)
      setCopyMessage('配对码已复制')
    } catch {
      setCopyMessage('无法自动复制，请选中配对码手动复制')
    }
  }

  return <ChamferCard className="extension-setup-card"><section className="extension-setup" id="ext-01" aria-labelledby="ext-01-title">
    <div className="extension-setup-head"><span>EXT-01 / BROWSER BRIDGE</span><h3 id="ext-01-title">网页与视频字幕捕获</h3></div>
    <p>点击浏览器工具栏图标后，扩展会保存当前文章；在带字幕的视频页面，则保存字幕与时间点。资料只写入这台电脑。</p>
    <div className="extension-setup-state" role="status">连接状态：<strong>{status?.error ? '本机连接异常' : status?.connected ? '扩展已连接' : '扩展未连接'}</strong></div>
    {status?.error && <p className="extension-setup-error">{status.error}</p>}
    <div className="extension-setup-pair">
      <span>PAIR CODE / 本机配对码</span>
      <code>{status?.pairCode ?? '请在 Tauri 桌面程序中查看'}</code>
      <HudButton type="button" disabled={!status?.pairCode} onClick={() => { void copyCode() }}>复制配对码</HudButton>
      {copyMessage && <small role="status">{copyMessage}</small>}
    </div>
    <ol className="extension-setup-steps">
      <li>在 Chrome 打开 <code>chrome://extensions</code>；在 Edge 打开 <code>edge://extensions</code>。</li>
      <li>开启「开发者模式」，选择「加载已解压的扩展」，选中本项目的 <code>extension</code> 文件夹。</li>
      <li>打开扩展详情的「扩展程序选项」，粘贴上方配对码并连接。</li>
      <li>打开普通网页文章，或打开带字幕的 YouTube / Bilibili 视频，点击浏览器工具栏的 KnowFlow 扩展图标。</li>
    </ol>
    <p className="extension-setup-note">视频必须有播放器提供的字幕轨道；无字幕的视频会显示失败原因。Bilibili 的部分字幕需要先登录。失败后在视频页再次点击扩展图标即可重试。</p>
    <p className="extension-setup-note">旧版抓取的 Bilibili 字幕未核对视频身份。新版会移除这些字幕并标记为待重新捕获；在原视频页再次点击扩展图标即可更新。</p>
    <p className="extension-setup-note">B 站专栏正文有误时，先在扩展管理页重新加载扩展，再回到文章页等待正文出现，点击扩展图标。新版会更新原记录并保留引用 ID。</p>
  </section></ChamferCard>
}
