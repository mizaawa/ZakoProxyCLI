---
name: cra-pe
description: >
  CHA leaf workflow for PE 分析. Use when: PE 节表 IAT DOS NT 头.
parent: cha-bin-unlock
---

# PE 分析 · LEAF-cra-pe

父技能：`cha-bin-unlock`
关键词：PE 节表 IAT DOS NT 头

第一步 读 DOS/NT 头、节、导入导出。

第二步 标入口、TLS、重定位异常点。

第三步 交 PE 摘要和下手偏移。

缺参用 TARGET / HOST / TOKEN / OFFSET / PATCH_BYTE / PAYLOAD / SERIAL / ROLE_A / ROLE_B。收口：当前:对象 / 结果 / 下一步
