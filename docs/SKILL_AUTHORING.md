# GearNote 技能包作者指南

GearNote 的社区技能是**声明式工作流包**：一个 JSON Manifest 加一份 Markdown 提示词。技能包不包含、也不会执行 JavaScript、EXE、DLL、安装脚本或 Shell 命令。

## 最小仓库结构

```text
my-skill/
├─ gearnote-skill.json
└─ prompts/
   └─ compare.md
```

仓库根目录必须放置 `gearnote-skill.json`。安装时输入：

```text
https://github.com/<owner>/<repository>
```

GearNote 会通过 GitHub API 读取默认分支，再下载 Manifest 和提示词。下载、Schema 校验和权限校验全部通过后，才写入本机应用数据目录。

## Manifest 示例

```json
{
  "type": "gearnote-skill",
  "schemaVersion": 1,
  "id": "com.example.compare",
  "version": "0.1.0",
  "name": "多资料对比",
  "description": "比较不同资料的观点、证据与分歧。",
  "author": "example",
  "license": "MIT",
  "homepage": "https://github.com/example/gearnote-compare",
  "permissions": ["records.read.selected", "agent.chat", "chat.write"],
  "inputs": [{ "type": "records", "min": 2, "max": 4 }],
  "steps": [{ "action": "agent.ask", "prompt": "prompts/compare.md" }],
  "output": "chat.message"
}
```

## 字段约束

| 字段 | 约束 |
| --- | --- |
| `type` | 必须为 `gearnote-skill` |
| `schemaVersion` | 当前为 `1` |
| `id` | 小写字母、数字、点、下划线或连字符；建议反向域名 |
| `version` | 简单的 `major.minor.patch` 版本号 |
| `permissions` | 只能使用下方白名单，不允许空权限声明来隐藏行为 |
| `inputs` | 当前每项 `type` 必须为 `records`，`min` 与 `max` 为合法选择范围 |
| `steps` | 当前只允许一个 `agent.ask` 步骤 |
| `prompt` | 包内相对 `.md` 路径，不能是绝对路径，不能包含 `..` |
| `output` | 当前为 `chat.message` |

## 权限白名单

| 权限 | 含义 |
| --- | --- |
| `records.read.selected` | 读取用户主动选择的资料，不等于读取整个资料库 |
| `agent.chat` | 使用用户已配置的模型服务，可能产生 API 用量 |
| `chat.write` | 把模型回答写入当前会话 |

当前技能不能：

- 修改资料、笔记、设置或会话计划；
- 访问其他网络、任意文件或外部进程；
- 自动执行、自动更新或安装其他包；
- 声明任意域名、任意路径或 Shell 动作。

## 官方示例

本仓库的 [`packages/skills/compare`](../packages/skills/compare) 是可直接用于测试的示例包。安装仓库地址后，选择 2–4 份资料即可运行。

## 发布前检查

1. 使用真实仓库地址执行一次「从 GitHub 安装」。
2. 验证资料数量不足、超出上限和没有模型配置时的提示。
3. 检查回答中的来源编号是否指向本轮资料。
4. 明确许可证、作者、版本和权限。
5. 更新版本时保留旧版本，方便用户回滚；新增权限必须在文档和安装提示中说明。

不要把 API 密钥、Cookie、用户资料或真实私有数据提交到技能仓库。
