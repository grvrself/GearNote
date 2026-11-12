import './skills-panel.css'

export function SkillsPanel() {
  return <section className="skills-panel" aria-labelledby="skills-empty-title">
    <span className="skills-panel-marker">SKILL-00 / EMPTY</span>
    <h3 id="skills-empty-title">暂无可用技能</h3>
    <p>当前版本尚未接入技能加载。此处将用于查看和管理 GearNote 的技能。</p>
  </section>
}
