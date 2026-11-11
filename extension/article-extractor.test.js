import test from 'node:test'
import assert from 'node:assert/strict'
import { extractArticle } from './article-extractor.js'

function inPage(url, content, run) {
  const oldLocation = globalThis.location
  const oldDocument = globalThis.document
  globalThis.location = { href: url }
  globalThis.document = {
    title: '测试专栏 - 哔哩哔哩',
    body: { innerText: '会员提示 推荐内容 经验值+10' },
    querySelector: selector => selector === '.opus-module-content' ? content : null,
  }
  try { run() } finally {
    globalThis.location = oldLocation
    globalThis.document = oldDocument
  }
}

test('B 站专栏只读取正文容器并去除跟踪参数', () => {
  const body = '第一段：这是作者写的正文。'.repeat(8)
  inPage('https://www.bilibili.com/opus/396287239310805041?from=search&spm_id_from=333', { innerText: body }, () => {
    const result = extractArticle()
    assert.equal(result.url, 'https://www.bilibili.com/opus/396287239310805041')
    assert.equal(result.text, body)
    assert.ok(!result.text.includes('会员提示'))
    assert.ok(!result.text.includes('经验值+10'))
  })
})

test('正文未加载时明确失败，不把整页当作文章', () => {
  inPage('https://www.bilibili.com/opus/396287239310805041', null, () => {
    const result = extractArticle()
    assert.match(result.error, /正文尚未加载/)
    assert.equal(result.text, undefined)
  })
})
