// This function is injected into the page's MAIN world. Keep all helpers inside it.
export async function discoverVideo() {
  const url = location.href
  const title = document.title.trim()
  const capturedAt = new Date().toISOString()
  const video = document.querySelector('video')
  const durationSeconds = Number.isFinite(video?.duration) && video.duration > 0
    ? Math.round(video.duration) : undefined
  const base = { url, title, capturedAt, durationSeconds }
  const host = location.hostname.toLowerCase()
  const preferredLanguage = navigator.language.toLowerCase()

  function chooseTrack(tracks, languageOf, isAuto) {
    const rank = track => {
      const language = String(languageOf(track) || '').toLowerCase()
      const preferred = preferredLanguage.split('-')[0]
      const languageScore = language.startsWith('zh') ? 0
        : language.startsWith(preferred) ? 1
        : language.startsWith('en') ? 2 : 3
      return languageScore * 10 + (isAuto(track) ? 1 : 0)
    }
    return [...tracks].sort((a, b) => rank(a) - rank(b))[0]
  }

  if (host === 'youtube.com' || host.endsWith('.youtube.com')) {
    const videoId = new URL(url).searchParams.get('v') || location.pathname.match(/^\/shorts\/([^/]+)/)?.[1]
    const player = document.querySelector('#movie_player')
    const response = player?.getPlayerResponse?.() || window.ytInitialPlayerResponse
    if (!response || (videoId && response.videoDetails?.videoId !== videoId)) {
      return { ...base, provider: 'youtube', error: '播放器字幕信息尚未加载，请稍后重试' }
    }
    const tracks = response.captions?.playerCaptionsTracklistRenderer?.captionTracks || []
    if (!tracks.length) return { ...base, provider: 'youtube', error: '该视频没有可读取的字幕轨道' }
    const track = chooseTrack(tracks, item => item.languageCode, item => item.kind === 'asr')
    const language = track.name?.simpleText || track.name?.runs?.map(run => run.text).join('') || track.languageCode || ''
    return {
      ...base,
      provider: 'youtube',
      durationSeconds: durationSeconds || Number(response.videoDetails?.lengthSeconds) || undefined,
      trackUrl: track.baseUrl,
      format: 'youtube',
      language,
    }
  }

  if (host === 'bilibili.com' || host.endsWith('.bilibili.com')) {
    const bvid = location.pathname.match(/\/video\/(BV[\w]+)/i)?.[1]
    const initial = window.__INITIAL_STATE__ || {}
    let details = initial.videoData || initial.videoInfo || {}
    const page = Math.max(1, Number(new URL(url).searchParams.get('p')) || 1)
    if (!bvid) return { ...base, provider: 'bilibili', error: '当前页面缺少视频 BV 号' }
    if (String(details.bvid || '').toUpperCase() !== bvid.toUpperCase() || !details.pages?.[page - 1]?.cid) {
      try {
        const viewResponse = await fetch(`https://api.bilibili.com/x/web-interface/view?bvid=${encodeURIComponent(bvid)}`, { credentials: 'include' })
        const view = await viewResponse.json()
        if (!viewResponse.ok || view.code !== 0 || String(view.data?.bvid || '').toUpperCase() !== bvid.toUpperCase()) {
          throw new Error('视频资料不匹配')
        }
        details = view.data
      } catch {
        return { ...base, provider: 'bilibili', error: '无法核对当前视频的 BV 号与分 P，请刷新后重试' }
      }
    }
    const cid = Number(details.pages?.[page - 1]?.cid)
    if (!Number.isSafeInteger(cid) || cid <= 0) {
      return { ...base, provider: 'bilibili', error: '未找到当前分 P 的 CID，请刷新页面后重试' }
    }
    let payload
    try {
      const endpoint = `https://api.bilibili.com/x/player/wbi/v2?bvid=${encodeURIComponent(bvid)}&cid=${cid}`
      const response = await fetch(endpoint, { credentials: 'include' })
      if (!response.ok) throw new Error('播放器接口不可用')
      payload = await response.json()
    } catch {
      return { ...base, provider: 'bilibili', error: '无法读取可信的 Bilibili 字幕信息，请确认已登录后重试' }
    }
    if (payload?.code !== 0) {
      return { ...base, provider: 'bilibili', error: 'Bilibili 字幕接口暂不可用，请确认已登录并刷新视频后重试' }
    }
    if (String(payload.data?.bvid || '').toUpperCase() !== bvid.toUpperCase() || Number(payload.data?.cid) !== cid) {
      return { ...base, provider: 'bilibili', error: '播放器返回的视频身份不匹配，已拒绝保存字幕' }
    }
    const tracks = payload?.data?.subtitle?.subtitles || []
    if (!tracks.length) {
      return { ...base, provider: 'bilibili', error: payload?.data?.need_login_subtitle
        ? '请先登录 Bilibili，再重试字幕捕获' : '该视频没有可读取的字幕轨道' }
    }
    const verifiedTracks = tracks.filter(track => {
      if (!track.subtitle_url) return false
      try {
        const subtitleUrl = new URL(track.subtitle_url, location.href)
        const subtitleHost = subtitleUrl.hostname.toLowerCase()
        return subtitleUrl.protocol === 'https:'
          && (subtitleHost === 'hdslb.com' || subtitleHost.endsWith('.hdslb.com'))
          && subtitleUrl.pathname.includes(String(cid))
      } catch {
        return false
      }
    })
    if (!verifiedTracks.length) {
      return { ...base, provider: 'bilibili', error: '字幕轨道未包含当前分 P 的 CID，已拒绝保存不匹配的字幕' }
    }
    const track = chooseTrack(verifiedTracks, item => item.lan, item => Number(item.ai_type) > 0)
    return {
      ...base,
      provider: 'bilibili',
      bvid,
      cid,
      durationSeconds: durationSeconds || Number(details.pages?.[page - 1]?.duration || details.duration) || undefined,
      trackUrl: new URL(track.subtitle_url, location.href).href,
      format: 'bilibili',
      language: track.lan_doc || track.lan || '',
    }
  }

  const textTracks = [...(video?.textTracks || [])]
  const loaded = chooseTrack(textTracks.filter(track => track.cues?.length), track => track.language, () => false)
  if (loaded) {
    const cues = [...loaded.cues].map(cue => ({ atSeconds: Math.round(cue.startTime), text: cue.text || '' }))
    return { ...base, provider: 'html5', language: loaded.label || loaded.language || '', cues }
  }
  const elements = [...document.querySelectorAll('video track[src]')]
  const track = chooseTrack(elements, item => item.srclang, () => false)
  if (track) {
    return { ...base, provider: 'html5', language: track.label || track.srclang || '', trackUrl: track.src, format: 'vtt' }
  }
  return { ...base, provider: 'html5', error: '当前视频未提供可读取的字幕轨道' }
}

function normalizeCues(cues) {
  if (cues.length > 5000) throw new Error('字幕超过 5000 条，暂不能完整保存')
  return cues.map(cue => ({
    atSeconds: Math.max(0, Math.round(Number(cue.atSeconds) || 0)),
    text: String(cue.text || '').replace(/\s+/g, ' ').trim(),
  })).filter(cue => cue.text)
}

function parseYouTubeJson(data) {
  return normalizeCues((data.events || []).map(event => ({
    atSeconds: Number(event.tStartMs || 0) / 1000,
    text: (event.segs || []).map(segment => segment.utf8 || '').join(''),
  })))
}

function parseBilibiliJson(data) {
  return normalizeCues((data.body || []).map(cue => ({ atSeconds: cue.from, text: cue.content })))
}

function parseVtt(text) {
  const cues = []
  const blocks = text.replace(/\r/g, '').split(/\n\s*\n/)
  for (const block of blocks) {
    const lines = block.split('\n')
    const stampIndex = lines.findIndex(line => line.includes('-->'))
    if (stampIndex < 0) continue
    const start = lines[stampIndex].split('-->')[0].trim()
    const parts = start.split(':').map(Number)
    if (parts.some(part => !Number.isFinite(part))) continue
    const seconds = parts.reduce((value, part) => value * 60 + part, 0)
    const cueText = lines.slice(stampIndex + 1).join(' ').replace(/<[^>]*>/g, ' ').trim()
    cues.push({ atSeconds: seconds, text: cueText })
  }
  return normalizeCues(cues)
}

async function getCaptionResponse(url, tabId) {
  try {
    const response = await fetch(url)
    if (!response.ok) throw new Error(`字幕请求失败 (${response.status})`)
    return response.text()
  } catch {
    const [injected] = await chrome.scripting.executeScript({
      target: { tabId }, world: 'MAIN',
      func: async captionUrl => {
        const response = await fetch(captionUrl, { credentials: 'include' })
        if (!response.ok) throw new Error(`字幕请求失败 (${response.status})`)
        return response.text()
      },
      args: [url],
    })
    return injected?.result || ''
  }
}

export async function loadVideoCues(discovered, tabId) {
  if (discovered.cues) return normalizeCues(discovered.cues)
  if (!discovered.trackUrl) return []
  if (discovered.format === 'youtube') {
    const url = new URL(discovered.trackUrl)
    url.searchParams.set('fmt', 'json3')
    try {
      const body = await getCaptionResponse(url.href, tabId)
      const cues = parseYouTubeJson(JSON.parse(body))
      if (cues.length) return cues
    } catch {
      // Some caption tracks only provide WebVTT.
    }
    url.searchParams.set('fmt', 'vtt')
    return parseVtt(await getCaptionResponse(url.href, tabId))
  }
  const body = await getCaptionResponse(discovered.trackUrl, tabId)
  if (discovered.format === 'bilibili') return parseBilibiliJson(JSON.parse(body))
  return parseVtt(body)
}
