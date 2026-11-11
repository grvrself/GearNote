// This function is injected into the current page. Keep it self-contained.
export function extractArticle() {
  const pageUrl = new URL(location.href)
  const biliOpus = /(^|\.)bilibili\.com$/i.test(pageUrl.hostname) && /^\/opus\/\d+\/?$/.test(pageUrl.pathname)
  const biliRead = /(^|\.)bilibili\.com$/i.test(pageUrl.hostname) && /^\/read\/cv\d+\/?$/.test(pageUrl.pathname)
  const title = document.title.trim()

  if (biliOpus || biliRead) {
    const content = biliOpus
      ? document.querySelector('.opus-module-content')
      : document.querySelector('#article-content, .article-content')
    if (!content) {
      return { error: 'B 站正文尚未加载，请等待文章出现后重新点击扩展图标' }
    }
    const text = (content.innerText || content.textContent || '')
      .replace(/\u00a0/g, ' ')
      .replace(/[ \t]+\n/g, '\n')
      .replace(/\n[ \t]+/g, '\n')
      .replace(/\n{3,}/g, '\n\n')
      .trim()
    if (text.length < 80) {
      return { error: 'B 站正文不足 80 字，请等待文章完整加载后重试' }
    }
    pageUrl.search = ''
    pageUrl.hash = ''
    if (biliOpus) pageUrl.pathname = `/opus/${pageUrl.pathname.split('/')[2]}`
    return { url: pageUrl.href, title, text: text.slice(0, 300000), capturedAt: new Date().toISOString() }
  }

  const candidates = [document.querySelector('article'), document.querySelector('main'), document.querySelector('[role="main"]'), document.body]
  let text = ''
  for (const candidate of candidates) {
    if (!candidate) continue
    const copy = candidate.cloneNode(true)
    copy.querySelectorAll('script,style,nav,footer,aside,form,button,iframe,svg,[aria-hidden="true"],.advertisement,.ads').forEach(node => node.remove())
    const blocks = [...copy.querySelectorAll('h1,h2,h3,h4,p,li,pre,blockquote')]
      .map(node => node.textContent.replace(/\s+/g, ' ').trim())
      .filter(Boolean)
    const candidateText = blocks.length ? blocks.join('\n\n') : copy.textContent.replace(/\s+/g, ' ').trim()
    if (candidateText.length > text.length) text = candidateText
    if (candidateText.length >= 300 && candidate !== document.body) break
  }
  return { url: pageUrl.href, title, text: text.slice(0, 300000), capturedAt: new Date().toISOString() }
}
