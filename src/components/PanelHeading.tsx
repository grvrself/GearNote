import './primitives.css'

type Props = { id: string; serial: string; overline: string; title: string; count?: number }

export function PanelHeading({ id, serial, overline, title, count }: Props) {
  return <div className="panel-heading" id={id}>
    <div className="panel-heading-serial">{serial} / {overline}</div>
    <div className="panel-heading-title"><h2>{title}</h2>{count != null && <span>{String(count).padStart(2, '0')}</span>}</div>
  </div>
}
