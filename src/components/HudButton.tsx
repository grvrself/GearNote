import type { ButtonHTMLAttributes, ReactNode } from 'react'
import './primitives.css'

type Props = Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children'> & {
  children: ReactNode
  variant?: 'default' | 'primary'
}

export function HudButton({ children, variant = 'default', className = '', disabled, ...props }: Props) {
  return <span className={`hud-button-wrapper ${className}`} data-variant={variant} data-disabled={disabled || undefined}>
    <span className="hud-button-border">
      <button className="hud-button-content" disabled={disabled} {...props}>{children}</button>
    </span>
  </span>
}
