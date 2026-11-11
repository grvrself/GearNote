import assert from 'node:assert/strict'
import test from 'node:test'
import { discoverVideo } from './video-extractor.js'

async function withBilibiliPage(playerData, run) {
  const names = ['location', 'document', 'window', 'navigator', 'fetch']
  const previous = Object.fromEntries(names.map(name => [name, Object.getOwnPropertyDescriptor(globalThis, name)]))
  const requests = []
  const values = {
    location: { href: 'https://www.bilibili.com/video/BV1fLpg6HE8E/', hostname: 'www.bilibili.com', pathname: '/video/BV1fLpg6HE8E/' },
    document: { title: 'Rust Tauri2 教程', querySelector: () => null },
    window: { __INITIAL_STATE__: { videoData: { bvid: 'BV1fLpg6HE8E', pages: [{ cid: 12345, duration: 261 }] } } },
    navigator: { language: 'zh-CN' },
    fetch: async url => {
      requests.push(String(url))
      return { ok: true, json: async () => playerData }
    },
  }
  for (const [name, value] of Object.entries(values)) {
    Object.defineProperty(globalThis, name, { configurable: true, writable: true, value })
  }
  try { return await run(requests) }
  finally {
    for (const name of names) {
      if (previous[name]) Object.defineProperty(globalThis, name, previous[name])
      else delete globalThis[name]
    }
  }
}

function response(subtitleUrl, cid = 12345) {
  return {
    code: 0,
    data: {
      bvid: 'BV1fLpg6HE8E', cid,
      subtitle: { subtitles: [{ lan: 'ai-zh', lan_doc: '中文', ai_type: 1, subtitle_url: subtitleUrl }] },
    },
  }
}

test('rejects a subtitle URL belonging to another CID', async () => {
  await withBilibiliPage(response('//aisubtitle.hdslb.com/bfs/subtitle/99999/other.json'), async requests => {
    const result = await discoverVideo()
    assert.match(result.error, /CID/)
    assert.equal(result.trackUrl, undefined)
    assert.equal(requests.length, 1)
    assert.match(requests[0], /\/x\/player\/wbi\/v2\?/)
  })
})

test('accepts a WBI subtitle URL bound to the current CID', async () => {
  await withBilibiliPage(response('//aisubtitle.hdslb.com/bfs/subtitle/12345/caption.json'), async () => {
    const result = await discoverVideo()
    assert.equal(result.error, undefined)
    assert.equal(result.bvid, 'BV1fLpg6HE8E')
    assert.equal(result.cid, 12345)
    assert.match(result.trackUrl, /12345/)
  })
})

test('rejects metadata that echoes a different CID', async () => {
  await withBilibiliPage(response('//aisubtitle.hdslb.com/bfs/subtitle/12345/caption.json', 67890), async () => {
    const result = await discoverVideo()
    assert.match(result.error, /身份不匹配/)
    assert.equal(result.trackUrl, undefined)
  })
})
