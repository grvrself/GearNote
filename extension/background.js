import { discoverVideo, loadVideoCues } from './video-extractor.js'
import { extractArticle } from './article-extractor.js'

const BRIDGE = 'http://127.0.0.1:45147'

async function bridgeRequest(path, token, options = {}) {
  const response = await fetch(`${BRIDGE}${path}`, {
    ...options,
    headers: { 'X-KnowFlow-Token': token, ...(options.headers || {}) },
  })
  const result = await response.json()
  if (!response.ok) throw new Error(result.error || `本机连接失败 (${response.status})`)
  return result
}

async function heartbeat() {
  const { pairCode } = await chrome.storage.local.get('pairCode')
  if (!pairCode) return
  try {
    await bridgeRequest('/v1/status', pairCode)
  } catch {
    // KnowFlow may be closed; the next alarm will reconnect.
  }
}

async function ensureHeartbeat() {
  await chrome.alarms.create('knowflow-heartbeat', { periodInMinutes: 0.5 })
  await heartbeat()
}

chrome.runtime.onInstalled.addListener(() => { void ensureHeartbeat() })
chrome.runtime.onStartup.addListener(() => { void ensureHeartbeat() })
chrome.alarms.onAlarm.addListener(alarm => {
  if (alarm.name === 'knowflow-heartbeat') void heartbeat()
})

async function showResult(tabId, ok, title) {
  try {
    await chrome.action.setBadgeBackgroundColor({ tabId, color: ok ? '#FFD400' : '#D64541' })
    await chrome.action.setBadgeText({ tabId, text: ok ? 'OK' : 'ERR' })
    await chrome.action.setTitle({ tabId, title })
    setTimeout(() => { void chrome.action.setBadgeText({ tabId, text: '' }).catch(() => {}) }, 6000)
  } catch {
    // The tab may have closed before capture finished.
  }
}

function isKnownVideoPage(url) {
  return /^https?:\/\/(www\.|m\.)?(youtube\.com\/(watch|shorts\/)|bilibili\.com\/video\/)/i.test(url)
}

async function captureVideo(tabId, pairCode) {
  const [injected] = await chrome.scripting.executeScript({ target: { tabId }, world: 'MAIN', func: discoverVideo })
  const discovered = injected?.result
  if (!discovered?.url) throw new Error('无法读取视频页面')
  let cues = []
  let faultReason = discovered.error || ''
  if (!faultReason) {
    try {
      cues = await loadVideoCues(discovered, tabId)
      if (!cues.length) faultReason = '字幕轨道未返回可读取的内容'
    } catch (error) {
      faultReason = `字幕读取失败：${error.message || '请稍后重试'}`
    }
  }
  const result = await bridgeRequest('/v1/videos', pairCode, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      url: discovered.url,
      title: discovered.title,
      capturedAt: discovered.capturedAt,
      durationSeconds: discovered.durationSeconds,
      language: discovered.language || '',
      bvid: discovered.bvid,
      cid: discovered.cid,
      trackUrl: discovered.trackUrl,
      cues,
      faultReason,
    }),
  })
  return { result, faultReason }
}

chrome.action.onClicked.addListener(async tab => {
  const tabId = tab.id
  if (tabId == null) return
  const { pairCode } = await chrome.storage.local.get('pairCode')
  if (!pairCode) {
    await chrome.runtime.openOptionsPage()
    return
  }
  if (!tab.url || !/^https?:\/\//i.test(tab.url)) {
    await showResult(tabId, false, '当前页面不能捕获，请打开普通网页文章')
    return
  }
  try {
    let videoPage = isKnownVideoPage(tab.url)
    if (!videoPage) {
      const [detected] = await chrome.scripting.executeScript({ target: { tabId }, func: () =>
        Boolean(document.querySelector('video track[src]') || [...document.querySelectorAll('video')].some(video => [...video.textTracks].some(track => track.cues?.length))) })
      videoPage = Boolean(detected?.result)
    }
    if (videoPage) {
      const { result, faultReason } = await captureVideo(tabId, pairCode)
      await showResult(tabId, !faultReason, faultReason
        ? `字幕捕获失败：${faultReason}。可在该页面再次点击扩展图标重试。`
        : result.alreadySaved ? '该视频已保存到 KnowFlow' : '视频字幕已保存到 KnowFlow')
      return
    }
    const injected = await chrome.scripting.executeScript({ target: { tabId }, func: extractArticle })
    const article = injected[0]?.result
    if (article?.error) throw new Error(article.error)
    if (!article || article.text.length < 80) throw new Error('未识别到足够的文章正文')
    const result = await bridgeRequest('/v1/articles', pairCode, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(article),
    })
    await showResult(tabId, true, result.alreadySaved ? '这篇文章已经保存在 KnowFlow' : result.updated ? '文章正文已更新到 KnowFlow' : '文章已保存到 KnowFlow')
  } catch (error) {
    await showResult(tabId, false, `捕获失败：${error.message || '请检查 KnowFlow 是否已打开'}`)
  }
})
