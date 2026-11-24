export type ChatCitation = {
  recordId: string
  atSeconds?: number
  sourceIndex?: number
  label?: string
  kind?: 'image' | 'chat'
}

export type ChatFrame = { imageId: string; recordId: string; atSeconds: number }

export type MessageKind = 'answer' | 'frame' | 'choice' | 'library_proposal' | 'note_proposal' | 'command_proposal' | 'command_result'
export type AnswerStatus = 'completed' | 'partial' | 'failed' | 'not_applicable'
export type AnswerEndReason = 'normal' | 'stream_interrupted' | 'output_limit' | 'finish_reason' | 'provider_error' | 'request_failed' | 'empty_answer' | 'reading_failed'
export type ReadingEndReason = 'complete' | 'enough' | 'user_stopped' | 'budget_exhausted' | 'read_failed'
export type MessageNotice = { code: string; message: string }
export type MessageOutcome = { kind: MessageKind; answer: AnswerStatus; answerReason?: AnswerEndReason; readingReason?: ReadingEndReason; notices?: MessageNotice[] }

export type ChatRun = { elapsedMs: number; changedFiles: string[]; readSources?: { sourceIndex: number; recordId: string; title: string; atSeconds?: number | null }[]; readCoverage?: { readUntilSeconds: number; totalSeconds: number; batchCount: number; complete: boolean; reason: string } }
export type ChatStreaming = { question: string; answer: string; startedAt: number; activity?: string; activities: string[]; requestId?: string; reading?: boolean }
export type ChatSuggestion = { label: string; prompt: string }

export type ChatMessage = {
  id: string
  role: 'user' | 'agent'
  createdAt: string
  paragraphs: string[]
  toolSteps?: string[]
  run?: ChatRun
  suggestions?: ChatSuggestion[]
  points?: string[]
  citations?: ChatCitation[]
  frame?: ChatFrame
  outcome?: MessageOutcome
}

// Example conversation. The cited times correspond to recordFixtures in data.ts.
export const chatFixtures: ChatMessage[] = [
  {
    id: 'msg-001', role: 'user', createdAt: '2026-10-31T10:02:00+08:00',
    paragraphs: ['用 3 个点总结这节课讲数据结构的核心思路。'],
  },
  {
    id: 'msg-002', role: 'agent', createdAt: '2026-10-31T10:02:18+08:00',
    paragraphs: ['根据《CS50 第 4 讲：数据结构》的字幕摘录：'],
    points: [
      '数组连续存储，查询较快；插入时需要移动元素。',
      '哈希表以空间换时间，适合按键查找。',
      'Trie 按字符分层，适合前缀检索。',
    ],
    citations: [
      { recordId: 'rec-cs50-003', atSeconds: 492 },
      { recordId: 'rec-cs50-003', atSeconds: 1300 },
      { recordId: 'rec-cs50-003', atSeconds: 2225 },
    ],
  },
  {
    id: 'msg-003', role: 'user', createdAt: '2026-10-31T10:03:00+08:00',
    paragraphs: ['Trie 适合做什么？'],
  },
  {
    id: 'msg-004', role: 'agent', createdAt: '2026-10-31T10:03:12+08:00',
    paragraphs: ['字幕 37:05 提到，Trie 按字符分层，适合前缀检索。'],
    citations: [{ recordId: 'rec-cs50-003', atSeconds: 2225 }],
  },
]
