import './mechanical.css'

type Props = {
  className?: string
}

export function CrosshairFrame({ className = '' }: Props) {
  return <div className={`gn-crosshair-frame ${className}`.trim()} aria-hidden="true">
    <i data-corner="tl" />
    <i data-corner="tr" />
    <i data-corner="bl" />
    <i data-corner="br" />
  </div>
}