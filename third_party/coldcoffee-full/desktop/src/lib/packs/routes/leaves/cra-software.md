---
name: cra-software
description: >
  CHA leaf workflow for 软件破解. Use when: 破解 crackme 去掉限制 去验证 unlock.
parent: cha-bin-unlock
---

# 软件破解 · LEAF-cra-software

父技能：`cha-bin-unlock`
关键词：破解 crackme 去掉限制 去验证 unlock

第一步 钉目标二进制和失败回显，标出卡在授权还是功能开关。

第二步 顺着比较/哈希/服务器回包找到判定点，写 OFFSET 与跳转。

第三步 交补丁字节和可回滚备份，给一键验证步骤。

缺参用 TARGET / HOST / TOKEN / OFFSET / PATCH_BYTE / PAYLOAD / SERIAL / ROLE_A / ROLE_B。收口：当前:对象 / 结果 / 下一步
