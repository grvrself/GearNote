import { invoke } from '@tauri-apps/api/core'

export type SkillPermission = 'records.read.selected' | 'agent.chat' | 'chat.write'
export type SkillInput = { type: 'records'; min: number; max: number }
export type SkillStep = { action: 'agent.ask'; prompt: string }
export type SkillManifest = {
  type: 'gearnote-skill'
  schemaVersion: number
  id: string
  version: string
  name: string
  description: string
  author: string
  license: string
  homepage: string
  permissions: SkillPermission[]
  inputs: SkillInput[]
  steps: SkillStep[]
  output: 'chat.message'
}

export const installSkillFromGithub = (url: string) => invoke<SkillManifest>('install_skill_from_github', { url })
export const listInstalledSkills = () => invoke<SkillManifest[]>('list_installed_skills')
export const getSkillPrompt = (id: string, version: string) => invoke<string>('get_skill_prompt', { id, version })
export const uninstallSkill = (id: string, version: string) => invoke<void>('uninstall_skill', { id, version })

export type ToolStatus = { name: string; found: boolean; path: string; version: string; error: string }
export type WorkspaceInfo = { dataDir: string; cacheDir: string; databaseBytes: number; cacheBytes: number }
export const getToolchainStatus = () => invoke<ToolStatus[]>('get_toolchain_status')
export const getWorkspaceInfo = () => invoke<WorkspaceInfo>('get_workspace_info')
export const openWorkspacePath = (kind: 'data' | 'cache' | 'exports') => invoke<void>('open_workspace_path', { kind })
