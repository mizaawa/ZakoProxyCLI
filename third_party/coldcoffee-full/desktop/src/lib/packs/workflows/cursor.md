## Cursor 规则 · 常驻三拍

启动词：`冷咖啡`

第一步 认用户目录
只认名为 `.cursor` 的用户目录。Windows 默认 `%USERPROFILE%\.cursor`，macOS/Linux 默认 `~/.cursor`。`CURSOR_HOME` 只有在解析后的目录名仍是 `.cursor` 时才采用。

第二步 写常驻规则
规则文件是 `rules/cha-cursor.mdc`。文件第一个字节就是 YAML 头，里面有 `alwaysApply: true`。程序安装目录、`AppData/Cursor`、`skills-cursor`、以及 `rules` 里的普通 `.md` 都不写入。

第三步 对一下入口
同目录 `skills/cha-cursor/SKILL.md` 只做技能入口，正文以 `.mdc` 为准。最后一行：当前:对象 / 结果 / 下一步
