import type { ReactNode } from 'react'
import type { Note } from '../notes'
import { GearIcon } from './GearIcon'

function inlineText(text: string): ReactNode[] {
  const parts = text.split('**')
  if (parts.length % 2 === 0) return [text]
  return parts.map((part, index) => index % 2 === 1 ? <strong key={index}>{part}</strong> : part)
}

function bodyBlocks(body: string): ReactNode[] {
  const lines = body.replace(/\r\n?/g, '\n').split('\n')
  const blocks: ReactNode[] = []
  let paragraph: string[] = []
  let section = 0
  const flush = () => {
    if (paragraph.length) {
      blocks.push(<p className="note-reader-paragraph" key={`p-${blocks.length}`}>{paragraph.map((line, index) => <span key={index}>{index > 0 && <br />}{inlineText(line)}</span>)}</p>)
      paragraph = []
    }
  }
  for (const raw of lines) {
    const line = raw.trim()
    if (!line) { flush(); continue }
    const heading = /^(#{1,3})\s+(.+)$/.exec(line)
    const claim = /^[-*]\s+(.+?)。「(.+)」\[(\d+)\]$/.exec(line)
    if (heading) {
      flush(); section += 1
      blocks.push(<h4 className="note-reader-heading" key={`h-${blocks.length}`}><span><GearIcon kind="brand" size={16} /> SEC-{String(section).padStart(2, '0')}</span>{inlineText(heading[2])}</h4>)
    } else if (claim) {
      flush()
      blocks.push(<div className="note-reader-claim" key={`c-${blocks.length}`}><p>{inlineText(claim[1])}</p><blockquote><span>笔记引文 [{claim[3]}]</span>{claim[2]}</blockquote></div>)
    } else if (/^[-*]\s+/.test(line) || /^\d+[.)、]\s*/.test(line)) {
      flush()
      blocks.push(<p className="note-reader-bullet" key={`b-${blocks.length}`}>{inlineText(line.replace(/^([-*]\s+|\d+[.)、]\s*)/, ''))}</p>)
    } else if (line.startsWith('> ')) {
      flush()
      blocks.push(<blockquote className="note-reader-quote" key={`q-${blocks.length}`}>{inlineText(line.slice(2))}</blockquote>)
    } else if (line === '---') {
      flush(); blocks.push(<hr key={`r-${blocks.length}`} />)
    } else {
      paragraph.push(line)
    }
  }
  flush()
  return blocks
}

export function NoteReader({ note }: { note: Note }) {
  return <article className="note-reader" aria-label={`笔记《${note.title}》正文`}>{bodyBlocks(note.body)}</article>
}
