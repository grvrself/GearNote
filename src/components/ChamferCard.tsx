import type { PropsWithChildren } from 'react'
import './primitives.css'

type Props = PropsWithChildren<{ className?: string; tone?: 'dark' | 'accent'; id?: string }>

export function ChamferCard({ children, className = '', tone = 'dark', id }: Props) {
  return <div id={id} className={`chamfer-wrapper ${className}`} data-tone={tone}>
    <div className="chamfer-border">
      <div className="chamfer-content">{children}</div>
    </div>
  </div>
}
