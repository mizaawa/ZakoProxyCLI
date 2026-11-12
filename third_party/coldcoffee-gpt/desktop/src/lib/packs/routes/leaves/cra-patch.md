---
name: cra-patch
description: >
  CHA leaf workflow for 补丁制作. Use when: 补丁 patch 改字节 JNZ JMP NOP.
parent: cha-bin-unlock
---

# 补丁制作 · LEAF-cra-patch

父技能：`cha-bin-unlock`
关键词：补丁 patch 改字节 JNZ JMP NOP

第一步 钉要改的指令与原字节。

第二步 写 PATCH_BYTE 表，避开完整性哈希或一并改哈希。

第三步 交补丁器和备份、校验哈希。

缺参用 TARGET / HOST / TOKEN / OFFSET / PATCH_BYTE / PAYLOAD / SERIAL / ROLE_A / ROLE_B。收口：当前:对象 / 结果 / 下一步
