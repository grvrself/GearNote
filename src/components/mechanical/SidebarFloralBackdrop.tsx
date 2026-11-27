import agentGothicArt from '../../assets/agent-gothic-art.jpg'
import { useThemeAndMotion } from './useThemeAndMotion'
import './mechanical.css'

/**
 * 简约黑白（mono）主题左侧边栏底部暗黑繁花暗纹：
 * - 复用 `agent-gothic-art.jpg`，通过 `object-position: left bottom` + `scale(1.18)` 仅裁切边缘繁花纹理；
 * - 严格仅在 `mono` 主题下挂载节点，不影响 `hud` 主题与上方会话列表交互（会话项保持 z-index: 1）。
 */
export function SidebarFloralBackdrop() {
  const { theme } = useThemeAndMotion()

  if (theme !== 'mono') {
    return null
  }

  return (
    <div className="sidebar-floral-backdrop" aria-hidden="true">
      <img src={agentGothicArt} alt="" draggable={false} />
    </div>
  )
}
