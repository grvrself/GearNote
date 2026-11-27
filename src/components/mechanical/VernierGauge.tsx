import './mechanical.css'

type Props = {
  label: string
  valueText: string
  value: number
  max: number
  hint?: string
}

export function VernierGauge({ label, valueText, value, max, hint }: Props) {
  const percent = max > 0 ? Math.min(100, Math.max(0, (value / max) * 100)) : 0
  return <div className="gn-vernier">
    <div className="gn-vernier-head">
      <span>{label}</span>
      <span className="gn-tabular">{valueText}</span>
    </div>
    <div className="gn-vernier-track" role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={max} aria-valuenow={value} aria-valuetext={valueText}>
      <div className="gn-ruler-divider" aria-hidden="true" />
      <div className="gn-vernier-fill" style={{ width: `${percent}%` }} aria-hidden="true">
        <i className="gn-vernier-cursor" />
      </div>
    </div>
    {hint && <small className="gn-vernier-hint">{hint}</small>}
  </div>
}