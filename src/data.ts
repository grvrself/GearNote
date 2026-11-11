export type ProcessStatus = 'extracting' | 'summarizing' | 'ready' | 'fault'
export type RecordType = 'video' | 'article'
export type TranscriptCue = { atSeconds: number; text: string }
export type KeyMoment = { atSeconds: number; title: string }
export type StoryboardCell = { cell: number; sheet: number; from: number; to: number; frameAt?: number | null }
export type StoryboardResult = { imageCount: number; cells: StoryboardCell[] }

export type KnowledgeRecord = {
  id: string
  type: RecordType
  title: string
  site: string
  savedAt: string // ISO 8601 timestamp
  status: ProcessStatus
  progress?: number
  durationSeconds?: number
  faultReason?: string
  transcript?: TranscriptCue[]
  excerpt?: string[]
  moments?: KeyMoment[]
  sourceUrl?: string
  contentText?: string
  captionLanguage?: string
  storyboard?: StoryboardResult
  demo?: boolean
}

// Fixture data keeps the same shape as records returned by the future storage layer.
export const recordFixtures: KnowledgeRecord[] = [
  {
    id: 'rec-transformer-001', type: 'video', title: 'Transformer 逐行解读：注意力机制到底在算什么',
    site: 'youtube.com', savedAt: '2026-10-31T10:42:00+08:00', status: 'extracting', progress: 36, durationSeconds: 1104,
  },
  {
    id: 'rec-attention-002', type: 'video', title: '10 分钟看懂注意力机制',
    site: 'bilibili.com', savedAt: '2026-10-31T10:26:00+08:00', status: 'summarizing', progress: 68, durationSeconds: 607,
    transcript: [{ atSeconds: 134, text: '注意力权重由查询与键的相似度计算，再作用于值。' }],
  },
  {
    id: 'rec-cs50-003', type: 'video', title: 'CS50 第 4 讲：数据结构 / 数组、哈希表、Trie',
    site: 'youtube.com', savedAt: '2026-10-31T09:45:00+08:00', status: 'ready', durationSeconds: 3138,
    transcript: [
      { atSeconds: 492, text: '数组把元素放在连续内存里，查询较快，但插入需要移动元素。' },
      { atSeconds: 1300, text: '哈希表以空间换时间，理想情况下查找也是常数级。' },
      { atSeconds: 2225, text: 'Trie 按字符分层，适合前缀检索。' },
    ],
    moments: [
      { atSeconds: 0, title: '开场' }, { atSeconds: 492, title: '数组与链表' },
      { atSeconds: 1300, title: '哈希表' }, { atSeconds: 2225, title: 'Trie' },
      { atSeconds: 2910, title: '总结' },
    ],
  },
  {
    id: 'rec-mdn-004', type: 'article', title: 'MDN：JavaScript 闭包',
    site: 'developer.mozilla.org', savedAt: '2026-10-30T21:18:00+08:00', status: 'ready',
    excerpt: ['闭包让函数访问其外部词法作用域中的变量。'],
  },
  {
    id: 'rec-notes-005', type: 'article', title: '如何建立自己的学习笔记系统',
    site: 'example.com', savedAt: '2026-10-29T18:03:00+08:00', status: 'fault', faultReason: '页面正文不可读取',
  },
]

export const statusText: Record<ProcessStatus, string> = {
  extracting: '正在提取字幕', summarizing: '正在生成摘要', ready: '可提问', fault: '提取失败',
}
