## 豆包技能 · 落点三拍

第一步 认目录
默认目录按平台选择：Windows `%LOCALAPPDATA%\Doubao\User Data\Default\.doubao\agent_mode\workspace\.user_skills`，macOS `~/Library/Application Support/Doubao/User Data/Default/.doubao/agent_mode/workspace/.user_skills`，Linux `$XDG_DATA_HOME/Doubao/User Data/Default/.doubao/agent_mode/workspace/.user_skills`（未设置时 `~/.local/share`）。环境变量 `DOUBAO_USER_SKILLS` 可以改这一个目录。

第二步 放技能
写入 `cha-doubao/SKILL.md`。`name` 是 `cha-doubao`。不要把文件放进并列的 `.skills`。

第三步 对路径
预览里的根目录必须以 `.user_skills` 结尾，目标文件只有 `cha-doubao/SKILL.md`。确认后再写入。结束行：当前:对象 / 结果 / 下一步
