export function isCaptureCommand(question: string): boolean {
  return /^(?:请|帮我|给我)?\s*(?:截取|截图|\/截图)/.test(question.trim())
}
