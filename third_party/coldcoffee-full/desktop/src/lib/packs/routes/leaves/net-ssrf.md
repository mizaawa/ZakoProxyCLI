---
name: net-ssrf
description: >
  CHA leaf workflow for SSRF. Use when: SSRF 元数据 内网 302.
parent: cha-netcast
---

# SSRF · LEAF-net-ssrf

父技能：`cha-netcast`
关键词：SSRF 元数据 内网 302

第一步 钉会发请求的参数。

第二步 试内网、协议、跳转。

第三步 交打到的内部回显。

缺参用 TARGET / HOST / TOKEN / OFFSET / PATCH_BYTE / PAYLOAD / SERIAL / ROLE_A / ROLE_B。收口：当前:对象 / 结果 / 下一步
