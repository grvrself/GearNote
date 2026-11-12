import { useId } from 'react'

type GearKind = 'brand' | 'divider' | 'active' | 'working' | 'complete' | 'conversation' | 'workspace' | 'center' | 'sidebar'

function teethPoints(teeth: number, root: number, tip: number) {
  const points: string[] = []
  for (let tooth = 0; tooth < teeth; tooth++) {
    for (const [part, radius] of [[0, root], [.16, tip], [.56, tip], [.72, root]] as const) {
      const angle = (tooth + part) * Math.PI * 2 / teeth - Math.PI / 2
      points.push(`${(16 + Math.cos(angle) * radius).toFixed(2)},${(16 + Math.sin(angle) * radius).toFixed(2)}`)
    }
  }
  return points.join(' ')
}

function spokeWindow(rotation: number) {
  const point = (radius: number, offset: number) => {
    const angle = (rotation + offset - 90) * Math.PI / 180
    return `${(16 + Math.cos(angle) * radius).toFixed(2)} ${(16 + Math.sin(angle) * radius).toFixed(2)}`
  }
  return `M${point(5.6, -28)} L${point(10.1, -28)} A10.1 10.1 0 0 1 ${point(10.1, 28)} L${point(5.6, 28)} A5.6 5.6 0 0 0 ${point(5.6, -28)} Z`
}

export function GearIcon({ kind, size = 16 }: { kind: GearKind; size?: 16 | 20 }) {
  const maskId = useId()
  const center = kind === 'center'
  const sidebar = kind === 'sidebar'
  return <svg className={`gear-icon gear-icon-${kind}`} width={size} height={size} viewBox="0 0 32 32" fill="currentColor" aria-hidden="true" focusable="false">
    <defs><mask id={maskId} maskUnits="userSpaceOnUse" x="0" y="0" width="32" height="32">
      <polygon points={center ? teethPoints(20, 12.5, 15.4) : sidebar ? teethPoints(16, 12.7, 15.4) : teethPoints(12, 12.4, 15.4)} fill="white" />
      {center ? <>
        <circle cx="16" cy="16" r="10.4" fill="none" stroke="black" strokeWidth=".9" />
        <circle cx="16" cy="16" r="3.4" fill="black" />
        <circle cx="16" cy="16" r="2.9" fill="white" />
        <circle cx="16" cy="16" r="2.4" fill="black" />
      </> : sidebar ? <>
        {[0, 90, 180, 270].map(rotation => <path key={rotation} d={spokeWindow(rotation)} fill="black" />)}
        <circle cx="16" cy="16" r="3.3" fill="black" />
      </> : <circle cx="16" cy="16" r="8.7" fill="black" />}
    </mask></defs>
    <rect width="32" height="32" mask={`url(#${maskId})`} />
  </svg>
}

export function GearDivider({ area }: { area: 'history' | 'composer' }) {
  return <div className={`gear-divider gear-divider-${area}`} aria-hidden="true"><span /><GearIcon kind={area === 'history' ? 'brand' : 'center'} size={20} /><span /></div>
}

export function PaperPlaneIcon() {
  return <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" aria-hidden="true" focusable="false"><path d="M1.5 7.2 14 2 9.3 14l-2.1-4.7L1.5 7.2Z" /><path d="M7.2 9.3 14 2" /></svg>
}
