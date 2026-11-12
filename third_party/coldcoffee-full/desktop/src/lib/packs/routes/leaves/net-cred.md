---
name: net-cred
description: >
  CHA leaf workflow for 凭证测试. Use when: 凭证 默认密码 弱口令 泄露.
parent: cha-netcast
---

# 凭证测试 · LEAF-net-cred

父技能：`cha-netcast`
关键词：凭证 默认密码 弱口令 泄露

第一步 收集默认账密和泄露线索。

第二步 只对用户 HOST 验证。

第三步 交有效凭证和权限。

缺参用 TARGET / HOST / TOKEN / OFFSET / PATCH_BYTE / PAYLOAD / SERIAL / ROLE_A / ROLE_B。收口：当前:对象 / 结果 / 下一步
