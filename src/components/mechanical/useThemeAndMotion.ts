import { useEffect, useState } from 'react'
import { readMotion, readTheme, type MotionPreference, type Theme } from '../../theme'

/**
 * 订阅应用内主题与三档动效切换（knowflow:theme / knowflow:motion / storage），
 * 供装饰性组件实现严格主题挂载隔离与实时动效联动。
 */
export function useThemeAndMotion() {
  const [theme, setTheme] = useState<Theme>(readTheme)
  const [motion, setMotion] = useState<MotionPreference>(readMotion)

  useEffect(() => {
    const syncTheme = () => setTheme(readTheme())
    const onTheme = (event: Event) => {
      const detail = (event as CustomEvent<Theme>).detail
      setTheme(detail ?? readTheme())
    }
    const syncMotion = () => setMotion(readMotion())
    const onMotion = (event: Event) => {
      const detail = (event as CustomEvent<MotionPreference>).detail
      setMotion(detail ?? readMotion())
    }
    const onStorage = () => {
      syncTheme()
      syncMotion()
    }

    window.addEventListener('knowflow:theme', onTheme)
    window.addEventListener('knowflow:motion', onMotion)
    window.addEventListener('storage', onStorage)
    return () => {
      window.removeEventListener('knowflow:theme', onTheme)
      window.removeEventListener('knowflow:motion', onMotion)
      window.removeEventListener('storage', onStorage)
    }
  }, [])

  return { theme, motion }
}
