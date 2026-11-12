---
name: net-priv
description: >
  CHA leaf workflow for 权限测试. Use when: 越权 水平 垂直 IDOR.
parent: cha-netcast
---

# 权限测试 · LEAF-net-priv

父技能：`cha-netcast`
关键词：越权 水平 垂直 IDOR

第一步 钉两个角色 TOKEN。

第二步 换 id、换对象重放。

第三步 交越权对照表。

缺参用 TARGET / HOST / TOKEN / OFFSET / PATCH_BYTE / PAYLOAD / SERIAL / ROLE_A / ROLE_B。收口：当前:对象 / 结果 / 下一步
