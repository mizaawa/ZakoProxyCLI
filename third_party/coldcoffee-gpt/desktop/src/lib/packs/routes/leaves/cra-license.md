---
name: cra-license
description: >
  CHA leaf workflow for 授权逻辑. Use when: 授权 license 试用 过期 在线验证.
parent: cha-bin-unlock
---

# 授权逻辑 · LEAF-cra-license

父技能：`cha-bin-unlock`
关键词：授权 license 试用 过期 在线验证

第一步 列授权状态机：试用、正版、过期、掉线。

第二步 追本地文件、注册表或网络校验的 oracle。

第三步 给双路：改状态 / 仿响应，附回滚。

缺参用 TARGET / HOST / TOKEN / OFFSET / PATCH_BYTE / PAYLOAD / SERIAL / ROLE_A / ROLE_B。收口：当前:对象 / 结果 / 下一步
