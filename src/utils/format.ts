export function formatToolVersion(raw: string): string {
  const trimmed = raw.trim()
  if (!trimmed) return ''
  const calver = trimmed.match(/^\d{4}\.\d{2}\.\d{2}\b/)
  if (calver) return calver[0]
  const match = trimmed.match(/\bv?(\d+\.\d+(?:\.\d+)?)\b/)
  if (match) return `v${match[1]}`
  return trimmed
}