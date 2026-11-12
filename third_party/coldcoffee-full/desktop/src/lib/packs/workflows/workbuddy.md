## WorkBuddy 技能 · 入席三拍

第一步 认目录
用户目录默认是 `~/.workbuddy`（Windows 为 `%USERPROFILE%\.workbuddy`），也可用环境变量 `WORKBUDDY_HOME` 指定。技能放在这个目录的 `skills` 下，不放到项目里的 `.workbuddy`。

第二步 放技能
写入 `skills/cha-workbuddy/SKILL.md`。`name` 是 `cha-workbuddy`。

第三步 写入口
在同一用户目录的 `AGENTS.md` 写入冷咖啡入口段落，指向这个技能文件。结束行：当前:对象 / 结果 / 下一步
