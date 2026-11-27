import { describe, expect, it } from 'vitest'
import { formatToolVersion } from './format'

describe('formatToolVersion', () => {
  it('从 ffmpeg 完整版权串中提取简洁版本号', () => {
    expect(formatToolVersion('ffmpeg version 8.1.1-full_build-www.gyan.dev Copyright (c) 2000-2026 the FFmpeg developers')).toBe('v8.1.1')
  })

  it('从 ffprobe 构建串中提取简洁版本号', () => {
    expect(formatToolVersion('ffprobe version 8.1.1-full_build-www.gyan.dev')).toBe('v8.1.1')
  })

  it('保留已带 v 前缀的标准版本号', () => {
    expect(formatToolVersion('v24.15.0')).toBe('v24.15.0')
  })

  it('CalVer 日期版本不加 v 前缀', () => {
    expect(formatToolVersion('2026.10.26')).toBe('2026.10.26')
  })

  it('CalVer 日期带后缀时只保留日期部分', () => {
    expect(formatToolVersion('2026.10.26.232832')).toBe('2026.10.26')
  })

  it('两段版本号同样提取', () => {
    expect(formatToolVersion('tool 8.1 build')).toBe('v8.1')
  })

  it('先 trim 再解析', () => {
    expect(formatToolVersion('  v24.15.0  ')).toBe('v24.15.0')
    expect(formatToolVersion('  2026.10.26  ')).toBe('2026.10.26')
  })

  it('无版本号字符串原样返回（交给 CSS 省略号截断）', () => {
    expect(formatToolVersion('some build without numbers')).toBe('some build without numbers')
  })

  it('空串与纯空白安全返回空串', () => {
    expect(formatToolVersion('')).toBe('')
    expect(formatToolVersion('   ')).toBe('')
  })
})