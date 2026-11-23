import type { CSSProperties } from 'react'

function gearPoints(teeth: number) {
  const points: string[] = []
  for (let tooth = 0; tooth < teeth; tooth++) {
    for (const [part, radius] of [[0, 12.5], [.16, 15.2], [.56, 15.2], [.72, 12.5]] as const) {
      const angle = (tooth + part) * Math.PI * 2 / teeth - Math.PI / 2
      points.push(`${(16 + Math.cos(angle) * radius).toFixed(2)},${(16 + Math.sin(angle) * radius).toFixed(2)}`)
    }
  }
  return points.join(' ')
}

type GearProps = {
  teeth?: number
  className?: string
  speed?: string
  reverse?: boolean
}

export function MechanicalGear({ teeth = 12, className = '', speed = '12s', reverse = false }: GearProps) {
  return <span className={`mech-gear ${reverse ? 'mech-reverse' : ''} ${className}`} style={{ '--mech-speed': speed } as CSSProperties} aria-hidden="true">
    <svg viewBox="0 0 32 32" fill="none" stroke="currentColor" strokeWidth="1.05" focusable="false">
      <polygon points={gearPoints(teeth)} strokeLinejoin="round" /><circle cx="16" cy="16" r="8.2" /><circle cx="16" cy="16" r="2.1" />
    </svg>
  </span>
}

const trainGears = [
  { className: 'g0', teeth: 20, speed: '26s', reverse: false },
  { className: 'g1', teeth: 14, speed: '12s', reverse: true },
  { className: 'g2', teeth: 16, speed: '17s', reverse: true },
  { className: 'g3', teeth: 12, speed: '10s', reverse: false },
  { className: 'g4', teeth: 12, speed: '8s', reverse: true },
]

export function MechanicalTrain() {
  return <div className="mech-train" aria-hidden="true">
    <div className="mech-core-ring" />
    {trainGears.map(gear => <MechanicalGear key={gear.className} className={`mech-train-gear ${gear.className}`} teeth={gear.teeth} speed={gear.speed} reverse={gear.reverse} />)}
    <span className="mech-rpm">TORQUE 042 / LINK 05</span>
  </div>
}
