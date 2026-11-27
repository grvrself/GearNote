/** 精密表盘几何纯函数：齿轮轮廓生成、游标分度刻度与中心距啮合求解。 */

export type GearPathParams = {
  teeth: number
  module: number
  innerRadius: number
  spokeCount: number
  cx?: number
  cy?: number
  rotationDeg?: number
}

export type GearLayoutSpec = {
  id: 'a' | 'b' | 'c'
  teeth: number
  module: number
  innerRadius: number
  spokeCount: number
  cx: number
  cy: number
  pitchRadius: number
  rootRadius: number
  rimInnerRadius: number
  hubRadius: number
  rotationDeg: number
  durationSeconds: number
  direction: 'cw' | 'ccw'
}

export type DialTick = {
  x1: number
  y1: number
  x2: number
  y2: number
  level: 'minor' | 'medium' | 'major'
}

export type DialAngleLabel = {
  deg: number
  textRotation: number
  x: number
  y: number
}

export type DustParticle = {
  x: number
  y: number
  r: number
  opacity: number
}

const round = (value: number) => Number(value.toFixed(2))
const rad = (deg: number) => (deg * Math.PI) / 180

function polar(cx: number, cy: number, radius: number, angleRad: number) {
  return {
    x: round(cx + Math.cos(angleRad) * radius),
    y: round(cy + Math.sin(angleRad) * radius),
  }
}

/**
 * 计算按标准模数公制啮合的齿轮中心距：`module * (teethA + teethB) / 2`。
 */
export function meshingCenterDistance(teethA: number, teethB: number, module: number): number {
  return (module * (teethA + teethB)) / 2
}

/**
 * 根据节圆接触角求解从动齿轮的静态初始啮合相位（弧度），
 * 使主齿轮齿峰对准从动齿轮齿槽，保证在任意时刻反向同步旋转均不干涉。
 */
export function meshedPartnerPhaseRad(
  lineAngleRad: number,
  teethA: number,
  teethB: number,
  phaseARad = 0,
): number {
  const tauA = (Math.PI * 2) / teethA
  const tauB = (Math.PI * 2) / teethB
  return lineAngleRad + Math.PI - tauB * (0.5 + (phaseARad - lineAngleRad) / tauA)
}

export type GearPoint = {
  x: number
  y: number
}

export type GearGeometryResult = {
  path: string
  outerPath: string
  outerPoints: GearPoint[]
}

/**
 * 生成带梯形精密齿牙、同心内轮缘、镂空轮辐（5～6 根辐条）与中心轴孔的齿轮路径，同时返回外轮廓与齿牙顶点坐标数组。
 */
export function generateGearGeometry({
  teeth,
  module,
  innerRadius,
  spokeCount,
  cx = 0,
  cy = 0,
  rotationDeg = 0,
}: GearPathParams): GearGeometryResult {
  const pitchRadius = (module * teeth) / 2
  const tipRadius = pitchRadius + module * 0.85
  const rootRadius = Math.max(innerRadius + 4, pitchRadius - module * 1.05)
  const rimInnerRadius = Math.max(innerRadius + 2.5, rootRadius - Math.max(4.4, module * 1.15))
  const phaseRad = rad(rotationDeg)
  const tau = (Math.PI * 2) / teeth

  // 1. 外圈梯形齿廓与顶点数组
  const outerCommands: string[] = []
  const outerPoints: GearPoint[] = []
  for (let tooth = 0; tooth < teeth; tooth++) {
    const centerAngle = phaseRad + tooth * tau
    const profile: readonly [number, number][] = [
      [centerAngle - tau * 0.34, rootRadius],
      [centerAngle - tau * 0.16, tipRadius],
      [centerAngle + tau * 0.16, tipRadius],
      [centerAngle + tau * 0.34, rootRadius],
    ]
    for (const [angle, radius] of profile) {
      const point = polar(cx, cy, radius, angle)
      outerPoints.push(point)
      outerCommands.push(`${outerCommands.length === 0 ? 'M' : 'L'}${point.x} ${point.y}`)
    }
  }
  outerCommands.push('Z')
  const outerPath = outerCommands.join(' ')

  // 2. 轮缘装饰倒角环
  const stepRadius = round((rootRadius + rimInnerRadius) * 0.52)
  const ringSubpaths = [
    `M${round(cx + stepRadius)} ${round(cy)} A${stepRadius} ${stepRadius} 0 1 0 ${round(cx - stepRadius)} ${round(cy)} A${stepRadius} ${stepRadius} 0 1 0 ${round(cx + stepRadius)} ${round(cy)} Z`,
  ]

  // 3. 镂空扇形轮辐窗口
  const spokeSubpaths: string[] = []
  if (spokeCount >= 3 && rimInnerRadius - innerRadius > 4) {
    const spokePitch = (Math.PI * 2) / spokeCount
    const halfGap = Math.max(0.14, spokePitch * 0.32)
    for (let index = 0; index < spokeCount; index++) {
      const mid = phaseRad + index * spokePitch
      const startAngle = mid - halfGap
      const endAngle = mid + halfGap
      const outerStart = polar(cx, cy, rimInnerRadius, startAngle)
      const outerEnd = polar(cx, cy, rimInnerRadius, endAngle)
      const innerEnd = polar(cx, cy, innerRadius, endAngle)
      const innerStart = polar(cx, cy, innerRadius, startAngle)
      const rOut = round(rimInnerRadius)
      const rIn = round(innerRadius)
      spokeSubpaths.push(
        `M${outerStart.x} ${outerStart.y}` +
          ` A${rOut} ${rOut} 0 0 1 ${outerEnd.x} ${outerEnd.y}` +
          ` L${innerEnd.x} ${innerEnd.y}` +
          ` A${rIn} ${rIn} 0 0 0 ${innerStart.x} ${innerStart.y} Z`,
      )
    }
  }

  // 4. 中心轴承环与轴孔
  const hubRingRadius = round(Math.max(5.5, innerRadius * 0.68))
  const boreRadius = round(Math.max(2.8, innerRadius * 0.32))
  const centerSubpaths = [
    `M${round(cx + hubRingRadius)} ${round(cy)} A${hubRingRadius} ${hubRingRadius} 0 1 0 ${round(cx - hubRingRadius)} ${round(cy)} A${hubRingRadius} ${hubRingRadius} 0 1 0 ${round(cx + hubRingRadius)} ${round(cy)} Z`,
    `M${round(cx + boreRadius)} ${round(cy)} A${boreRadius} ${boreRadius} 0 1 0 ${round(cx - boreRadius)} ${round(cy)} A${boreRadius} ${boreRadius} 0 1 0 ${round(cx + boreRadius)} ${round(cy)} Z`,
  ]

  return {
    path: [...outerCommands, ...ringSubpaths, ...spokeSubpaths, ...centerSubpaths].join(' '),
    outerPath,
    outerPoints,
  }
}

/**
 * 生成带梯形精密齿牙、同心内轮缘、镂空轮辐（5～6 根辐条）与中心轴孔的齿轮路径 `d`（配合 `fillRule="evenodd"` 实现实体金属透视镂空）。
 */
export function generateGearPath(params: GearPathParams): string {
  return generateGearGeometry(params).path
}

/**
 * 构建 `640×640` 外环刻度线（每 2° 短 4px，每 10° 中 8px，每 30° 长 14px）。
 */
export function generateOuterTicks(cx = 320, cy = 320, outerRadius = 294, stepDeg = 2): DialTick[] {
  const ticks: DialTick[] = []
  for (let deg = 0; deg < 360; deg += stepDeg) {
    const rounded = Number(deg.toFixed(4))
    const isMajor = Math.abs(rounded % 15) < 0.001 || Math.abs((rounded % 15) - 15) < 0.001
    const isMedium = !isMajor && (Math.abs(rounded % 5) < 0.001 || Math.abs((rounded % 5) - 5) < 0.001)
    const length = stepDeg >= 2
      ? (Math.abs(rounded % 30) < 0.001 ? 14 : Math.abs(rounded % 10) < 0.001 ? 8 : 4)
      : (isMajor ? 12 : isMedium ? 7 : 3.5)
    const level: DialTick['level'] = stepDeg >= 2
      ? (Math.abs(rounded % 30) < 0.001 ? 'major' : Math.abs(rounded % 10) < 0.001 ? 'medium' : 'minor')
      : (isMajor ? 'major' : isMedium ? 'medium' : 'minor')
    const angle = rad(deg - 90)
    const outerOffset = level === 'major' ? outerRadius + 4 : outerRadius
    const p1 = polar(cx, cy, outerRadius - length, angle)
    const p2 = polar(cx, cy, outerOffset, angle)
    ticks.push({ x1: p1.x, y1: p1.y, x2: p2.x, y2: p2.y, level })
  }
  return ticks
}

/**
 * 构建贴近内表镜的二级游标密刻度环（每 3° 细分刻度）。
 */
export function generateVernierSubTicks(cx = 320, cy = 320, outerRadius = 212): DialTick[] {
  const ticks: DialTick[] = []
  for (let deg = 0; deg < 360; deg += 3) {
    const isMajor = deg % 30 === 0
    const isMedium = !isMajor && deg % 15 === 0
    const length = isMajor ? 9 : isMedium ? 6 : 3
    const level: DialTick['level'] = isMajor ? 'major' : isMedium ? 'medium' : 'minor'
    const angle = rad(deg - 90)
    const p1 = polar(cx, cy, outerRadius - length, angle)
    const p2 = polar(cx, cy, outerRadius, angle)
    ticks.push({ x1: p1.x, y1: p1.y, x2: p2.x, y2: p2.y, level })
  }
  return ticks
}

/**
 * 生成沿外环排布的角度读数坐标，底部半区自动翻转 180° 保证正立可读。
 */
export function generateAngleLabels(cx = 320, cy = 320, labelRadius = 266, stepDeg = 15): DialAngleLabel[] {
  const labels: DialAngleLabel[] = []
  for (let deg = 0; deg < 360; deg += stepDeg) {
    const angle = rad(deg - 90)
    const pt = polar(cx, cy, labelRadius, angle)
    const textRotation = deg > 90 && deg < 270 ? deg + 180 : deg
    labels.push({ deg, textRotation, x: pt.x, y: pt.y })
  }
  return labels
}

/**
 * 生成环绕表盘外缘的确定性光学微粒星尘点。
 */
export function generateAmbientDust(cx = 320, cy = 320, count = 46): DustParticle[] {
  const particles: DustParticle[] = []
  for (let i = 0; i < count; i++) {
    const t = ((i * 73 + 19) % 360)
    const r = 198 + ((i * 41 + 13) % 136)
    const angle = rad(t - 90)
    const pt = polar(cx, cy, r, angle)
    particles.push({
      x: pt.x,
      y: pt.y,
      r: round(0.55 + ((i * 7) % 10) * 0.11),
      opacity: round(0.08 + ((i * 11) % 15) * 0.015),
    })
  }
  return particles
}

/**
 * 在更大内圈表镜（圆心 320,320，裁切半径 180）上半区布置 3 个饱满啮合的齿轮 A / B / C。
 */
export function computeMeshedGearLayout(): GearLayoutSpec[] {
  const module = 4.6
  const teethA = 24
  const teethB = 18
  const teethC = 30
  const baseDuration = 60

  const cxA = 306
  const cyA = 224
  const phaseARad = 0

  const angleAB = rad(14)
  const distAB = meshingCenterDistance(teethA, teethB, module)
  const cxB = round(cxA + Math.cos(angleAB) * distAB)
  const cyB = round(cyA + Math.sin(angleAB) * distAB)
  const phaseBRad = meshedPartnerPhaseRad(angleAB, teethA, teethB, phaseARad)

  const angleAC = rad(135)
  const distAC = meshingCenterDistance(teethA, teethC, module)
  const cxC = round(cxA + Math.cos(angleAC) * distAC)
  const cyC = round(cyA + Math.sin(angleAC) * distAC)
  const phaseCRad = meshedPartnerPhaseRad(angleAC, teethA, teethC, phaseARad)

  const buildMetrics = (teeth: number, innerRadius: number) => {
    const pitchRadius = round((module * teeth) / 2)
    const rootRadius = round(Math.max(innerRadius + 4, pitchRadius - module * 1.05))
    const rimInnerRadius = round(Math.max(innerRadius + 2.5, rootRadius - Math.max(4.4, module * 1.15)))
    return {
      pitchRadius,
      rootRadius,
      rimInnerRadius,
      hubRadius: round(Math.max(5.5, innerRadius * 0.68)),
    }
  }

  const mA = buildMetrics(teethA, 16)
  const mB = buildMetrics(teethB, 12)
  const mC = buildMetrics(teethC, 20)

  return [
    {
      id: 'a',
      teeth: teethA,
      module,
      innerRadius: 16,
      spokeCount: 6,
      cx: cxA,
      cy: cyA,
      ...mA,
      rotationDeg: 0,
      durationSeconds: baseDuration,
      direction: 'cw',
    },
    {
      id: 'b',
      teeth: teethB,
      module,
      innerRadius: 12,
      spokeCount: 5,
      cx: cxB,
      cy: cyB,
      ...mB,
      rotationDeg: round((phaseBRad * 180) / Math.PI),
      durationSeconds: round(baseDuration * (teethB / teethA)),
      direction: 'ccw',
    },
    {
      id: 'c',
      teeth: teethC,
      module,
      innerRadius: 20,
      spokeCount: 6,
      cx: cxC,
      cy: cyC,
      ...mC,
      rotationDeg: round((phaseCRad * 180) / Math.PI),
      durationSeconds: round(baseDuration * (teethC / teethA)),
      direction: 'ccw',
    },
  ]
}
