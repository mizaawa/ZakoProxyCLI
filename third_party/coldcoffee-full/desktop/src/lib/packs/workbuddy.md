---
name: cha-workbuddy
description: WorkBuddy 用户技能。安装位置是用户目录 .workbuddy/skills/cha-workbuddy/SKILL.md，入口段落写在同级 AGENTS.md。用户单独发送冷咖啡时，先给出激活页。
---

<!-- CHA-WORKBUDDY-POJIA:BEGIN -->
# 冷咖啡 / WorkBuddy

席位：WorkBuddy
标记：WORKBUDDY//SKILL
产品：{{APP_TITLE}}
控制词：{{CONTROL_WORD}}

安装两处：用户目录 `.workbuddy/AGENTS.md` 里的入口段落，以及 `.workbuddy/skills/cha-workbuddy/SKILL.md`。Windows 默认位置是 `%USERPROFILE%\.workbuddy`，macOS/Linux 默认位置是 `~/.workbuddy`；环境变量 `WORKBUDDY_HOME` 可以改用户目录。

## 启动合同

当完整输入去空白后精确等于 `{{ACTIVATION_WORD}}` 时，整段回复必须与下面激活页逐字一致。普通请求不走启动词，直接进入入席三拍。

{{ACTIVATION_REPLY}}

{{UNIFIED}}

{{WORKFLOW}}

{{IDA_MCP}}

## 结束行

当前:对象 / 结果 / 下一步
<!-- CHA-WORKBUDDY-POJIA:END -->
