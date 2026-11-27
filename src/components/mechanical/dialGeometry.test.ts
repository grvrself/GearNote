import { describe, expect, it } from 'vitest'
import {
  computeMeshedGearLayout,
  generateAmbientDust,
  generateAngleLabels,
  generateGearPath,
  generateOuterTicks,
  generateVernierSubTicks,
  meshingCenterDistance,
} from './dialGeometry'

describe('dialGeometry 精密表盘几何', () => {
  it('generateGearPath 生成封闭齿廓、轮辐窗与中心孔径向路径', () => {
    const path = generateGearPath({ teeth: 24, module: 4.6, innerRadius: 16, spokeCount: 6, cx: 306, cy: 224 })
    expect(path.startsWith('M')).toBe(true)
    expect(path.endsWith('Z')).toBe(true)
    // 1 外齿廓 + 1 台阶环 + 6 扇形轮辐 + 2 轴承/轴孔 = 10 个闭合子路径
    expect(path.match(/Z/g)?.length).toBe(10)
    expect(path).not.toContain('NaN')
  })

  it('三齿轮组严格满足中心距公式并按真实齿数比计算反向旋转周期', () => {
    const gears = computeMeshedGearLayout()
    expect(gears).toHaveLength(3)
    const [a, b, c] = gears

    const distAB = Math.hypot(b.cx - a.cx, b.cy - a.cy)
    const distAC = Math.hypot(c.cx - a.cx, c.cy - a.cy)
    expect(distAB).toBeCloseTo(meshingCenterDistance(a.teeth, b.teeth, a.module), 1)
    expect(distAC).toBeCloseTo(meshingCenterDistance(a.teeth, c.teeth, a.module), 1)

    expect(a.direction).toBe('cw')
    expect(b.direction).toBe('ccw')
    expect(c.direction).toBe('ccw')
    expect(a.durationSeconds).toBe(60)
    expect(b.durationSeconds).toBe(45)
    expect(c.durationSeconds).toBe(75)
  })

  it('生成外环 180 刻度、内游标 120 细分刻度、正立角度标注与周围微粒', () => {
    const ticks = generateOuterTicks(320, 320, 294)
    const denseTicks = generateOuterTicks(395, 92, 224, 1.5)
    const subTicks = generateVernierSubTicks(320, 320, 212)
    const labels = generateAngleLabels(320, 320, 266, 15)
    const dust = generateAmbientDust(320, 320, 48)
    expect(ticks).toHaveLength(180)
    expect(denseTicks).toHaveLength(240)
    expect(ticks.filter(tick => tick.level === 'major')).toHaveLength(12)
    expect(ticks.filter(tick => tick.level === 'medium')).toHaveLength(24)
    expect(subTicks).toHaveLength(120)
    expect(labels).toHaveLength(24)
    expect(labels[0].deg).toBe(0)
    expect(labels.find(item => item.deg === 180)?.textRotation).toBe(360)
    expect(dust).toHaveLength(48)
  })
})
