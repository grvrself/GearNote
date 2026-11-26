import { createElement, useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { ConfirmDialog, type ConfirmOptions } from '../components/ConfirmDialog'

// 每个组件本地持有确认弹窗，独立渲染的单测无需全局 Provider。
export function useConfirm() {
  const [options, setOptions] = useState<ConfirmOptions | null>(null)
  const resolveRef = useRef<((value: boolean) => void) | null>(null)

  const settle = useCallback((value: boolean) => {
    resolveRef.current?.(value)
    resolveRef.current = null
    setOptions(null)
  }, [])

  const confirm = useCallback((next: ConfirmOptions) => {
    resolveRef.current?.(false)
    return new Promise<boolean>(resolve => {
      resolveRef.current = resolve
      setOptions(next)
    })
  }, [])

  useEffect(() => () => { resolveRef.current?.(false); resolveRef.current = null }, [])

  const confirmDialog: ReactNode = options
    ? createElement(ConfirmDialog, { ...options, onConfirm: () => settle(true), onCancel: () => settle(false) })
    : null
  return { confirm, confirmDialog }
}