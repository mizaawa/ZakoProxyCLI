---
name: net-cve
description: >
  CHA leaf workflow for CVE 验证. Use when: CVE POC 复现 版本.
parent: cha-netcast
---

# CVE 验证 · LEAF-net-cve

父技能：`cha-netcast`
关键词：CVE POC 复现 版本

第一步 钉版本和 CVE 号。

第二步 按指纹选 PoC 复现。

第三步 交是否打中、回显、补丁建议。

缺参用 TARGET / HOST / TOKEN / OFFSET / PATCH_BYTE / PAYLOAD / SERIAL / ROLE_A / ROLE_B。收口：当前:对象 / 结果 / 下一步
