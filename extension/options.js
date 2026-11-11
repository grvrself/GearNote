const form = document.getElementById('pair-form')
const input = document.getElementById('pair-code')
const result = document.getElementById('result')

chrome.storage.local.get('pairCode').then(({ pairCode }) => {
  if (pairCode) input.value = pairCode
})

form.addEventListener('submit', async event => {
  event.preventDefault()
  const pairCode = input.value.trim()
  if (!/^[a-f0-9]{32}$/i.test(pairCode)) {
    result.textContent = '请输入桌面程序显示的 32 位配对码。'
    result.dataset.state = 'error'
    return
  }
  result.textContent = '正在连接本机程序…'
  result.dataset.state = 'pending'
  try {
    const response = await fetch('http://127.0.0.1:45147/v1/status', {
      headers: { 'X-KnowFlow-Token': pairCode },
    })
    const data = await response.json()
    if (!response.ok) throw new Error(data.error || '连接失败')
    await chrome.storage.local.set({ pairCode })
    result.textContent = '连接成功。现在可以点击扩展图标保存网页文章。'
    result.dataset.state = 'success'
  } catch (error) {
    result.textContent = `连接失败：${error.message || '请先打开 KnowFlow 桌面程序'}`
    result.dataset.state = 'error'
  }
})
